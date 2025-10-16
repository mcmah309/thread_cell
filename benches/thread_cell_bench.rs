use thread_cell::ThreadCell;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

#[derive(Default)]
struct TestResource {
    counter: usize,
    data: Vec<usize>,
}

impl TestResource {
    fn increment(&mut self) -> usize {
        for x in self.data.iter_mut().take(8) {
            *x = x.wrapping_add(1);
        }
        self.counter += 1;
        self.counter
    }

    fn read_counter(&self) -> usize {
        self.data.iter().take(4).fold(self.counter, |acc, &x| acc.wrapping_add(x))
    }

    fn add_value(&mut self, value: usize) {
        self.data.push(value);
    }

    fn reset(&mut self) {
        self.counter = 0;
        self.data.clear();
    }
}

//************************************************************************//

fn benchmark_batch_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Individual vs Batch Operations");

    let thread_cell = ThreadCell::<TestResource>::new(TestResource::default());

    group.bench_function("individual", |b| {
        b.iter(|| {
            for i in 0..100 {
                let result = thread_cell.run_blocking(move |resource| {
                    if i % 10 == 0 {
                        resource.add_value(i);
                    } else {
                        black_box(resource.read_counter());
                    }
                    resource.increment()
                });
                black_box(result);
            }
            thread_cell.run_blocking(|resource| resource.reset());
        })
    });

    group.bench_function("batched", |b| {
        b.iter(|| {
            let result = thread_cell.run_blocking(|resource| {
                let mut last_result = 0;
                for i in 0..100 {
                    if i % 10 == 0 {
                        resource.add_value(i);
                    } else {
                        black_box(resource.read_counter());
                    }
                    last_result = resource.increment();
                }
                last_result
            });
            black_box(result);
            thread_cell.run_blocking(|resource| resource.reset());
        })
    });

    group.finish();
}

//************************************************************************//

fn benchmark_lock_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Lock Operations");

    let thread_cell = ThreadCell::<TestResource>::new(TestResource::default());

    group.bench_function("with_lock_no_contention", |b| {
        b.iter(|| {
            let lock = thread_cell.session_blocking();
            for i in 0..100 {
                let result = lock.run_blocking(move |resource| {
                    if i % 10 == 0 {
                        resource.add_value(i);
                    } else {
                        black_box(resource.read_counter());
                    }
                    resource.increment()
                });
                black_box(result);
            }
            lock.run_blocking(|resource| resource.reset());
        })
    });

    group.bench_function("without_lock_no_contention", |b| {
        b.iter(|| {
            for i in 0..100 {
                let result = thread_cell.run_blocking(move |resource| {
                    if i % 10 == 0 {
                        resource.add_value(i);
                    } else {
                        black_box(resource.read_counter());
                    }
                    resource.increment()
                });
                black_box(result);
            }
            thread_cell.run_blocking(|resource| resource.reset());
        })
    });

    group.finish();
}

//************************************************************************//

fn benchmark_mutex_comparison(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("Contention Scaling");

    // Parameters
    const OPS_PER_THREAD: usize = 10_000;
    const THREAD_COUNTS: &[usize] = &[4, 64];

    for &threads in THREAD_COUNTS {
        let thread_cell = ThreadCell::<TestResource>::new(TestResource::default());
        let mutex_resource = Arc::new(Mutex::new(TestResource::default()));
        let tokio_mutex_resource = Arc::new(TokioMutex::new(TestResource::default()));

        // Blocking ThreadCell
        group.bench_function(format!("thread_cell_blocking_{}t", threads), |b| {
            b.iter(|| {
                std::thread::scope(|s| {
                    let mut handles = Vec::with_capacity(threads);
                    for _ in 0..threads {
                        let thread_cell_clone = thread_cell.clone();
                        handles.push(s.spawn(move || {
                            for i in 0..OPS_PER_THREAD {
                                thread_cell_clone.run_blocking(move |resource| {
                                    if i % 10 == 0 {
                                        resource.add_value(i);
                                    } else {
                                        black_box(resource.read_counter());
                                    }
                                    black_box(resource.increment());
                                });
                            }
                        }));
                    }
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            })
        });

        // Blocking Mutex
        group.bench_function(format!("mutex_blocking_{}t", threads), |b| {
            let mutex_resource = mutex_resource.clone();
            b.iter(|| {
                std::thread::scope(|s| {
                    let mut handles = Vec::with_capacity(threads);
                    for _ in 0..threads {
                        let m = mutex_resource.clone();
                        handles.push(s.spawn(move || {
                            for i in 0..OPS_PER_THREAD {
                                let mut guard = m.lock().unwrap();
                                if i % 10 == 0 {
                                    guard.add_value(i);
                                } else {
                                    black_box(guard.read_counter());
                                }
                                black_box(guard.increment());
                            }
                        }));
                    }
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            })
        });

        // Async ThreadCell
        group.bench_function(format!("thread_cell_async_{}t", threads), |b| {
            let thread_cell = thread_cell.clone();
            b.to_async(&rt).iter(|| {
                let thread_cell = thread_cell.clone();
                async move {
                    let mut join_handles = Vec::with_capacity(threads);
                    for _ in 0..threads {
                        let m = thread_cell.clone();
                        join_handles.push(tokio::spawn(async move {
                            for i in 0..OPS_PER_THREAD {
                                m.run(move |resource| {
                                    if i % 10 == 0 {
                                        resource.add_value(i);
                                    } else {
                                        black_box(resource.read_counter());
                                    }
                                    black_box(resource.increment());
                                })
                                .await;
                            }
                        }));
                    }
                    for j in join_handles {
                        j.await.unwrap();
                    }
                }
            });
        });

        // Async Tokio Mutex
        group.bench_function(format!("tokio_mutex_{}t", threads), |b| {
            let tokio_mutex_resource = tokio_mutex_resource.clone();
            b.to_async(&rt).iter(|| {
                let resource = tokio_mutex_resource.clone();
                async move {
                    let mut join_handles = Vec::with_capacity(threads);
                    for _ in 0..threads {
                        let r = resource.clone();
                        join_handles.push(tokio::spawn(async move {
                            for i in 0..OPS_PER_THREAD {
                                let mut guard = r.lock().await;
                                if i % 10 == 0 {
                                    guard.add_value(i);
                                } else {
                                    black_box(guard.read_counter());
                                }
                                black_box(guard.increment());
                            }
                        }));
                    }
                    for j in join_handles {
                        j.await.unwrap();
                    }
                }
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_batch_operations,
    benchmark_lock_operations,
    benchmark_mutex_comparison,
);
criterion_main!(benches);