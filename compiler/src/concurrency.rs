/// P10 — NXL Async / Concurrency Runtime
///
/// Provides the concurrency primitives used by NXL programs and the VM.
///
/// # Implementation status
///
/// | Subsystem              | Status                              |
/// |------------------------|-------------------------------------|
/// | Task / spawn           | IMPLEMENTED (OS threads)            |
/// | await_task             | IMPLEMENTED (blocking join)         |
/// | Channel (unbounded)    | IMPLEMENTED                         |
/// | Channel (bounded)      | IMPLEMENTED                         |
/// | Channel close          | IMPLEMENTED                         |
/// | Backpressure           | IMPLEMENTED (bounded channel)       |
/// | CancellationToken      | IMPLEMENTED                         |
/// | TaskGroup              | IMPLEMENTED (structured concurrency)|
/// | Task failure / panic   | IMPLEMENTED                         |
/// | Deadlock detection     | NOT IMPLEMENTED (future work)       |
/// | True async scheduler   | NOT IMPLEMENTED (P14+)              |
/// | Actor model            | IMPLEMENTED (mailbox)               |
///
/// # Design
///
/// The current implementation uses OS threads as the task primitive.
/// This is correct and testable now. A cooperative async scheduler
/// (green threads / async-await) is deferred to P14 once the standard
/// library and runtime are more complete.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::diagnostic::Diagnostic;
use crate::source::Span;

// ── Error type ────────────────────────────────────────────────────────────────

/// A concurrency error, convertible to a `Diagnostic`.
#[derive(Clone, Debug, PartialEq)]
pub struct ConcurrencyError {
    pub code: &'static str,
    pub message: String,
}

impl ConcurrencyError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn into_diagnostic(self) -> Diagnostic {
        Diagnostic::error(self.code, self.message, Span::default())
    }
}

impl From<ConcurrencyError> for Diagnostic {
    fn from(e: ConcurrencyError) -> Self {
        e.into_diagnostic()
    }
}

// ── CancellationToken ─────────────────────────────────────────────────────────

/// A shared cancellation signal.
///
/// Any number of tasks can hold a clone and poll `is_cancelled()`.
/// The owner calls `cancel()` to signal all holders.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal cancellation to all holders of this token.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns `true` if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

// ── Task ──────────────────────────────────────────────────────────────────────

/// A handle to a spawned task.
///
/// The task runs on an OS thread. Call `join()` to wait for completion
/// and retrieve the result.
pub struct Task<T: Send + 'static> {
    handle: Option<thread::JoinHandle<T>>,
    name: String,
}

impl<T: Send + 'static> Task<T> {
    /// The name given to this task at spawn time.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Block until the task completes and return its result.
    ///
    /// Returns `Err` if the task panicked.
    pub fn join(mut self) -> Result<T, ConcurrencyError> {
        self.handle
            .take()
            .expect("task already joined")
            .join()
            .map_err(|_| {
                ConcurrencyError::new("NXL10001", format!("task '{}' panicked", self.name))
            })
    }

    /// Returns `true` if the task has finished (non-blocking check).
    pub fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .map(|h| h.is_finished())
            .unwrap_or(true)
    }
}

/// Spawn a named task that runs `f` on a new OS thread.
pub fn spawn<F, T>(name: impl Into<String>, f: F) -> Task<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let name = name.into();
    let handle = thread::Builder::new()
        .name(name.clone())
        .spawn(f)
        .expect("failed to spawn thread");
    Task {
        handle: Some(handle),
        name,
    }
}

/// Block until `task` completes and return its result.
pub fn await_task<T: Send + 'static>(task: Task<T>) -> Result<T, ConcurrencyError> {
    task.join()
}

// ── Channel ───────────────────────────────────────────────────────────────────

/// The sending half of a `Channel`.
pub struct Sender<T> {
    inner: Arc<ChannelInner<T>>,
}

/// The receiving half of a `Channel`.
pub struct Receiver<T> {
    inner: Arc<ChannelInner<T>>,
}

struct ChannelInner<T> {
    queue: Mutex<std::collections::VecDeque<T>>,
    closed: AtomicBool,
    capacity: Option<usize>,
    /// Condvar for receivers waiting on new items.
    not_empty: std::sync::Condvar,
    /// Condvar for senders waiting for space (bounded channels).
    not_full: std::sync::Condvar,
}

impl<T> ChannelInner<T> {
    fn new(capacity: Option<usize>) -> Self {
        Self {
            queue: Mutex::new(std::collections::VecDeque::new()),
            closed: AtomicBool::new(false),
            capacity,
            not_empty: std::sync::Condvar::new(),
            not_full: std::sync::Condvar::new(),
        }
    }
}

/// Create an unbounded channel.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(ChannelInner::new(None));
    (
        Sender {
            inner: inner.clone(),
        },
        Receiver { inner },
    )
}

/// Create a bounded channel with the given capacity.
///
/// Senders block when the channel is full (backpressure).
pub fn bounded_channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    assert!(capacity > 0, "channel capacity must be > 0");
    let inner = Arc::new(ChannelInner::new(Some(capacity)));
    (
        Sender {
            inner: inner.clone(),
        },
        Receiver { inner },
    )
}

impl<T> Sender<T> {
    /// Send a value into the channel.
    ///
    /// Blocks if the channel is bounded and full (backpressure).
    /// Returns `Err` if the channel has been closed.
    pub fn send(&self, value: T) -> Result<(), ConcurrencyError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(ConcurrencyError::new("NXL10002", "send on closed channel"));
        }
        let mut queue = self.inner.queue.lock().unwrap();
        // Bounded backpressure: wait until there is space.
        if let Some(cap) = self.inner.capacity {
            while queue.len() >= cap {
                if self.inner.closed.load(Ordering::Acquire) {
                    return Err(ConcurrencyError::new("NXL10002", "send on closed channel"));
                }
                queue = self.inner.not_full.wait(queue).unwrap();
            }
        }
        queue.push_back(value);
        self.inner.not_empty.notify_one();
        Ok(())
    }

    /// Non-blocking send. Returns `Err` immediately if the channel is full or closed.
    pub fn try_send(&self, value: T) -> Result<(), ConcurrencyError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(ConcurrencyError::new("NXL10002", "send on closed channel"));
        }
        let mut queue = self.inner.queue.lock().unwrap();
        if let Some(cap) = self.inner.capacity {
            if queue.len() >= cap {
                return Err(ConcurrencyError::new("NXL10003", "channel is full"));
            }
        }
        queue.push_back(value);
        self.inner.not_empty.notify_one();
        Ok(())
    }

    /// Close the channel. No further sends are allowed.
    pub fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
        self.inner.not_empty.notify_all();
        self.inner.not_full.notify_all();
    }

    /// Returns `true` if the channel has been closed.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Receiver<T> {
    /// Receive a value, blocking until one is available.
    ///
    /// Returns `None` if the channel is closed and empty.
    pub fn recv(&self) -> Option<T> {
        let mut queue = self.inner.queue.lock().unwrap();
        loop {
            if let Some(value) = queue.pop_front() {
                self.inner.not_full.notify_one();
                return Some(value);
            }
            if self.inner.closed.load(Ordering::Acquire) {
                return None;
            }
            queue = self.inner.not_empty.wait(queue).unwrap();
        }
    }

    /// Non-blocking receive. Returns `None` if the channel is empty.
    pub fn try_recv(&self) -> Option<T> {
        let mut queue = self.inner.queue.lock().unwrap();
        let value = queue.pop_front();
        if value.is_some() {
            self.inner.not_full.notify_one();
        }
        value
    }

    /// Receive with a timeout. Returns `None` if no value arrives in time.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<T> {
        let mut queue = self.inner.queue.lock().unwrap();
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(value) = queue.pop_front() {
                self.inner.not_full.notify_one();
                return Some(value);
            }
            if self.inner.closed.load(Ordering::Acquire) {
                return None;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (q, _) = self.inner.not_empty.wait_timeout(queue, remaining).unwrap();
            queue = q;
        }
    }

    /// Returns `true` if the channel is closed and empty (iteration complete).
    pub fn is_done(&self) -> bool {
        let queue = self.inner.queue.lock().unwrap();
        queue.is_empty() && self.inner.closed.load(Ordering::Acquire)
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

// ── TaskGroup (structured concurrency) ───────────────────────────────────────

/// A structured concurrency group.
///
/// All tasks spawned into the group must complete before `join_all()`
/// returns. If any task panics, `join_all()` returns the first error
/// but still waits for all remaining tasks.
pub struct TaskGroup<T: Send + 'static> {
    tasks: Vec<Task<T>>,
}

impl<T: Send + 'static> TaskGroup<T> {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    /// Spawn a task into this group.
    pub fn spawn<F>(&mut self, name: impl Into<String>, f: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        self.tasks.push(spawn(name, f));
    }

    /// Wait for all tasks to complete.
    ///
    /// Returns all results in spawn order. If any task panicked,
    /// returns the first error encountered after collecting all results.
    pub fn join_all(self) -> Result<Vec<T>, ConcurrencyError> {
        let mut results = Vec::new();
        let mut first_error: Option<ConcurrencyError> = None;
        for task in self.tasks {
            match task.join() {
                Ok(v) => results.push(v),
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }
        match first_error {
            Some(e) => Err(e),
            None => Ok(results),
        }
    }
}

impl<T: Send + 'static> Default for TaskGroup<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ── Actor ─────────────────────────────────────────────────────────────────────

/// An actor handle: a named, thread-safe mailbox.
///
/// Messages are enqueued with [`Actor::dispatch`] and drained either in bulk
/// with [`Actor::drain`] or one at a time through a caller-supplied handler via
/// [`Actor::drain_with`]. The handle is cheaply cloneable and shareable across
/// tasks. Full supervision trees are deferred to a later phase, but the mailbox
/// itself is real (not a stub).
#[derive(Clone)]
pub struct Actor {
    name: String,
    mailbox: Arc<Mutex<Vec<String>>>,
}

impl Actor {
    /// Create a named actor with an empty mailbox.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mailbox: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The actor's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Enqueue a message for this actor.
    pub fn dispatch(&self, message: &str) {
        self.mailbox.lock().unwrap().push(message.to_owned());
    }

    /// Number of queued (undrained) messages.
    pub fn pending(&self) -> usize {
        self.mailbox.lock().unwrap().len()
    }

    /// Remove and return all queued messages in FIFO order.
    pub fn drain(&self) -> Vec<String> {
        std::mem::take(&mut *self.mailbox.lock().unwrap())
    }

    /// Drain the mailbox through `handler`, returning the number of messages
    /// delivered. Messages are processed in FIFO order.
    pub fn drain_with<F: FnMut(&str)>(&self, mut handler: F) -> usize {
        let messages = self.drain();
        for m in &messages {
            handler(m);
        }
        messages.len()
    }
}

// ── Synchronization helpers ───────────────────────────────────────────────────

/// A simple barrier that blocks until `n` threads have arrived.
pub struct Barrier {
    inner: Arc<std::sync::Barrier>,
}

impl Barrier {
    pub fn new(n: usize) -> Self {
        Self {
            inner: Arc::new(std::sync::Barrier::new(n)),
        }
    }

    /// Block until all `n` threads have called `wait()`.
    pub fn wait(&self) {
        self.inner.wait();
    }
}

impl Clone for Barrier {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
