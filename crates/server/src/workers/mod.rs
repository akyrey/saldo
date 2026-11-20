//! Background tasks started by `saldo serve`: each is a named loop that runs
//! at start, then every interval or as soon as new events were written, and
//! stops on shutdown (ARCHITECTURE.md 8.5).
//!
//! `tokio::spawn` starts a task that runs alongside the server, like
//! `go func()`; its `JoinHandle` is the `sync.WaitGroup` entry that lets
//! shutdown wait for it. A `CancellationToken` is Go's `context.Context`
//! cancellation: one call stops every task that holds a clone.
//!
//! Every task keeps its progress in the database, so killing the process at
//! any point and starting again is safe.

mod alerts;
mod attachments;
mod budgets;
pub mod notifier;
pub mod recurrences;
mod reminders;
mod signal;
mod webhooks;

use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub use signal::EventSignal;

use crate::state::AppState;

/// How long shutdown waits for a task to finish its current run.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// The running tasks and the token that stops them.
pub struct Workers {
    cancel: CancellationToken,
    handles: Vec<(&'static str, JoinHandle<()>)>,
}

impl Workers {
    pub fn new(cancel: CancellationToken) -> Self {
        Self {
            cancel,
            handles: Vec::new(),
        }
    }

    /// Starts a task that calls `run` now, then whenever `interval` passes or
    /// `signal` reports new events, until shutdown. A failed run is logged
    /// and the loop goes on.
    ///
    /// `F: FnMut() -> Fut` accepts any closure that can be called repeatedly
    /// and returns a future: like passing a Go `func() error`.
    pub fn spawn<F, Fut>(
        &mut self,
        name: &'static str,
        interval: Duration,
        signal: Option<&EventSignal>,
        run: F,
    ) where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let cancel = self.cancel.clone();
        let wake = signal.map(EventSignal::subscribe);
        let handle = tokio::spawn(run_loop(name, interval, wake, cancel, run));
        self.handles.push((name, handle));
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.handles.iter().map(|(name, _)| *name).collect()
    }

    /// Stops every task and waits (briefly) for the current runs to end.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        for (name, handle) in self.handles {
            if tokio::time::timeout(SHUTDOWN_GRACE, handle).await.is_err() {
                error!(task = name, "background task did not stop in time");
            }
        }
    }
}

/// The loop behind [`Workers::spawn`].
async fn run_loop<F, Fut>(
    name: &'static str,
    interval: Duration,
    mut wake: Option<tokio::sync::watch::Receiver<u64>>,
    cancel: CancellationToken,
    mut run: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    info!(task = name, "background task started");
    loop {
        if let Err(message) = run().await {
            error!(task = name, error = %message, "background task run failed");
        }
        // `select!` waits for whichever happens first, like Go's `select`.
        tokio::select! {
            () = cancel.cancelled() => break,
            () = tokio::time::sleep(interval) => {}
            () = changed(&mut wake) => {}
        }
    }
    info!(task = name, "background task stopped");
}

/// Resolves when new events were signalled; never, for tasks without a signal.
async fn changed(wake: &mut Option<tokio::sync::watch::Receiver<u64>>) {
    match wake {
        Some(receiver) => {
            if receiver.changed().await.is_err() {
                // The sender is gone (server stopping): wait for cancellation.
                std::future::pending::<()>().await;
            }
        }
        None => std::future::pending::<()>().await,
    }
}

/// Every background task of the server, registered in one place: later
/// phases add their task here.
pub fn start(state: &AppState, cancel: CancellationToken) -> Workers {
    let mut workers = Workers::new(cancel);
    webhooks::register(&mut workers, state);
    let app = state.app.clone();
    workers.spawn(budgets::NAME, budgets::INTERVAL, None, move || {
        budgets::run(app.clone())
    });
    reminders::spawn_bill_scheduler(&mut workers, state);
    notifier::spawn_notifier(&mut workers, state);
    alerts::spawn(&mut workers, state);
    recurrences::spawn(&mut workers, state);
    let app = state.app.clone();
    workers.spawn(attachments::NAME, attachments::INTERVAL, None, move || {
        attachments::run(app.clone())
    });
    workers
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn counter() -> (
        Arc<AtomicU32>,
        impl FnMut() -> std::future::Ready<Result<(), String>> + Send + 'static,
    ) {
        let runs = Arc::new(AtomicU32::new(0));
        let seen = runs.clone();
        (runs, move || {
            let n = seen.fetch_add(1, Ordering::SeqCst) + 1;
            // Every second run fails, to prove failures do not stop the loop.
            std::future::ready(if n.is_multiple_of(2) {
                Err(format!("run {n} failed"))
            } else {
                Ok(())
            })
        })
    }

    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn runs_at_start_then_every_interval_despite_failures() {
        let mut workers = Workers::new(CancellationToken::new());
        let (runs, run) = counter();
        workers.spawn("counter", Duration::from_secs(60), None, run);
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        for expected in 2..=4 {
            tokio::time::advance(Duration::from_secs(60)).await;
            settle().await;
            assert_eq!(runs.load(Ordering::SeqCst), expected);
        }
        assert_eq!(workers.names(), ["counter"]);
        workers.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn new_events_wake_the_task_before_the_interval() {
        let signal = EventSignal::new();
        let mut workers = Workers::new(CancellationToken::new());
        let (runs, run) = counter();
        workers.spawn("woken", Duration::from_secs(3600), Some(&signal), run);
        settle().await;
        signal.notify();
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 2);
        // Several signals while busy collapse into one extra run.
        signal.notify();
        signal.notify();
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), 3);
        workers.shutdown().await;
    }

    #[tokio::test]
    async fn start_runs_every_background_task() {
        let pool = saldo_app::db::connect_in_memory().await.unwrap();
        saldo_app::db::migrate(&pool).await.unwrap();
        let app = saldo_app::App::new(pool, Arc::new(saldo_app::clock::SystemClock));
        let config = crate::config::Config::from_lookup(|_| None).unwrap();
        let state = AppState::new(app, config);
        let workers = start(&state, CancellationToken::new());
        assert_eq!(
            workers.names(),
            [
                "webhook-dispatcher",
                "webhook-delivery",
                "auto-budgets",
                "bill-scheduler",
                "notifier",
                "alert-scheduler",
                "recurrence-runner",
                "attachment-sweep"
            ]
        );
        // Both run once at start; a write signal wakes them again.
        settle().await;
        state.events.notify();
        settle().await;
        workers.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_stops_every_task() {
        let cancel = CancellationToken::new();
        let mut workers = Workers::new(cancel.clone());
        let (runs, run) = counter();
        workers.spawn("stopped", Duration::from_secs(1), None, run);
        settle().await;
        workers.shutdown().await;
        assert!(cancel.is_cancelled());
        let after = runs.load(Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(10)).await;
        settle().await;
        assert_eq!(runs.load(Ordering::SeqCst), after);
    }
}
