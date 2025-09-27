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

/// A "lock" on the resource held by the thread until dropped.
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

/// A cell that holds a value bound to a single thread. Thus T can be non-`Send` and/or non-`Sync`,
/// but `ThreadCell<T>` is always `Send`/`Sync`. Alternative to `Arc<Mutex<T>>`.
pub struct ThreadCell<T: 'static> {
    sender: crossbeam::channel::Sender<ThreadCellMessage<T>>,
}

impl<T: 'static> Clone for ThreadCell<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<T: Send> ThreadCell<T> {
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

impl<T> ThreadCell<T> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct TestResource {
        counter: usize,
    }

    impl TestResource {
        fn increment(&mut self) -> usize {
            self.counter += 1;
            self.counter
        }
    }

    #[test]
    fn basic_run_blocking_works() {
        let cell = ThreadCell::new(TestResource::default());
        let value = cell.run_blocking(|res| {
            res.increment();
            res.increment()
        });
        assert_eq!(value, 2);

        let value = cell.run_blocking(|res| res.increment());
        assert_eq!(value, 3);
    }

    #[test]
    fn can_be_sent_to_another_thread() {
        let cell = ThreadCell::new(TestResource::default());
        let handle = std::thread::spawn(move || cell.run_blocking(|res| res.increment()));
        let result = handle.join().unwrap();
        assert_eq!(result, 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_run_works() {
        let cell = ThreadCell::new(TestResource::default());
        let result = cell.run(|res| res.increment()).await;
        assert_eq!(result, 1);
    }

    #[test]
    fn lock_blocking_gives_mutable_access() {
        let cell = ThreadCell::new(TestResource::default());
        let lock = cell.lock_blocking();
        let value = lock.run_blocking(|res| {
            res.increment();
            res.increment()
        });
        assert_eq!(value, 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_lock_works() {
        let cell = ThreadCell::new(TestResource::default());
        let lock = cell.lock().await;
        let value = lock.run(|res| res.increment()).await;
        assert_eq!(value, 1);
    }

    #[test]
    fn can_hold_non_send_type() {
        #[derive(Default)]
        struct NotSend(Rc<()>); // Rc is !Send
        let cell = ThreadCell::new_with(|| NotSend(Rc::new(())));
        let count = cell.run_blocking(|res| Rc::strong_count(&res.0));
        assert_eq!(count, 1);
    }

    #[test]
    fn concurrent_run_blocking_requests_are_serialized() {
        let cell = ThreadCell::new(TestResource::default());
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..10 {
            let cell = cell.clone();
            let counter = counter.clone();
            handles.push(std::thread::spawn(move || {
                cell.run_blocking(move |res| {
                    let val = res.increment();
                    counter.fetch_add(val, Ordering::SeqCst);
                });
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // The sum of 1..=10 = 55
        assert_eq!(counter.load(Ordering::SeqCst), 55);
    }

    #[test]
    fn dropping_cell_does_not_panic() {
        let cell = ThreadCell::new(TestResource::default());
        drop(cell);
        // no panic = pass
    }
}
