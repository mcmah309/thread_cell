use actor_cell::ResourceManager;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;

// Test resource type
#[derive(Default)]
struct TestResource {
    counter: usize,
    data: Vec<usize>,
}

impl TestResource {
    fn increment(&mut self) -> usize {
        self.counter += 1;
        self.counter
    }
    
    fn add_value(&mut self, value: usize) -> usize {
        self.data.push(value);
        self.data.len()
    }
    
    fn compute_sum(&mut self) -> usize {
        self.data.iter().map(|&x| x as usize).sum::<usize>() + self.counter
    }
    
    fn reset(&mut self) {
        self.counter = 0;
        self.data.clear();
    }
}

// Function pointer versions for RunFn
fn increment_fn(resource: &mut TestResource) -> usize {
    resource.increment()
}

fn add_value_fn(resource: &mut TestResource) -> usize {
    resource.add_value(42)
}

fn compute_sum_fn(resource: &mut TestResource) -> usize {
    resource.compute_sum()
}

fn reset_fn(resource: &mut TestResource) -> usize {
    resource.reset();
    0
}

fn benchmark_run_vs_run_fn(c: &mut Criterion) {
    let mut group = c.benchmark_group("ResourceManager Run vs RunFn");
    
    // Setup managers for different test scenarios
    let manager_usize = ResourceManager::<TestResource, usize>::new(TestResource::default());
    let manager_unit = ResourceManager::<TestResource, ()>::new(TestResource::default());
    
    // Benchmark simple increment operation (returns usize)
    group.bench_function("run_blocking_increment", |b| {
        b.iter(|| {
            manager_usize.run_blocking(|resource| {
                black_box(resource.increment())
            })
        })
    });
    
    group.bench_function("run_blocking_fn_increment", |b| {
        b.iter(|| {
            manager_usize.run_blocking_fn(increment_fn)
        })
    });
    
    // Benchmark add operation (returns usize)
    group.bench_function("run_blocking_add", |b| {
        b.iter(|| {
            manager_usize.run_blocking(|resource| {
                black_box(resource.add_value(42))
            })
        })
    });
    
    group.bench_function("run_blocking_fn_add", |b| {
        b.iter(|| {
            manager_usize.run_blocking_fn(add_value_fn)
        })
    });
    
    // Benchmark compute operation (more expensive, returns usize)
    group.bench_function("run_blocking_compute", |b| {
        b.iter(|| {
            manager_usize.run_blocking(|resource| {
                black_box(resource.compute_sum())
            })
        })
    });
    
    group.bench_function("run_blocking_fn_compute", |b| {
        b.iter(|| {
            manager_usize.run_blocking_fn(compute_sum_fn)
        })
    });
    
    // Benchmark void operation (returns ())
    group.bench_function("run_blocking_reset", |b| {
        b.iter(|| {
            manager_unit.run_blocking(|resource| {
                black_box(resource.reset())
            })
        })
    });
    
    group.finish();
}

fn benchmark_batch_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Batch Operations");
    
    let manager = ResourceManager::<TestResource, usize>::new(TestResource::default());
    
    // Benchmark batch of operations using run_blocking
    group.bench_function("batch_run_blocking", |b| {
        b.iter(|| {
            for i in 0..100 {
                let value = manager.run_blocking(move |resource| {
                    resource.add_value(i);
                    resource.increment()
                });
                black_box(value);
            }
            // Reset for next iteration
            manager.run_blocking(|resource| resource.reset());
        })
    });
    
    // Benchmark batch of operations using run_blocking_fn
    group.bench_function("batch_run_blocking_fn", |b| {
        b.iter(|| {
            for _i in 0..100 {
                let value1 = manager.run_blocking_fn(add_value_fn);
                let value2 = manager.run_blocking_fn(increment_fn);
                black_box((value1, value2));
            }
            // Reset for next iteration
            manager.run_blocking_fn(reset_fn);
        })
    });
    
    group.finish();
}

fn benchmark_lock_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Lock Operations");
    
    let manager = ResourceManager::<TestResource, usize>::new(TestResource::default());
    
    // Benchmark using lock for batch operations
    group.bench_function("lock_batch_operations", |b| {
        b.iter(|| {
            let lock = manager.lock_blocking();
            for i in 0..100 {
                let value1 = lock.run_blocking(move |resource| {
                    resource.add_value(i);
                    resource.increment()
                });
                black_box(value1);
            }
            lock.run_blocking(|resource| resource.reset());
        })
    });
    
    // Compare with individual calls (no lock held)
    group.bench_function("no_lock_batch_operations", |b| {
        b.iter(|| {
            for i in 0..100 {
                let value = manager.run_blocking(move |resource| {
                    resource.add_value(i);
                    resource.increment()
                });
                black_box(value);
            }
            manager.run_blocking(|resource| resource.reset());
        })
    });
    
    group.finish();
}

fn benchmark_async_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Async Operations");
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let manager = Arc::new(ResourceManager::<TestResource, usize>::new(TestResource::default()));
    
    // Benchmark async run
    group.bench_function("async_run", |b| {
        b.to_async(&rt).iter(|| async {
            let manager = manager.clone();
            let value = manager.run(|resource| {
                black_box(resource.increment())
            }).await;
            black_box(value);
        })
    });
    
    // Benchmark async lock
    group.bench_function("async_lock", |b| {
        b.to_async(&rt).iter(|| async {
            let manager = manager.clone();
            let lock = manager.lock().await;
            let value = lock.run(|resource| {
                black_box(resource.increment())
            }).await;
            black_box(value);
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_run_vs_run_fn,
    benchmark_batch_operations,
    benchmark_lock_operations,
    benchmark_async_operations
);
criterion_main!(benches);