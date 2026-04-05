use agent_core::{Priority, Role, TaskMessage};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Per-role multi-priority lane.
pub struct Lane {
    pub high_tx: mpsc::UnboundedSender<TaskMessage>,
    pub normal_tx: mpsc::UnboundedSender<TaskMessage>,
    pub low_tx: mpsc::UnboundedSender<TaskMessage>,
    pub high_rx: Option<mpsc::UnboundedReceiver<TaskMessage>>,
    pub normal_rx: Option<mpsc::UnboundedReceiver<TaskMessage>>,
    pub low_rx: Option<mpsc::UnboundedReceiver<TaskMessage>>,
}

impl Lane {
    pub fn new() -> Self {
        let (high_tx, high_rx) = mpsc::unbounded_channel();
        let (normal_tx, normal_rx) = mpsc::unbounded_channel();
        let (low_tx, low_rx) = mpsc::unbounded_channel();
        Self {
            high_tx,
            normal_tx,
            low_tx,
            high_rx: Some(high_rx),
            normal_rx: Some(normal_rx),
            low_rx: Some(low_rx),
        }
    }

    /// Take ownership of the receivers (called once when worker spawns).
    pub fn take_receivers(
        &mut self,
    ) -> (
        mpsc::UnboundedReceiver<TaskMessage>,
        mpsc::UnboundedReceiver<TaskMessage>,
        mpsc::UnboundedReceiver<TaskMessage>,
    ) {
        (
            self.high_rx.take().expect("high rx"),
            self.normal_rx.take().expect("normal rx"),
            self.low_rx.take().expect("low rx"),
        )
    }
}

impl Default for Lane {
    fn default() -> Self {
        Self::new()
    }
}

/// Sender half for dispatch from outside (agents, CLI, PM).
#[derive(Clone)]
pub struct LaneSender {
    pub high: mpsc::UnboundedSender<TaskMessage>,
    pub normal: mpsc::UnboundedSender<TaskMessage>,
    pub low: mpsc::UnboundedSender<TaskMessage>,
}

impl LaneSender {
    pub fn send(&self, msg: TaskMessage) -> Result<(), mpsc::error::SendError<TaskMessage>> {
        match msg.priority {
            Priority::High => self.high.send(msg),
            Priority::Normal => self.normal.send(msg),
            Priority::Low => self.low.send(msg),
        }
    }
}

/// One lane per agent role. Workers poll high→normal→low.
pub struct LaneQueue {
    lanes: HashMap<Role, Lane>,
}

impl LaneQueue {
    pub fn new() -> Self {
        let mut lanes = HashMap::new();
        for r in Role::all() {
            lanes.insert(*r, Lane::new());
        }
        Self { lanes }
    }

    pub fn sender(&self, role: Role) -> LaneSender {
        let lane = self.lanes.get(&role).expect("role lane");
        LaneSender {
            high: lane.high_tx.clone(),
            normal: lane.normal_tx.clone(),
            low: lane.low_tx.clone(),
        }
    }

    pub fn take_lane(&mut self, role: Role) -> Lane {
        self.lanes.remove(&role).expect("role lane")
    }
}

impl Default for LaneQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Receive the highest-priority message available across the three
/// receivers. Blocks if all are empty (waits on any to become ready).
pub async fn recv_prioritized(
    high: &mut mpsc::UnboundedReceiver<TaskMessage>,
    normal: &mut mpsc::UnboundedReceiver<TaskMessage>,
    low: &mut mpsc::UnboundedReceiver<TaskMessage>,
) -> Option<TaskMessage> {
    // Fast path: drain priority first.
    if let Ok(m) = high.try_recv() {
        return Some(m);
    }
    if let Ok(m) = normal.try_recv() {
        return Some(m);
    }
    if let Ok(m) = low.try_recv() {
        return Some(m);
    }

    tokio::select! {
        biased;
        m = high.recv() => m,
        m = normal.recv() => m,
        m = low.recv() => m,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{TaskKind, TaskMessage};

    #[tokio::test]
    async fn high_preempts_normal() {
        let mut lane = Lane::new();
        let sender = LaneSender {
            high: lane.high_tx.clone(),
            normal: lane.normal_tx.clone(),
            low: lane.low_tx.clone(),
        };
        let mut n = TaskMessage::new(Role::PM, Role::BA, TaskKind::Requirement, serde_json::json!(1));
        n.priority = Priority::Normal;
        let mut h = TaskMessage::new(Role::PM, Role::BA, TaskKind::Requirement, serde_json::json!(2));
        h.priority = Priority::High;
        sender.send(n).unwrap();
        sender.send(h).unwrap();

        let (mut hr, mut nr, mut lr) = lane.take_receivers();
        let first = recv_prioritized(&mut hr, &mut nr, &mut lr).await.unwrap();
        assert_eq!(first.priority, Priority::High);
        let second = recv_prioritized(&mut hr, &mut nr, &mut lr).await.unwrap();
        assert_eq!(second.priority, Priority::Normal);
    }
}
