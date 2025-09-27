use actor_cell::ResourceManager;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

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

    fn reset(&mut self) {
        self.counter = 0;
        self.data.clear();
    }
}

//************************************************************************//

fn benchmark_batch_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Individual vs Batch Operations");

    let manager = ResourceManager::<TestResource>::new(TestResource::default());

    group.bench_function("individual", |b| {
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

    group.bench_function("batched", |b| {
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

    group.bench_function("with_lock_no_contention", |b| {
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

    group.bench_function("without_lock_no_contention", |b| {
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

    group.finish();
}

fn benchmark_mutex_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("Vs Mutex");

    // Setup both approaches
    let manager = ResourceManager::<TestResource>::new(TestResource::default());
    let mutex_resource = Arc::new(Mutex::new(TestResource::default()));
    let tokio_mutex_resource = Arc::new(TokioMutex::new(TestResource::default()));

    group.bench_function("manager_increment_no_contention", |b| {
        b.iter(|| {
            let result = manager.run_blocking(|resource| black_box(resource.increment()));
            black_box(result);
        })
    });

    group.bench_function("mutex_increment_no_contention", |b| {
        let mutex_resource = mutex_resource.clone();
        b.iter(|| {
            let result = {
                let mut guard = mutex_resource.lock().unwrap();
                black_box(guard.increment())
            };
            black_box(result);
        })
    });

    // Async comparison
    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("async_manager_increment_no_contention", |b| {
        b.to_async(&rt).iter(|| async {
            let result = manager.run(|resource| black_box(resource.increment())).await;
            black_box(result);
        })
    });

    group.bench_function("async_tokio_mutex_increment_no_contention", |b| {
        let tokio_mutex_resource = tokio_mutex_resource.clone();
        b.to_async(&rt).iter(|| async {
            let result = {
                let mut guard = tokio_mutex_resource.lock().await;
                black_box(guard.increment())
            };
            black_box(result);
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_batch_operations,
    benchmark_lock_operations,
    benchmark_mutex_comparison,
);
criterion_main!(benches);
