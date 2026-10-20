# NXL Concurrency Model (Phase 8)

NXL runs concurrent work on **real worker threads**, managed by the VM. This
document is the contract: what the primitives mean, what is guaranteed, what
is refused, and how to write programs that terminate.

## The primitives

| Form | Kind | Meaning |
| --- | --- | --- |
| `spawn f(args)` | expression → task | Runs `f` on a worker thread; returns a handle. `spawn obj.m(args)` clones the receiver in. Any other expression evaluates immediately and wraps the value in a completed task. |
| `await t` | expression | Blocks until the task completes; evaluates to its return value. Failures re-raise catchably (`task failed: ...`). Awaiting a plain value returns that value. |
| `t.is_done()` | task method | Non-blocking: has the task finished? |
| `t.timeout(ms)` | task method → task | A handle to the **same task** whose awaits give up after `ms` (budget starts at the `timeout` call). On expiry the await fails catchably: `error[NXL11018]: await on task#N timed out after Nms`. |
| `channel()` | expression → channel | A queue shared between threads. |
| `c.send(v)` | channel method | Appends; never blocks. Fails catchably when closed. |
| `c.recv()` | channel method | Blocks until a value arrives; fails catchably when closed **and** drained. |
| `c.recv_timeout(ms)` | channel method | `recv`, bounded. On expiry: `error[NXL11018]: channel#N recv timed out after Nms`; nothing is consumed and the channel stays open. |
| `c.is_closed()` | channel method | Whether the close signal has been sent (a polled cancellation check). |
| `c.close()` | channel method | Seals the channel: pending and future receives fail catchably once drained. |
| `for v in c` | statement | Streams received values until the channel closes. |
| `async { ... }` | expression → task | Snapshots visible bindings **by value**, runs the block as a worker function, returns its task. |
| `std.concurrency.sleep(ms)` | builtin (VM-only) | Parks the calling worker thread. Negative or > 1 hour is a catchable error. |

Every task/channel method and `std.concurrency.sleep` are **VM-only**
builtins: the JS and Rust backends refuse programs that use them with
NXL2007, like `std.http`.

## Asynchronous I/O vs CPU-parallel execution

NXL's `spawn` is **CPU parallelism**, not async I/O:

- Each spawned task is an OS thread. It runs to completion on its core;
  there is no executor, no future polling, and no work-stealing event loop.
- Blocking is fine and expected inside a task: `sleep`, channel `recv`,
  `recv_timeout`, and the blocking stdlib (`std.http`, `std.fs`,
  `std.networking`) park only that worker thread. Other tasks keep running
  on their own threads.
- What NXL does **not** have is cooperative scheduling of many cheap
  tasks on one thread. Ten thousand `spawn`s are ten thousand OS threads —
  use bounded worker pools (see the example below) instead of per-item
  tasks when the count is large.
- The main thread participates: `await` on the main thread parks the main
  thread only; workers continue independently.

## Blocking rules

1. Every `spawn`ed task **must terminate**. The VM's structured shutdown
   joins every spawned task before the process exits; a task that never
   finishes blocks exit forever. Bounded loops and cancellation polling are
   not style advice — they are required for termination.
2. `await` with no bound waits forever (by design). Use `t.timeout(ms)` at
   system boundaries where a stuck peer must not stall the program.
3. Channels are the only values shared between threads. Each thread's
   locals and globals are private (by design — NXL programs are
   data-race-free by construction).
4. `send` never blocks; `recv` blocks. Producers therefore cannot be
   throttled by slow consumers — bound queues yourself if a program can
   out-produce its consumer (count with a second channel, or have the
   consumer ack).

## Cancellation model

Cancellation in NXL is **cooperative** — a worker stops when its code
decides to stop:

- **Signal**: close a channel the worker polls. `c.is_closed()` is the
  non-blocking poll; a worker blocked in `c.recv()` unblocks with the
  catchable closed-channel error.
- **Deadlines**: `t.timeout(ms)` bounds how long a *joiner* waits, and
  `c.recv_timeout(ms)` bounds a single receive. Timing out does not kill
  the task — it stops the waiter from waiting. To actually stop the work,
  combine a deadline with the cancellation signal.
- **Errors**: a task that fails (uncaught `throw` or an internal panic —
  panics are caught and converted) reports `task failed: ...` through its
  handle's `await`, catchably, without taking down other tasks.
- **Shutdown**: the end-of-run reaper joins all tasks to real completion —
  a deadline attached by a caller never shortens the reaper's wait. A
  background task that failed at shutdown fails the run loudly.

There are deliberately **no** cancellation tokens as values, no preemption,
and no `select`. Tokens would need a first-class value type; `select` needs
an awaitable-array story. Both are documented non-goals for Phase 8.

## A complete, tested example

Worker pool with fan-out, deadlines, and cooperative shutdown — the shape
the test suite (`compiler/tests/concurrency_tests.rs`) runs:

```nxl
import std.concurrency;

fn worker(cancel, id) -> Int {
    loop {
        if cancel.is_closed() { break; }
        std.concurrency.sleep(5); // poll interval; work would go here
    }
    return id;
}

let cancel = channel();
let a = spawn worker(cancel, 1);
let b = spawn worker(cancel, 2);
let c = spawn worker(cancel, 3);

std.concurrency.sleep(20); // ... do main-thread work ...

cancel.close();             // cooperative cancellation signal
print(await a);             // 1
print(await b);             // 2
print(await c);             // 3
```

And a bounded await with a catchable timeout:

```nxl
let t = spawn worker(cancel, 9);
let bounded = t.timeout(50);
try {
    print(await bounded);
} catch e {
    print(e); // error[NXL11018]: await on task#N timed out after 50ms
}
cancel.close();  // let the worker finish so shutdown can join it
print(await t);  // 9
```

## Benchmarks

`nxl bench` includes two concurrency rows (median µs/iter, indicative smoke
baselines like the rest of the suite):

- `spawn_roundtrip` — 10 spawn/await pairs per run (thread create + join)
- `channel_msgs` — 50 send/receive pairs through one channel per run

## Status

IMPLEMENTED (VM): spawn/await, task methods (`is_done`, `timeout`), channels
(`send`/`recv`/`recv_timeout`/`close`/`is_closed`, `for..in`), `async {}`
blocks, actor mailboxes, `std.concurrency.sleep`, panic isolation in task
workers, structured end-of-run join, benchmarks.

NOT IMPLEMENTED (by decision): async/await *semantics* beyond the current
spawn/await model (no event loop, no executors), `select`, cancellation
tokens as values, preemption, shared mutable state, cross-backend
concurrency (JS/Rust refuse with NXL2007).
