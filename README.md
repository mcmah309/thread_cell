# thread_cell

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

// ❌ Does not compile — Arc<Mutex<T>> requires T: Send + Sync
// assert_send::<Arc<std::sync::Mutex<NonSendSync>>>();
// assert_sync::<Arc<std::sync::Mutex<NonSendSync>>>();
```

## Example

```rust
use thread_cell::ThreadCell;

#[derive(Default)]
struct Resource {
    counter: usize,
}

fn main() {
    let cell = ThreadCell::new(Resource::default());

    // Blocking access
    let result = cell.run_blocking(|res| {
        res.counter += 1;
        res.counter
    });
    println!("Counter: {result}");

    // Async access
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let result = cell
            .run(|res| {
                res.counter += 1;
                res.counter
            })
            .await;
        println!("Counter: {result}");
    });

    // Session access (exclusive, multiple operations)
    let session = cell.session_blocking();
    session.run_blocking(|res| {
        res.counter += 10;
    });
    // do some other work..
    let final_value = session.run_blocking(|res| {
        res.counter *= 2;
        res.counter
    });
    println!("Final counter: {final_value}");
}
```

## When to Use ThreadCell

✅ You have a **non-`Send` type** but still want to share and mutate it safely across threads.
✅ You want **serialized access** without locks (no risk of deadlocks).
✅ You need **async-friendly** access to such data.

❌ Use `Arc<Mutex<T>>` if `T: Send + Sync` and you just want a regular concurrent lock.

---

## Performance Notes

* Each call involves **message passing** to a background thread. This may be faster than a contended mutex for many workloads, but also may be slower for very fine-grained access patterns.

---

## Safety

`ThreadCell<T>` guarantees:

* All access to `T` happens on the same thread.
* No data races, even if `T` is `!Send` or `!Sync`.
