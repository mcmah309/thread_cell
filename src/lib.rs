use std::thread;
use tracing::warn;

/// Messages sent to the manager thread
enum ThreadCellMessage<T> {
    Run(Box<dyn FnOnce(&mut T) + Send>),
    GetLockSync(crossbeam::channel::Sender<ThreadCellLock<T>>),
    GetLockAsync(tokio::sync::oneshot::Sender<ThreadCellLock<T>>),
}

/// A message type for session callbacks
type SessionMsg<T> = Box<dyn FnOnce(&mut T) + Send>;

static SESSION_ERROR_MESSAGE: &str = "Session thread has panicked or resource was dropped";

/// A "lock" on the resource until dropped.
/// While held, this is the only way to access the resource.
pub struct ThreadCellLock<T> {
    sender: crossbeam::channel::Sender<SessionMsg<T>>,
}

impl<T> ThreadCellLock<T> {
    pub fn run_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.sender
            .send(Box::new(move |resource| {
                let res = f(resource);
                let _ = tx.send(res);
            }))
            .expect(SESSION_ERROR_MESSAGE);
        rx.recv().expect(SESSION_ERROR_MESSAGE)
    }

    pub async fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Box::new(move |resource| {
                let res = f(resource);
                let _ = tx.send(res);
            }))
            .expect(SESSION_ERROR_MESSAGE);
        rx.await.expect(SESSION_ERROR_MESSAGE)
    }
}

static MANAGER_ERROR_MESSAGE: &str = "Manager thread has panicked";

/// Single-threaded manager for any `!Send` resource.
pub struct ThreadCell<T: 'static> {
    sender: crossbeam::channel::Sender<ThreadCellMessage<T>>,
}

impl<T: 'static> Clone for ThreadCell<T> {
    fn clone(&self) -> Self {
        Self { sender: self.sender.clone() }
    }
}

impl<T: Send + 'static> ThreadCell<T> {
    /// Creates new
    pub fn new(mut resource: T) -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<ThreadCellMessage<T>>();

        thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                match msg {
                    ThreadCellMessage::Run(f) => f(&mut resource),
                    ThreadCellMessage::GetLockSync(responder) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T>>();
                        if responder.send(ThreadCellLock { sender: stx }).is_err() {
                            warn!("Lock responder dropped before responding");
                        }
                        while let Ok(f) = srx.recv() {
                            f(&mut resource);
                        }
                    }
                    ThreadCellMessage::GetLockAsync(sender) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T>>();
                        if sender.send(ThreadCellLock { sender: stx }).is_err() {
                            warn!("Lock responder dropped before responding");
                        }
                        while let Ok(f) = srx.recv() {
                            f(&mut resource);
                        }
                    }
                }
            }
        });

        Self { sender: tx }
    }
}

impl<T: Send> ThreadCell<T> {
    /// Creates a new when `T` is not `Send` but a function to create `T` is
    pub fn new_with<F: FnOnce() -> T + Send + 'static>(resource_fn: F) -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<ThreadCellMessage<T>>();

        thread::spawn(move || {
            let mut resource = resource_fn();
            while let Ok(msg) = rx.recv() {
                match msg {
                    ThreadCellMessage::Run(f) => f(&mut resource),
                    ThreadCellMessage::GetLockSync(responder) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T>>();
                        if responder.send(ThreadCellLock { sender: stx }).is_err() {
                            warn!("Lock responder dropped before responding");
                        }
                        while let Ok(f) = srx.recv() {
                            f(&mut resource);
                        }
                    }
                    ThreadCellMessage::GetLockAsync(sender) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T>>();
                        if sender.send(ThreadCellLock { sender: stx }).is_err() {
                            warn!("Lock responder dropped before responding");
                        }
                        while let Ok(f) = srx.recv() {
                            f(&mut resource);
                        }
                    }
                }
            }
        });

        Self { sender: tx }
    }

    pub fn run_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.sender
            .send(ThreadCellMessage::Run(Box::new(move |resource| {
                let res = f(resource);
                let _ = tx.send(res);
            })))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.recv().expect(MANAGER_ERROR_MESSAGE)
    }

    pub async fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ThreadCellMessage::Run(Box::new(move |resource| {
                let res = f(resource);
                let _ = tx.send(res);
            })))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.await.expect(MANAGER_ERROR_MESSAGE)
    }

    pub fn lock_blocking(&self) -> ThreadCellLock<T> {
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.sender
            .send(ThreadCellMessage::GetLockSync(tx))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.recv().expect(MANAGER_ERROR_MESSAGE)
    }

    pub async fn lock(&self) -> ThreadCellLock<T> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ThreadCellMessage::GetLockAsync(tx))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.await.expect(MANAGER_ERROR_MESSAGE)
    }
}
