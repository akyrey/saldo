//! "New events were written": a signal that wakes background tasks early.

use tokio::sync::watch;

/// A counter that every write bumps. Each subscriber sees that it changed,
/// even if it was busy at the moment of the bump; several bumps while busy
/// count as one. Like a Go channel of capacity one per listener, carrying no
/// data.
#[derive(Clone)]
pub struct EventSignal {
    sender: std::sync::Arc<watch::Sender<u64>>,
}

impl Default for EventSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSignal {
    pub fn new() -> Self {
        let (sender, _) = watch::channel(0);
        Self {
            sender: std::sync::Arc::new(sender),
        }
    }

    /// Call after committing a change that appended events.
    pub fn notify(&self) {
        self.sender
            .send_modify(|count| *count = count.wrapping_add(1));
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.sender.subscribe()
    }
}
