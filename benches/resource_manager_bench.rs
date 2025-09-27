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
        self.data.iter().sum::<usize>() + self.counter
    }
    
    fn reset(&mut self) {
        self.counter = 0;
        self.data.clear();
    }
    
    fn batch_operation(&mut self, value: usize) -> (usize, usize) {
        self.data.push(value);
        self.counter += 1;
        (self.data.len(), self.counter)
    }
}

fn benchmark_basic_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Basic Operations");
    
    let manager = ResourceManager::<TestResource>::new(TestResource::default());
    
    // Simple increment
    group.bench_function("increment", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                black_box(resource.increment())
            });
            black_box(result);
        })
    });
    
    // Add operation
    group.bench_function("add_value", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                black_box(resource.add_value(42))
            });
            black_box(result);
        })
    });
    
    // More expensive compute operation
    group.bench_function("compute_sum", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                black_box(resource.compute_sum())
            });
            black_box(result);
        })
    });
    
    // Void operation
    group.bench_function("reset", |b| {
        b.iter(|| {
            manager.run_blocking(|resource| {
                black_box(resource.reset())
            });
        })
    });
    
    // Combined operation (simulates real-world usage)
    group.bench_function("batch_operation", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                black_box(resource.batch_operation(42))
            });
            black_box(result);
        })
    });
    
    group.finish();
}

fn benchmark_closure_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("Closure Types");
    
    let manager = ResourceManager::<TestResource>::new(TestResource::default());
    
    // Function pointer (direct function call)
    fn increment_function(resource: &mut TestResource) -> usize {
        resource.increment()
    }
    
    group.bench_function("function_pointer", |b| {
        b.iter(|| {
            let result = manager.run_blocking(increment_function);
            black_box(result);
        })
    });
    
    // Simple closure
    group.bench_function("simple_closure", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| resource.increment());
            black_box(result);
        })
    });
    
    // Closure with capture
    let capture_value = 42;
    group.bench_function("capturing_closure", |b| {
        b.iter(|| {
            let result = manager.run_blocking(move |resource| {
                resource.add_value(capture_value);
                resource.increment()
            });
            black_box(result);
        })
    });
    
    // Complex closure
    group.bench_function("complex_closure", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                for i in 0..10 {
                    resource.add_value(i);
                }
                resource.compute_sum()
            });
            black_box(result);
        })
    });
    
    group.finish();
}

fn benchmark_batch_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Batch Operations");
    
    let manager = ResourceManager::<TestResource>::new(TestResource::default());
    
    // Many individual operations
    group.bench_function("individual_operations", |b| {
        b.iter(|| {
            for i in 0..100 {
                let result = manager.run_blocking(move |resource| {
                    resource.add_value(i);
                    resource.increment()
                });
                black_box(result);
            }
            // Reset for next iteration
            manager.run_blocking(|resource| resource.reset());
        })
    });
    
    // Batch operations in single call
    group.bench_function("batched_in_single_call", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                let mut last_result = 0;
                for i in 0..100 {
                    resource.add_value(i);
                    last_result = resource.increment();
                }
                last_result
            });
            black_box(result);
            
            // Reset for next iteration
            manager.run_blocking(|resource| resource.reset());
        })
    });
    
    group.finish();
}

fn benchmark_lock_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Lock Operations");
    
    let manager = ResourceManager::<TestResource>::new(TestResource::default());
    
    // Using lock for batch operations
    group.bench_function("with_lock", |b| {
        b.iter(|| {
            let lock = manager.lock_blocking();
            for i in 0..100 {
                let result = lock.run_blocking(move |resource| {
                    resource.add_value(i);
                    resource.increment()
                });
                black_box(result);
            }
            lock.run_blocking(|resource| resource.reset());
        })
    });
    
    // Without lock (individual manager calls)
    group.bench_function("without_lock", |b| {
        b.iter(|| {
            for i in 0..100 {
                let result = manager.run_blocking(move |resource| {
                    resource.add_value(i);
                    resource.increment()
                });
                black_box(result);
            }
            manager.run_blocking(|resource| resource.reset());
        })
    });
    
    // Lock with single batched operation
    group.bench_function("lock_with_batch", |b| {
        b.iter(|| {
            let lock = manager.lock_blocking();
            let result = lock.run_blocking(|resource| {
                let mut last_result = 0;
                for i in 0..100 {
                    resource.add_value(i);
                    last_result = resource.increment();
                }
                last_result
            });
            black_box(result);
            lock.run_blocking(|resource| resource.reset());
        })
    });
    
    group.finish();
}

fn benchmark_async_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Async Operations");
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let manager = Arc::new(ResourceManager::<TestResource>::new(TestResource::default()));
    
    // Async run using iter_custom
    group.bench_function("async_run", |b| {
        b.iter_custom(|iters| {
            let manager = manager.clone();
            rt.block_on(async move {
                let start = std::time::Instant::now();
                for _i in 0..iters {
                    let result = manager.run(|resource| {
                        black_box(resource.increment())
                    }).await;
                    black_box(result);
                }
                start.elapsed()
            })
        });
    });
    
    // Async lock using iter_custom
    group.bench_function("async_lock", |b| {
        b.iter_custom(|iters| {
            let manager = manager.clone();
            rt.block_on(async move {
                let start = std::time::Instant::now();
                for _i in 0..iters {
                    let lock = manager.lock().await;
                    let result = lock.run(|resource| {
                        black_box(resource.increment())
                    }).await;
                    black_box(result);
                }
                start.elapsed()
            })
        });
    });
    
    // Compare with blocking versions
    group.bench_function("blocking_equivalent", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                black_box(resource.increment())
            });
            black_box(result);
        })
    });
    
    group.finish();
}

fn benchmark_different_workloads(c: &mut Criterion) {
    let mut group = c.benchmark_group("Different Workloads");
    
    let manager = ResourceManager::<TestResource>::new(TestResource::default());
    
    // Light workload (just increment)
    group.bench_function("light_workload", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                black_box(resource.increment())
            });
            black_box(result);
        })
    });
    
    // Medium workload (some computation)
    group.bench_function("medium_workload", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                for i in 0..10 {
                    resource.add_value(i);
                }
                black_box(resource.compute_sum())
            });
            black_box(result);
        })
    });
    
    // Heavy workload (lots of computation)
    group.bench_function("heavy_workload", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| {
                for i in 0..1000 {
                    resource.add_value(i);
                }
                let sum = resource.compute_sum();
                resource.reset();
                black_box(sum)
            });
            black_box(result);
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_basic_operations,
    benchmark_closure_types,
    benchmark_batch_operations,
    benchmark_lock_operations,
    benchmark_async_operations,
    benchmark_different_workloads
);
criterion_main!(benches);