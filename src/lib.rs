#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

use std::thread;

/// Messages sent to the manager thread
enum ThreadCellMessage<T> {
    Run(Box<dyn FnOnce(&mut T) + Send>),
    GetSessionSync(crossbeam::channel::Sender<ThreadCellSession<T>>),
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    GetSessionAsync(tokio::sync::oneshot::Sender<ThreadCellSession<T>>),
}

/// A message type for session callbacks
type SessionMsg<T> = Box<dyn FnOnce(&mut T) + Send>;

static SESSION_ERROR_MESSAGE: &str = "Session thread has panicked or resource was dropped";

/// A session with exclusive access to the resource held by the thread.
/// While held, this is the only way to access the resource. It is possible to create a "deadlock"
/// if a `ThreadCellSession` is requested while one is already held.
pub struct ThreadCellSession<T> {
    sender: crossbeam::channel::Sender<SessionMsg<T>>,
}

impl<T> ThreadCellSession<T> {
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

    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
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

/// A cell that holds a value bound to a single thread. Thus `T` can be non-`Send` and/or non-`Sync`,
/// but `ThreadCell<T>` is always `Send`/`Sync`. Access is provided through message passing, so no
/// internal locking is used. But a lock-like `ThreadCellSession` can be acquired to gain exclusive
/// access to the underlying resource while held.
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
                    ThreadCellMessage::GetSessionSync(responder) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T>>();
                        let _ = responder.send(ThreadCellSession { sender: stx });
                        while let Ok(f) = srx.recv() {
                            f(&mut resource);
                        }
                    }
                    #[cfg(feature = "tokio")]
                    ThreadCellMessage::GetSessionAsync(sender) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T>>();
                        let _ = sender.send(ThreadCellSession { sender: stx });
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
                    ThreadCellMessage::GetSessionSync(responder) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T>>();
                        let _ = responder.send(ThreadCellSession { sender: stx });
                        while let Ok(f) = srx.recv() {
                            f(&mut resource);
                        }
                    }
                    #[cfg(feature = "tokio")]
                    ThreadCellMessage::GetSessionAsync(sender) => {
                        let (stx, srx) = crossbeam::channel::unbounded::<SessionMsg<T>>();
                        let _ = sender.send(ThreadCellSession { sender: stx });
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

    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
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

    pub fn session_blocking(&self) -> ThreadCellSession<T> {
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.sender
            .send(ThreadCellMessage::GetSessionSync(tx))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.recv().expect(MANAGER_ERROR_MESSAGE)
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    pub async fn session(&self) -> ThreadCellSession<T> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ThreadCellMessage::GetSessionAsync(tx))
            .expect(MANAGER_ERROR_MESSAGE);
        rx.await.expect(MANAGER_ERROR_MESSAGE)
    }
}

impl<T: Send> ThreadCell<T> {
    /// Set the resource in a blocking manner
    pub fn set_blocking(&self, new_value: T) {
        self.run_blocking(|res| *res = new_value);
    }

    /// Set the resource in an async manner
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    pub async fn set(&self, new_value: T) {
        self.run(|res| *res = new_value).await;
    }

    /// Set the resource in a blocking manner, returning the old value
    pub fn replace_blocking(&self, new_value: T) -> T {
        self.run_blocking(|res| std::mem::replace(res, new_value))
    }

    /// Set the resource in an async manner, returning the old value
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    pub async fn replace(&self, new_value: T) -> T {
        self.run(|res| std::mem::replace(res, new_value)).await
    }
}

impl<T: Send + Default> ThreadCell<T> {
    pub fn take_blocking(&self) -> T {
        self.run_blocking(|res| std::mem::take(res))
    }

    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    pub async fn take(&self) -> T {
        self.run(|res| std::mem::take(res)).await
    }
}

impl<T: Send + Clone> ThreadCell<T> {
    /// Get a clone of the resource in a blocking manner
    pub fn get_blocking(&self) -> T {
        self.run_blocking(|res| res.clone())
    }

    /// Get a clone of the resource in an async manner
    #[cfg(feature = "tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
    pub async fn get(&self) -> T {
        self.run(|res| res.clone()).await
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

    #[cfg(feature = "tokio")]
    #[tokio::test(flavor = "current_thread")]
    async fn async_run_works() {
        let cell = ThreadCell::new(TestResource::default());
        let result = cell.run(|res| res.increment()).await;
        assert_eq!(result, 1);
    }

    #[test]
    fn session_blocking_gives_mutable_access() {
        let cell = ThreadCell::new(TestResource::default());
        let lock = cell.session_blocking();
        let value = lock.run_blocking(|res| {
            res.increment();
            res.increment()
        });
        assert_eq!(value, 2);
    }

    #[cfg(feature = "tokio")]
    #[tokio::test(flavor = "current_thread")]
    async fn async_session_works() {
        let cell = ThreadCell::new(TestResource::default());
        let lock = cell.session().await;
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
