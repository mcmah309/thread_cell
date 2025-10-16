use criterion::{Criterion, criterion_group, criterion_main};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, RwLock},
};
use thread_cell::ThreadCell;

#[derive(Default)]
struct NonSendSync {
    parent: Option<Rc<RefCell<NonSendSync>>>,
    children: Vec<Rc<RefCell<NonSendSync>>>,
    score: usize,
}

#[derive(Default)]
struct SendSync {
    parent: Option<Arc<RwLock<SendSync>>>,
    children: Vec<Arc<RwLock<SendSync>>>,
    score: usize,
}

impl NonSendSync {
    fn update(&mut self, depth: usize) -> usize {
        self.score = self.score.wrapping_add(depth);
        for child in self.children.iter().take(4) {
            let mut borrow = child.borrow_mut();
            borrow.score = borrow.score.wrapping_add(1);
        }
        self.score
    }

    fn read(&self) -> usize {
        self.children
            .iter()
            .map(|c| c.borrow().score)
            .sum::<usize>()
            + self.score
    }
}

impl SendSync {
    fn update(&mut self, depth: usize) -> usize {
        self.score = self.score.wrapping_add(depth);
        for child in self.children.iter().take(4) {
            if let Ok(mut c) = child.write() {
                c.score = c.score.wrapping_add(1);
            }
        }
        self.score
    }

    fn read(&self) -> usize {
        self.children
            .iter()
            .filter_map(|c| c.read().ok())
            .map(|c| c.score)
            .sum::<usize>()
            + self.score
    }
}

//************************************************************************//

fn benchmark_nonsend_vs_sendsync(c: &mut Criterion) {
    let mut group = c.benchmark_group("NonSendSync vs SendSync");

    let make_nonsend_graph = || {
        ThreadCell::new_with(move || {
            let root = Rc::new(RefCell::new(NonSendSync::default()));
            for _ in 0..100 {
                let child = Rc::new(RefCell::new(NonSendSync::default()));
                child.borrow_mut().parent = Some(root.clone());
                root.borrow_mut().children.push(child);
            }
            root
        })
    };

    let make_sendsync_graph = || {
        let root = Arc::new(RwLock::new(SendSync::default()));
        for _ in 0..100 {
            let child = Arc::new(RwLock::new(SendSync::default()));
            child.write().unwrap().parent = Some(root.clone());
            root.write().unwrap().children.push(child);
        }
        root
    };

    let thread_cell_graph = make_nonsend_graph();
    let sendsync_graph = make_sendsync_graph();

    const OPS: usize = 10_000;

    group.bench_function("ThreadCell<NonSendSync>", |b| {
        b.iter(|| {
            std::thread::scope(|s| {
                for _ in 0..12 {
                    let root_cell = thread_cell_graph.clone();
                    s.spawn(move || {
                        for i in 0..OPS {
                            root_cell.run_blocking(move |root| {
                                let mut r = root.borrow_mut();
                                if i % 10 == 0 {
                                    r.update(i);
                                } else {
                                    r.read();
                                }
                            });
                        }
                    });
                }
            });
        });
    });

    group.bench_function("Arc<RwLock<SendSync>>", |b| {
        b.iter(|| {
            std::thread::scope(|s| {
                for _ in 0..12 {
                    let root_mutex = sendsync_graph.clone();
                    s.spawn(move || {
                        for i in 0..OPS {
                            let mut root = root_mutex.write().unwrap();
                            if i % 10 == 0 {
                                root.update(i);
                            } else {
                                root.read();
                            }
                        }
                    });
                }
            });
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_nonsend_vs_sendsync);
criterion_main!(benches);
