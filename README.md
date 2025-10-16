# thread_cell

[<img alt="github" src="https://img.shields.io/badge/github-mcmah309/thread_cell-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/mcmah309/thread_cell)
[<img alt="crates.io" src="https://img.shields.io/crates/v/thread_cell.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/thread_cell)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-thread_cell-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/thread_cell)
[<img alt="test status" src="https://img.shields.io/github/actions/workflow/status/mcmah309/thread_cell/rust.yml?branch=master&style=for-the-badge" height="20">](https://github.com/mcmah309/thread_cell/actions/workflows/rust.yml)

**thread_cell** is a Rust crate that gives you **safe, `Send`/`Sync` access to non-`Send`/`Sync` data** by isolating it on a dedicated thread and interacting with it through message passing.

Unlike `Arc<Mutex<T>>`, `ThreadCell<T>` does not require `T: Send + Sync`, yet you can still share it across threads. This makes it perfect for types like `Rc`, `RefCell`, or FFI handles that are not `Send`.

---

## Why?

Sometimes you have data that **must stay on a single thread** (e.g. `Rc`, `RefCell`, or certain graphics/audio/DB handles).
But you still want to send it around safely — possibly to other threads — and run operations on it in a serialized manner.

### Example: making `Rc` safely shareable across threads

```rust
use std::rc::Rc;
use std::cell::RefCell;

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}

struct NonSendSync {
    parent: Rc<RefCell<NonSendSync>>,
    children: Vec<Rc<RefCell<NonSendSync>>>,
}

use thread_cell::ThreadCell;

// ✅ Compiles — ThreadCell makes `NonSendSync` shareable across threads
assert_send::<ThreadCell<NonSendSync>>();
assert_sync::<ThreadCell<NonSendSync>>();

// ❌ Does not compile — Arc<Mutex<T>> requires T: Send
// assert_send::<Arc<std::sync::Mutex<NonSendSync>>>();
// assert_sync::<Arc<std::sync::Mutex<NonSendSync>>>();
```

## Example

```rust
use std::rc::Rc;
use std::cell::RefCell;
use thread_cell::ThreadCell;

#[derive(Debug)]
struct GameState {
    score: usize,
}

fn main() {
    // Rc<RefCell<_>> is !Send and !Sync
    let shared = ThreadCell::new_with(|| Rc::new(RefCell::new(GameState { score: 0 })));

    // synchronous access
    shared.run_blocking(|state| {
        state.borrow_mut().score += 10;
        println!("Score after sync update: {}", state.borrow().score);
    });

    // async access - `tokio` feature flag
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on({
        let shared = shared.clone();
        async move {
            let a = shared.clone();
            let b = shared;

            let t1 = tokio::spawn(async move {
                a.run(|state| {
                    state.borrow_mut().score += 5;
                    state.borrow().score
                })
                .await
            });

            let t2 = tokio::spawn(async move {
                b.run(|state| {
                    state.borrow_mut().score += 20;
                    state.borrow().score
                })
                .await
            });

            let (s1, s2) = tokio::join!(t1, t2);
            println!("Task 1 score: {}", s1.unwrap());
            println!("Task 2 score: {}", s2.unwrap());
        }
    });

    let final_score = shared.run_blocking(|state| state.borrow().score);
    println!("Final score: {final_score}");
}
```
```console
Score after sync update: 10
Task 1 score: 15
Task 2 score: 35
Final score: 35
```

## When to Use ThreadCell

Each call with `ThreadCell` involves message passing to a background thread. This may be faster than a highly contended mutex for some workloads, but also may be slower for very fine-grained access patterns. If `T: Send + Sync` default to using a regular concurrent lock - `Arc<Mutex<T>>`.

When dealing with `!Send` types and still wanting to mutate it safely across threads, use `ThreadCell`.

Use case:
```rust
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::RwLock;

struct NonSendSync {
    parent: Rc<RefCell<NonSendSync>>,
    children: Vec<Rc<RefCell<NonSendSync>>>,
}

struct SendSync {
    parent: Arc<RwLock<SendSync>>,
    children: Vec<Arc<RwLock<SendSync>>>,
}
```

Operations on a `ThreadCell<NonSendSync>` is usually faster than operations on a `Arc<Mutex<SendSync>>`.
