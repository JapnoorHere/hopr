//! Transport-agnostic session plumbing: the actor command channel, state watch,
//! output batching, and UI flow control (ADR 0006, ADR 0007).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{Notify, mpsc, watch};

use crate::{AppError, ErrorCode, SessionId, SessionState};

/// Capacity of an actor's command queue.
pub const COMMAND_CAPACITY: usize = 64;
/// Capacity (in chunks) of the transport → batcher queue. Bounded for backpressure.
pub const OUTPUT_CAPACITY: usize = 64;

/// Commands every session actor accepts.
#[derive(Debug)]
pub enum SessionCommand {
    /// Bytes typed or pasted by the user.
    Write(Vec<u8>),
    /// Terminal grid size changed.
    Resize {
        /// Columns.
        cols: u32,
        /// Rows.
        rows: u32,
    },
    /// Close the session gracefully.
    Close,
}

/// Cheap, cloneable handle to a running session actor.
#[derive(Clone)]
pub struct SessionHandle {
    /// Session id.
    pub id: SessionId,
    commands: mpsc::Sender<SessionCommand>,
    state: watch::Receiver<SessionState>,
    flow: Arc<FlowControl>,
}

impl SessionHandle {
    /// Assemble a handle from an actor's channels.
    pub fn new(
        id: SessionId,
        commands: mpsc::Sender<SessionCommand>,
        state: watch::Receiver<SessionState>,
        flow: Arc<FlowControl>,
    ) -> Self {
        Self {
            id,
            commands,
            state,
            flow,
        }
    }

    /// Send user input.
    pub async fn write(&self, data: Vec<u8>) -> Result<(), AppError> {
        self.send(SessionCommand::Write(data)).await
    }

    /// Propagate a terminal resize.
    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), AppError> {
        self.send(SessionCommand::Resize { cols, rows }).await
    }

    /// Ask the actor to close.
    pub async fn close(&self) -> Result<(), AppError> {
        self.send(SessionCommand::Close).await
    }

    /// Non-blocking write for synchronous callers. IPC commands use this so keystrokes keep
    /// their order (async commands may run concurrently and reorder).
    pub fn try_write(&self, data: Vec<u8>) -> Result<(), AppError> {
        self.try_send(SessionCommand::Write(data))
    }

    /// Non-blocking resize; see [`Self::try_write`].
    pub fn try_resize(&self, cols: u32, rows: u32) -> Result<(), AppError> {
        self.try_send(SessionCommand::Resize { cols, rows })
    }

    fn try_send(&self, cmd: SessionCommand) -> Result<(), AppError> {
        self.commands.try_send(cmd).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => AppError::new(
                ErrorCode::Internal,
                "Input is arriving faster than the session can send it",
            ),
            mpsc::error::TrySendError::Closed(_) => {
                AppError::new(ErrorCode::SessionNotFound, "This session has ended")
            }
        })
    }

    /// The UI has rendered `bytes` more output.
    pub fn ack(&self, bytes: u64) {
        self.flow.ack(bytes);
    }

    /// Subscribe to state changes.
    pub fn state(&self) -> watch::Receiver<SessionState> {
        self.state.clone()
    }

    async fn send(&self, cmd: SessionCommand) -> Result<(), AppError> {
        self.commands
            .send(cmd)
            .await
            .map_err(|_| AppError::new(ErrorCode::SessionNotFound, "This session has ended"))
    }
}

/// Byte-count flow control between the batcher and the UI renderer.
///
/// The batcher stops pulling output once more than `high` bytes are unacknowledged and
/// resumes when the UI has caught up to `low`. While paused, the bounded output queue
/// fills, the transport reader blocks, and the remote is throttled instead of RAM growing.
pub struct FlowControl {
    unacked: AtomicU64,
    notify: Notify,
    high: u64,
    low: u64,
}

impl Default for FlowControl {
    fn default() -> Self {
        Self::new(2 * 1024 * 1024, 512 * 1024)
    }
}

impl FlowControl {
    /// Pause above `high` unacked bytes, resume at or below `low`.
    pub fn new(high: u64, low: u64) -> Self {
        Self {
            unacked: AtomicU64::new(0),
            notify: Notify::new(),
            high,
            low: low.min(high),
        }
    }

    /// Record bytes handed to the UI.
    pub fn sent(&self, n: u64) {
        self.unacked.fetch_add(n, Ordering::AcqRel);
    }

    /// Record bytes the UI finished rendering.
    pub fn ack(&self, n: u64) {
        let _ = self
            .unacked
            .try_update(Ordering::AcqRel, Ordering::Acquire, |cur| {
                Some(cur.saturating_sub(n))
            });
        self.notify.notify_waiters();
    }

    /// Bytes sent but not yet acknowledged.
    pub fn unacked(&self) -> u64 {
        self.unacked.load(Ordering::Acquire)
    }

    /// Wait until the UI has room for more output.
    pub async fn wait_for_room(&self) {
        if self.unacked() <= self.high {
            return;
        }
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.unacked() <= self.low {
                return;
            }
            notified.await;
        }
    }
}

/// Output batching parameters.
#[derive(Debug, Clone, Copy)]
pub struct BatchConfig {
    /// Maximum time output waits before being flushed.
    pub interval: Duration,
    /// Flush immediately once this many bytes are buffered.
    pub max_bytes: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(12),
            max_bytes: 64 * 1024,
        }
    }
}

/// Coalesce transport output into batches and hand them to `flush`.
///
/// Flushes at most every `interval`, or immediately at `max_bytes`. `flush` returns
/// `false` when its sink is gone, which stops the batcher. Returns after a final flush
/// once `rx` closes.
pub async fn run_batcher<F>(
    mut rx: mpsc::Receiver<Vec<u8>>,
    flow: Arc<FlowControl>,
    cfg: BatchConfig,
    mut flush: F,
) where
    F: FnMut(Vec<u8>) -> bool,
{
    let mut buf: Vec<u8> = Vec::with_capacity(cfg.max_bytes);
    let mut ticker = tokio::time::interval(cfg.interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut emit = |buf: &mut Vec<u8>| -> bool {
        let out = std::mem::replace(buf, Vec::with_capacity(cfg.max_bytes));
        flow.sent(out.len() as u64);
        flush(out)
    };

    loop {
        if buf.is_empty() {
            flow.wait_for_room().await;
        }
        tokio::select! {
            biased;
            chunk = rx.recv(), if buf.len() < cfg.max_bytes => match chunk {
                Some(c) => {
                    buf.extend_from_slice(&c);
                    if buf.len() >= cfg.max_bytes && !emit(&mut buf) {
                        return;
                    }
                }
                None => {
                    if !buf.is_empty() {
                        emit(&mut buf);
                    }
                    return;
                }
            },
            _ = ticker.tick(), if !buf.is_empty() => {
                if !emit(&mut buf) {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    type Flushes = Arc<Mutex<Vec<Vec<u8>>>>;

    fn collector() -> (Flushes, impl FnMut(Vec<u8>) -> bool) {
        let out = Arc::new(Mutex::new(Vec::new()));
        let sink = out.clone();
        (out, move |b| {
            sink.lock().unwrap().push(b);
            true
        })
    }

    #[tokio::test(start_paused = true)]
    async fn coalesces_small_chunks_within_interval() {
        let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
        let (out, sink) = collector();
        let flow = Arc::new(FlowControl::default());
        let task = tokio::spawn(run_batcher(rx, flow, BatchConfig::default(), sink));
        for _ in 0..10 {
            tx.send(b"ab".to_vec()).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
        drop(tx);
        task.await.unwrap();
        let out = out.lock().unwrap();
        assert_eq!(out.concat(), b"ab".repeat(10));
        assert!(
            out.len() <= 2,
            "expected coalescing, got {} flushes",
            out.len()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn flushes_immediately_at_max_bytes() {
        let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
        let (out, sink) = collector();
        let cfg = BatchConfig {
            interval: Duration::from_secs(3600),
            max_bytes: 8,
        };
        let flow = Arc::new(FlowControl::default());
        let task = tokio::spawn(run_batcher(rx, flow, cfg, sink));
        tx.send(vec![1; 10]).await.unwrap();
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(out.lock().unwrap().len(), 1);
        drop(tx);
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn final_flush_when_input_closes() {
        let (tx, rx) = mpsc::channel(OUTPUT_CAPACITY);
        let (out, sink) = collector();
        let cfg = BatchConfig {
            interval: Duration::from_secs(3600),
            max_bytes: 1024,
        };
        let task = tokio::spawn(run_batcher(rx, Arc::new(FlowControl::default()), cfg, sink));
        tx.send(b"tail".to_vec()).await.unwrap();
        drop(tx);
        task.await.unwrap();
        assert_eq!(out.lock().unwrap().concat(), b"tail");
    }

    #[tokio::test(start_paused = true)]
    async fn pauses_when_ui_is_behind_and_resumes_on_ack() {
        let (tx, rx) = mpsc::channel(1);
        let (out, sink) = collector();
        let flow = Arc::new(FlowControl::new(4, 2));
        let cfg = BatchConfig {
            interval: Duration::from_millis(1),
            max_bytes: 4,
        };
        let task = tokio::spawn(run_batcher(rx, flow.clone(), cfg, sink));

        tx.send(vec![0; 5]).await.unwrap(); // flushed → 5 unacked > high
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(out.lock().unwrap().len(), 1);

        // Batcher is paused: queue (capacity 1) fills, the next send would block.
        tx.send(vec![1; 3]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(
            tx.try_send(vec![2]).is_err(),
            "queue should be full while paused"
        );
        assert_eq!(out.lock().unwrap().len(), 1);

        flow.ack(5);
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(out.lock().unwrap().len(), 2);
        drop(tx);
        task.await.unwrap();
    }

    #[test]
    fn ack_saturates_at_zero() {
        let f = FlowControl::default();
        f.sent(3);
        f.ack(10);
        assert_eq!(f.unacked(), 0);
    }
}
