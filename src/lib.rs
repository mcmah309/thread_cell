use std::thread;
use tracing::warn;

/// Messages sent to the manager thread
enum ManagerMessage<T, CR> {
    Run(Box<dyn FnOnce(&mut T) + Send>),
    RunFn(fn(&mut T) -> CR, crossbeam::channel::Sender<CR>),
    GetLockSync(crossbeam::channel::Sender<ResourceLock<T, CR>>),
    GetLockAsync(tokio::sync::oneshot::Sender<ResourceLock<T, CR>>),
}

/// A message type for session callbacks
enum SessionMsg<T, CR> {
    Boxed(Box<dyn FnOnce(&mut T) + Send>),
    Fn(fn(&mut T) -> CR, crossbeam::channel::Sender<CR>),
}

static SESSION_ERROR_MESSAGE: &str = "Session thread has panicked or resource was dropped";

/// A "lock" on the resource until dropped.
/// While held, this is the only way to access the resource.
pub struct ResourceLock<T, CR> {
    sender: crossbeam::channel::Sender<SessionMsg<T, CR>>,
}

impl<T, CR> ResourceLock<T, CR> {
    pub fn run_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.sender
            .send(SessionMsg::Boxed(Box::new(move |resource| {
                let res = f(resource);
                let _ = tx.send(res);
            })))
            .expect(SESSION_ERROR_MESSAGE);
        rx.recv().expect(SESSION_ERROR_MESSAGE)
    }

    pub fn run_blocking_fn<F>(&self, f: fn(&mut T) -> CR) -> CR {
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.sender
            .send(SessionMsg::Fn(f, tx))
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
            .send(SessionMsg::Boxed(Box::new(move |resource| {
                let res = f(resource);
                let _ = tx.send(res);
            })))
            .expect(SESSION_ERROR_MESSAGE);
        rx.await.expect(SESSION_ERROR_MESSAGE)
    }
}

static MANAGER_ERROR_MESSAGE: &str = "Manager thread has panicked";

/// Single-threaded manager for any `!Send` resource.
#[derive(Clone)]
pub struct ResourceManager<T: 'static, CR = ()>
where
    CR: Send + 'static,
{
    sender: crossbeam::channel::Sender<ManagerMessage<T, CR>>,
}

impl<T: Send + 'static, CR: Send + 'static> ResourceManager<T, CR> {
    /// Creates new
    pub fn new(mut resource: T) -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<ManagerMessage<T, CR>>();

        thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                match msg {
                    ManagerMessage::Run(f) => f(&mut resource),
                    ManagerMessage::RunFn(f, responder) => {
                        let _ = responder.send(f(&mut resource));
                    }
                    ManagerMessage::GetLockSync(responder) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T, CR>>();
                        if responder.send(ResourceLock { sender: stx }).is_err() {
                            warn!("Lock responder dropped before responding");
                        }
                        while let Ok(f) = srx.recv() {
                            match f {
                                SessionMsg::Boxed(f) => f(&mut resource),
                                SessionMsg::Fn(f, responder) => {
                                    let result = f(&mut resource);
                                    let _ = responder.send(result);
                                }
                            };
                        }
                    }
                    ManagerMessage::GetLockAsync(sender) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T, CR>>();
                        if sender.send(ResourceLock { sender: stx }).is_err() {
                            warn!("Lock responder dropped before responding");
                        }
                        while let Ok(f) = srx.recv() {
                            match f {
                                SessionMsg::Boxed(f) => f(&mut resource),
                                SessionMsg::Fn(f, responder) => {
                                    let result = f(&mut resource);
                                    let _ = responder.send(result);
                                }
                            };
                        }
                    }
                }
            }
        });

        Self { sender: tx }
    }
}

impl<T: Send, CR: Send + 'static> ResourceManager<T, CR> {
    /// Creates a new when `T` is not `Send` but a function to create `T` is
    pub fn new_with<F: FnOnce() -> T + Send + 'static>(resource_fn: F) -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<ManagerMessage<T, CR>>();

        thread::spawn(move || {
            let mut resource = resource_fn();
            while let Ok(msg) = rx.recv() {
                match msg {
                    ManagerMessage::Run(f) => f(&mut resource),
                    ManagerMessage::RunFn(f, responder) => {
                        let _ = responder.send(f(&mut resource));
                    }
                    ManagerMessage::GetLockSync(responder) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T, CR>>();
                        if responder.send(ResourceLock { sender: stx }).is_err() {
                            warn!("Lock responder dropped before responding");
                        }
                        while let Ok(f) = srx.recv() {
                            match f {
                                SessionMsg::Boxed(f) => f(&mut resource),
                                SessionMsg::Fn(f, responder) => {
                                    let result = f(&mut resource);
                                    let _ = responder.send(result);
                                }
                            };
                        }
                    }
                    ManagerMessage::GetLockAsync(sender) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T, CR>>();
                        if sender.send(ResourceLock { sender: stx }).is_err() {
                            warn!("Lock responder dropped before responding");
                        }
                        while let Ok(f) = srx.recv() {
                            match f {
                                SessionMsg::Boxed(f) => f(&mut resource),
                                SessionMsg::Fn(f, responder) => {
                                    let result = f(&mut resource);
                                    let _ = responder.send(result);
                                }
                            };
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
            .send(ManagerMessage::Run(Box::new(move |resource| {
                let res = f(resource);
                let _ = tx.send(res);
            })))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.recv().expect(MANAGER_ERROR_MESSAGE)
    }

    pub fn run_blocking_fn(&self, f: fn(&mut T) -> CR) -> CR {
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.sender
            .send(ManagerMessage::RunFn(f, tx))
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
            .send(ManagerMessage::Run(Box::new(move |resource| {
                let res = f(resource);
                let _ = tx.send(res);
            })))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.await.expect(MANAGER_ERROR_MESSAGE)
    }

    pub fn lock_blocking(&self) -> ResourceLock<T, CR> {
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.sender
            .send(ManagerMessage::GetLockSync(tx))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.recv().expect(MANAGER_ERROR_MESSAGE)
    }

    pub async fn lock(&self) -> ResourceLock<T, CR> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ManagerMessage::GetLockAsync(tx))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.await.expect(MANAGER_ERROR_MESSAGE)
    }
}
