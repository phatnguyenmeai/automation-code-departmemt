use crate::lane_queue::{recv_prioritized, LaneQueue, LaneSender};
use crate::resume::ResumeState;
use crate::session::Session;
use crate::store::FsArtifactStore;
use crate::transcript::{self, TranscriptHandle};
use agent_core::{
    Agent, AgentCtx, AgentOutput, ArtifactStore, Dispatcher, Result as AgentResult, Role,
    TaskMessage,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

const META_FILENAME: &str = "meta.json";
const TRANSCRIPT_FILENAME: &str = "transcript.jsonl";

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: Uuid,
    pub workspace_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub parent_session_id: Option<Uuid>,
}

/// Workspace bundles config + session state for one orchestration run.
pub struct Workspace {
    pub id: String,
    pub session: Session,
    pub run_dir: PathBuf,
    pub artifacts: Arc<FsArtifactStore>,
    pub transcript: TranscriptHandle,
    pub transcript_join: JoinHandle<()>,
    pub resume_state: Option<ResumeState>,
}

impl Workspace {
    /// Create a brand-new session: mint a uuid, create `runs/<id>/`, init
    /// the transcript writer, write `meta.json`.
    pub async fn new_run(workspace_id: impl Into<String>, runs_dir: &Path) -> std::io::Result<Self> {
        let workspace_id = workspace_id.into();
        let session_id = Uuid::new_v4();
        let run_dir = runs_dir.join(session_id.to_string());
        tokio::fs::create_dir_all(&run_dir).await?;

        let meta = SessionMeta {
            session_id,
            workspace_id: workspace_id.clone(),
            created_at: Utc::now(),
            parent_session_id: None,
        };
        tokio::fs::write(
            run_dir.join(META_FILENAME),
            serde_json::to_vec_pretty(&meta).unwrap(),
        )
        .await?;

        let (handle, join) = transcript::spawn(run_dir.join(TRANSCRIPT_FILENAME));
        let artifacts = Arc::new(FsArtifactStore::new(&run_dir));
        let session = Session::new(session_id, workspace_id.clone(), run_dir.clone(), handle.clone());

        Ok(Self {
            id: workspace_id,
            session,
            run_dir,
            artifacts,
            transcript: handle,
            transcript_join: join,
            resume_state: None,
        })
    }

    /// Resume from an existing `runs/<session_id>/` directory. Loads
    /// transcript into a `ResumeState` and opens transcript writer in
    /// append mode so new entries accumulate after existing ones.
    pub async fn resume(workspace_id: impl Into<String>, run_dir: PathBuf) -> std::io::Result<Self> {
        let workspace_id = workspace_id.into();
        let meta_bytes = tokio::fs::read(run_dir.join(META_FILENAME)).await?;
        let meta: SessionMeta = serde_json::from_slice(&meta_bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if meta.workspace_id != workspace_id {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "workspace mismatch: run was {}, config says {}",
                    meta.workspace_id, workspace_id
                ),
            ));
        }

        let transcript_path = run_dir.join(TRANSCRIPT_FILENAME);
        let resume_state = crate::resume::load(&transcript_path).await?;
        tracing::info!(
            session_id = %meta.session_id,
            entries = resume_state.entries.len(),
            in_flight = resume_state.in_flight.len(),
            "resume state loaded"
        );

        let (handle, join) = transcript::spawn(&transcript_path);
        let artifacts = Arc::new(FsArtifactStore::new(&run_dir));
        let session = Session::new(
            meta.session_id,
            workspace_id.clone(),
            run_dir.clone(),
            handle.clone(),
        );

        Ok(Self {
            id: workspace_id,
            session,
            run_dir,
            artifacts,
            transcript: handle,
            transcript_join: join,
            resume_state: Some(resume_state),
        })
    }
}

/// Cloneable dispatcher backed by per-role LaneSenders.
#[derive(Clone)]
pub struct GatewayDispatcher {
    senders: Arc<HashMap<Role, LaneSender>>,
    session: Session,
}

#[async_trait]
impl Dispatcher for GatewayDispatcher {
    async fn dispatch(&self, msg: TaskMessage) -> AgentResult<()> {
        self.session.record(&msg);
        let sender = self
            .senders
            .get(&msg.to)
            .ok_or_else(|| agent_core::AgentError::Other(format!("no lane for {}", msg.to)))?;
        sender
            .send(msg)
            .map_err(|e| agent_core::AgentError::Other(format!("dispatch: {e}")))?;
        Ok(())
    }
}

/// Terminal signal emitted when PM produces a FinalReport.
pub type FinalSignal = mpsc::UnboundedReceiver<serde_json::Value>;

pub struct Gateway {
    workspace: Workspace,
    queue: LaneQueue,
    senders: HashMap<Role, LaneSender>,
    final_tx: mpsc::UnboundedSender<serde_json::Value>,
    final_rx: Option<FinalSignal>,
}

impl Gateway {
    pub fn new(workspace: Workspace) -> Self {
        let queue = LaneQueue::new();
        let mut senders = HashMap::new();
        for r in Role::all() {
            senders.insert(*r, queue.sender(*r));
        }
        let (final_tx, final_rx) = mpsc::unbounded_channel();
        Self {
            workspace,
            queue,
            senders,
            final_tx,
            final_rx: Some(final_rx),
        }
    }

    pub fn sender(&self, role: Role) -> LaneSender {
        self.senders.get(&role).expect("role").clone()
    }

    pub fn dispatcher(&self) -> Arc<dyn Dispatcher> {
        Arc::new(GatewayDispatcher {
            senders: Arc::new(self.senders.clone()),
            session: self.workspace.session.clone(),
        })
    }

    pub fn take_final_rx(&mut self) -> FinalSignal {
        self.final_rx.take().expect("final rx already taken")
    }

    pub fn run_dir(&self) -> &Path {
        &self.workspace.run_dir
    }

    pub fn session_id(&self) -> Uuid {
        self.workspace.session.id
    }

    pub fn resume_state(&self) -> Option<&ResumeState> {
        self.workspace.resume_state.as_ref()
    }

    /// Take ownership of the transcript join handle so the caller can await
    /// flush-on-shutdown.
    pub fn take_transcript_join(&mut self) -> Option<JoinHandle<()>> {
        // Replace with a no-op completed handle
        let noop = tokio::spawn(async {});
        Some(std::mem::replace(&mut self.workspace.transcript_join, noop))
    }

    /// Spawn one worker task per agent. Returns join handles.
    pub fn spawn_workers(&mut self, agents: Vec<Box<dyn Agent>>) -> Vec<JoinHandle<()>> {
        let artifacts: Arc<dyn ArtifactStore> = self.workspace.artifacts.clone();
        let ctx = AgentCtx {
            workspace_id: self.workspace.id.clone(),
            session_id: self.workspace.session.id,
            run_dir: self.workspace.run_dir.clone(),
            dispatch: self.dispatcher(),
            artifacts,
        };
        let session = self.workspace.session.clone();
        let final_tx = self.final_tx.clone();

        let mut handles = Vec::new();
        for agent in agents {
            let role = agent.role();
            let mut lane = self.queue.take_lane(role);
            let (mut hr, mut nr, mut lr) = lane.take_receivers();
            let agent = Arc::new(Mutex::new(agent));
            let ctx = ctx.clone();
            let session = session.clone();
            let final_tx = final_tx.clone();
            let dispatcher = self.dispatcher();

            let handle = tokio::spawn(async move {
                tracing::info!(role = %role, "agent worker started");
                while let Some(msg) = recv_prioritized(&mut hr, &mut nr, &mut lr).await {
                    tracing::info!(role = %role, from = %msg.from, kind = ?msg.kind, "handling");
                    let result = {
                        let mut agent = agent.lock().await;
                        agent.handle(msg.clone(), &ctx).await
                    };
                    match result {
                        Ok(AgentOutput::Dispatch(outs)) => {
                            for out in outs {
                                if let Err(e) = dispatcher.dispatch(out).await {
                                    tracing::error!(?e, "dispatch failed");
                                }
                            }
                        }
                        Ok(AgentOutput::Done(payload)) => {
                            tracing::info!(role = %role, "done");
                            let _ = final_tx.send(payload);
                        }
                        Ok(AgentOutput::Blocked(reason)) => {
                            tracing::warn!(role = %role, reason, "blocked");
                            // Synthesize a blocker artifact + message to PM.
                            let blocker_id = agent_core::TaskId::new();
                            let blocker_ref = match ctx
                                .artifacts
                                .write(
                                    role,
                                    "blocker",
                                    msg.id,
                                    blocker_id,
                                    &serde_json::json!({"reason": reason}),
                                    &format!("# Blocker from {role}\n\n{reason}\n"),
                                )
                                .await
                            {
                                Ok(r) => r,
                                Err(e) => {
                                    tracing::error!(?e, "blocker artifact write failed");
                                    let _ = session; // keep session alive for borrowck
                                    continue;
                                }
                            };
                            let blocker = msg.reply(
                                role,
                                Role::PM,
                                agent_core::TaskKind::Blocker,
                                blocker_ref,
                                serde_json::json!({"reason": reason}),
                            );
                            let _ = dispatcher.dispatch(blocker).await;
                        }
                        Err(e) => {
                            tracing::error!(?e, role = %role, "agent error");
                            let blocker_id = agent_core::TaskId::new();
                            let err_text = e.to_string();
                            let blocker_ref = match ctx
                                .artifacts
                                .write(
                                    role,
                                    "blocker",
                                    msg.id,
                                    blocker_id,
                                    &serde_json::json!({"error": err_text}),
                                    &format!("# Error from {role}\n\n```\n{err_text}\n```\n"),
                                )
                                .await
                            {
                                Ok(r) => r,
                                Err(e) => {
                                    tracing::error!(?e, "blocker artifact write failed");
                                    continue;
                                }
                            };
                            let blocker = msg.reply(
                                role,
                                Role::PM,
                                agent_core::TaskKind::Blocker,
                                blocker_ref,
                                serde_json::json!({"error": err_text}),
                            );
                            let _ = dispatcher.dispatch(blocker).await;
                        }
                    }
                }
                tracing::info!(role = %role, "agent worker exiting");
            });
            handles.push(handle);
        }
        handles
    }
}
