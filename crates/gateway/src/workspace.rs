use crate::lane_queue::{recv_prioritized, LaneQueue, LaneSender};
use crate::session::Session;
use agent_core::{
    Agent, AgentCtx, AgentOutput, ContextAssembly, Dispatcher, Result as AgentResult, Role,
    TaskMessage,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use storage::Storage;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

/// Workspace bundles config + session state for one orchestration run.
pub struct Workspace {
    pub id: String,
    pub session: Session,
}

impl Workspace {
    /// Create a new workspace with an ephemeral (in-memory only) session.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        let session = Session::new(id.clone());
        Self { id, session }
    }

    /// Create a new workspace backed by persistent storage.
    ///
    /// Follows OpenClaw's pattern: every session is persisted so it can be
    /// inspected later or resumed after interruption.
    pub fn with_storage(id: impl Into<String>, storage: Arc<dyn Storage>) -> Self {
        let id = id.into();
        let session = Session::with_storage(id.clone(), storage);
        Self { id, session }
    }

    /// Resume an existing session from storage.
    ///
    /// Loads the session metadata and full message history from the storage
    /// backend, allowing a previously interrupted pipeline to be inspected
    /// or continued.
    pub async fn resume(
        session_id: uuid::Uuid,
        storage: Arc<dyn Storage>,
    ) -> Result<Self, storage::StorageError> {
        let session = Session::resume(session_id, storage).await?;
        let id = session.workspace_id.clone();
        Ok(Self { id, session })
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

/// Terminal signal emitted when PM produces a FinalReport or all agents idle.
pub type FinalSignal = mpsc::UnboundedReceiver<serde_json::Value>;

pub struct Gateway {
    workspace: Workspace,
    queue: LaneQueue,
    senders: HashMap<Role, LaneSender>,
    final_tx: mpsc::UnboundedSender<serde_json::Value>,
    final_rx: Option<FinalSignal>,
    /// Optional memory-aware context assembler (OpenClaw-style).
    assembler: Option<Arc<dyn ContextAssembly>>,
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
            assembler: None,
        }
    }

    /// Set a memory-aware context assembler (OpenClaw-style memory management).
    ///
    /// When set, agents will receive this assembler via `AgentCtx` and can
    /// use it to build prompts with recalled conversation history.
    pub fn with_assembler(mut self, assembler: Arc<dyn ContextAssembly>) -> Self {
        self.assembler = Some(assembler);
        self
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

    pub fn session(&self) -> Session {
        self.workspace.session.clone()
    }

    /// Spawn one worker task per agent. Returns join handles.
    pub fn spawn_workers(
        &mut self,
        agents: Vec<Box<dyn Agent>>,
    ) -> Vec<JoinHandle<()>> {
        let ctx = AgentCtx {
            workspace_id: self.workspace.id.clone(),
            dispatch: self.dispatcher(),
            session_id: self.workspace.session.id,
            assembler: self.assembler.clone(),
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
                    session.record(&msg);
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
                            let blocker = msg.reply(
                                role,
                                Role::PM,
                                agent_core::TaskKind::Blocker,
                                serde_json::json!({"reason": reason}),
                            );
                            let _ = dispatcher.dispatch(blocker).await;
                        }
                        Err(e) => {
                            tracing::error!(?e, role = %role, "agent error");
                            let blocker = msg.reply(
                                role,
                                Role::PM,
                                agent_core::TaskKind::Blocker,
                                serde_json::json!({"reason": e.to_string()}),
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
