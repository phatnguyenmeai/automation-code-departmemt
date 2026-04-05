//! Batched JSONL transcript writer.
//!
//! Each dispatched `TaskMessage` is recorded as one line in
//! `runs/<session>/transcript.jsonl`. Writes are batched: flushed when
//! the buffer reaches `BATCH_SIZE` entries, on a `FLUSH_INTERVAL` tick,
//! or on shutdown.

use agent_core::TaskMessage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const BATCH_SIZE: usize = 64;
const FLUSH_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub timestamp: DateTime<Utc>,
    pub msg: TaskMessage,
}

/// Handle for submitting entries to the background writer.
#[derive(Clone)]
pub struct TranscriptHandle {
    tx: mpsc::UnboundedSender<TranscriptEntry>,
}

impl TranscriptHandle {
    pub fn record(&self, msg: TaskMessage) {
        let entry = TranscriptEntry {
            timestamp: Utc::now(),
            msg,
        };
        let _ = self.tx.send(entry);
    }
}

/// Spawn the background writer. Returns a handle and its join handle. The
/// writer exits when all `TranscriptHandle` clones are dropped (channel
/// closes) *and* the buffer is flushed.
pub fn spawn(transcript_path: impl Into<PathBuf>) -> (TranscriptHandle, JoinHandle<()>) {
    let path = transcript_path.into();
    let (tx, rx) = mpsc::unbounded_channel::<TranscriptEntry>();
    let handle = tokio::spawn(writer_loop(path, rx));
    (TranscriptHandle { tx }, handle)
}

async fn writer_loop(path: PathBuf, mut rx: mpsc::UnboundedReceiver<TranscriptEntry>) {
    if let Some(parent) = path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            tracing::error!(?e, "transcript mkdir failed");
            return;
        }
    }

    let file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(?e, path = %path.display(), "transcript open failed");
            return;
        }
    };
    let mut writer = tokio::io::BufWriter::new(file);
    let mut buf: Vec<TranscriptEntry> = Vec::with_capacity(BATCH_SIZE);
    let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;
            maybe_entry = rx.recv() => {
                match maybe_entry {
                    Some(entry) => {
                        buf.push(entry);
                        if buf.len() >= BATCH_SIZE {
                            flush(&mut writer, &mut buf).await;
                        }
                    }
                    None => {
                        // senders closed: final drain and exit.
                        flush(&mut writer, &mut buf).await;
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                if !buf.is_empty() {
                    flush(&mut writer, &mut buf).await;
                }
            }
        }
    }
    if let Err(e) = writer.flush().await {
        tracing::warn!(?e, "transcript final flush failed");
    }
}

async fn flush<W: AsyncWriteExt + Unpin>(writer: &mut W, buf: &mut Vec<TranscriptEntry>) {
    for entry in buf.drain(..) {
        match serde_json::to_vec(&entry) {
            Ok(mut bytes) => {
                bytes.push(b'\n');
                if let Err(e) = writer.write_all(&bytes).await {
                    tracing::warn!(?e, "transcript write failed");
                    return;
                }
            }
            Err(e) => tracing::warn!(?e, "transcript serialize failed"),
        }
    }
    if let Err(e) = writer.flush().await {
        tracing::warn!(?e, "transcript flush failed");
    }
}

/// Read all entries from an existing transcript file.
pub async fn load(transcript_path: &Path) -> std::io::Result<Vec<TranscriptEntry>> {
    let bytes = tokio::fs::read(transcript_path).await?;
    let mut out = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_slice::<TranscriptEntry>(line) {
            Ok(e) => out.push(e),
            Err(e) => tracing::warn!(?e, "transcript parse skip"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{ArtifactRef, Role, TaskKind, TaskMessage};
    use std::path::PathBuf;

    fn sample_msg() -> TaskMessage {
        let r = ArtifactRef {
            json_path: PathBuf::from("artifacts/pm/x.json"),
            md_path: PathBuf::from("artifacts/pm/x.md"),
            kind: "requirement".into(),
            role: Role::PM,
            task_id: agent_core::TaskId::new(),
        };
        TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            r,
            serde_json::json!({"text_len": 7}),
        )
    }

    #[tokio::test]
    async fn flushes_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("transcript.jsonl");
        let (handle, join) = spawn(&path);
        for _ in 0..3 {
            handle.record(sample_msg());
        }
        drop(handle);
        join.await.unwrap();

        let entries = load(&path).await.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn flushes_on_tick() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("transcript.jsonl");
        let (handle, _join) = spawn(&path);
        handle.record(sample_msg());
        tokio::time::sleep(Duration::from_millis(250)).await;
        let entries = load(&path).await.unwrap();
        assert_eq!(entries.len(), 1);
    }
}
