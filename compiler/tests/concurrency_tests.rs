//! Phase 8 — language-level concurrency: await timeouts, receive timeouts,
//! task inspection, and cooperative cancellation via channel close.
//!
//! The VM-only `std.concurrency` module provides `sleep`; every builtin that
//! takes a task or channel value is a method on that value instead, because
//! the typed stdlib bridge only accepts scalars.

use nxl_compiler::source::SourceFile;

#[test]
fn await_timeout_fails_catchably_then_cooperative_cancel_cleans_up() {
    // The showcase program: a bounded await gives up with NXL11018, the
    // worker is then stopped cooperatively (the cancellation channel it
    // polls is closed), and the unbounded await returns the worker's real
    // result — so the end-of-run reaper has nothing left to wait for.
    let source = SourceFile::new(
        "timeout_cancel.nxl",
        "import std.concurrency;\n\
         fn slow(cancel) -> Int {\n\
             loop {\n\
                 if cancel.is_closed() { break; }\n\
                 std.concurrency.sleep(5);\n\
             }\n\
             return 0;\n\
         }\n\
         let cancel = channel();\n\
         let t = spawn slow(cancel);\n\
         let bounded = t.timeout(30);\n\
         try { print(await bounded); } catch e { print(e); }\n\
         cancel.close();\n\
         print(await t);\n\
         print(\"done\");\n",
    );
    let result = nxl_compiler::vm::run(&source).expect("timeout program should run");
    assert_eq!(result.output.len(), 3, "output: {:?}", result.output);
    assert!(
        result.output[0].starts_with("error[NXL11018]: await on task#")
            && result.output[0].ends_with(" timed out after 30ms"),
        "unexpected timeout line: {}",
        result.output[0]
    );
    assert_eq!(result.output[1], "0");
    assert_eq!(result.output[2], "done");
}

#[test]
fn await_timeout_returns_the_value_when_the_task_finishes_first() {
    let source = SourceFile::new(
        "timeout_fast.nxl",
        "fn fast() -> Int { return 7; }\n\
         let t = spawn fast();\n\
         let bounded = t.timeout(5000);\n\
         print(await bounded);\n",
    );
    let result = nxl_compiler::vm::run(&source).expect("fast task should run");
    assert_eq!(result.output, vec!["7"]);
}

#[test]
fn recv_timeout_times_out_on_an_empty_channel_and_passes_on_a_full_one() {
    let source = SourceFile::new(
        "recv_timeout.nxl",
        "let c = channel();\n\
         try { print(c.recv_timeout(20)); } catch e { print(e); }\n\
         let full = channel();\n\
         full.send(7);\n\
         print(full.recv_timeout(1000));\n\
         let closed = channel();\n\
         closed.close();\n\
         try { print(closed.recv_timeout(5000)); } catch e { print(e); }\n",
    );
    let result = nxl_compiler::vm::run(&source).expect("recv_timeout should run");
    assert_eq!(result.output.len(), 3, "output: {:?}", result.output);
    assert!(
        result.output[0].starts_with("error[NXL11018]: channel#")
            && result.output[0].ends_with(" recv timed out after 20ms"),
        "unexpected recv timeout line: {}",
        result.output[0]
    );
    assert_eq!(result.output[1], "7");
    // A closed channel fails immediately; the 5000ms budget is never spent.
    assert_eq!(result.output[2], "receive on a closed, empty channel");
}

#[test]
fn channel_is_closed_reflects_the_close_signal() {
    let source = SourceFile::new(
        "is_closed.nxl",
        "let c = channel();\n\
         print(c.is_closed());\n\
         c.close();\n\
         print(c.is_closed());\n",
    );
    let result = nxl_compiler::vm::run(&source).expect("is_closed should run");
    assert_eq!(result.output, vec!["false", "true"]);
}

#[test]
fn task_is_done_is_false_while_blocked_and_true_after_await() {
    // Fully deterministic: the worker cannot finish until the channel it
    // receives on is fed, so is_done must report false before the send.
    let source = SourceFile::new(
        "is_done.nxl",
        "fn waiter(c) -> Int { return c.recv(); }\n\
         let c = channel();\n\
         let t = spawn waiter(c);\n\
         print(t.is_done());\n\
         c.send(5);\n\
         print(await t);\n\
         print(t.is_done());\n",
    );
    let result = nxl_compiler::vm::run(&source).expect("is_done should run");
    assert_eq!(result.output, vec!["false", "5", "true"]);
}

#[test]
fn worker_pool_fan_out_stops_cooperatively_on_cancel() {
    let source = SourceFile::new(
        "worker_pool.nxl",
        "import std.concurrency;\n\
         fn worker(cancel, id) -> Int {\n\
             loop {\n\
                 if cancel.is_closed() { break; }\n\
                 std.concurrency.sleep(5);\n\
             }\n\
             return id;\n\
         }\n\
         let cancel = channel();\n\
         let a = spawn worker(cancel, 1);\n\
         let b = spawn worker(cancel, 2);\n\
         let c = spawn worker(cancel, 3);\n\
         std.concurrency.sleep(20);\n\
         cancel.close();\n\
         print(await a);\n\
         print(await b);\n\
         print(await c);\n\
         print(cancel.is_closed());\n",
    );
    let result = nxl_compiler::vm::run(&source).expect("worker pool should run");
    assert_eq!(result.output, vec!["1", "2", "3", "true"]);
}

#[test]
fn task_failure_is_catchable_and_the_run_continues() {
    let source = SourceFile::new(
        "task_failure.nxl",
        "fn boom() -> Int { throw \"boom\"; return 0; }\n\
         let t = spawn boom();\n\
         try { print(await t); } catch e { print(e); }\n\
         print(\"still running\");\n",
    );
    let result = nxl_compiler::vm::run(&source).expect("failed task should not abort the run");
    assert_eq!(result.output, vec!["task failed: boom", "still running"]);
}

#[test]
fn sleep_validates_its_argument_and_then_sleeps() {
    let source = SourceFile::new(
        "sleep_validate.nxl",
        "import std.concurrency;\n\
         try { std.concurrency.sleep(-1); } catch e { print(e); }\n\
         try { std.concurrency.sleep(999999999); } catch e { print(e); }\n\
         std.concurrency.sleep(1);\n\
         print(\"ok\");\n",
    );
    let result = nxl_compiler::vm::run(&source).expect("sleep should run");
    assert_eq!(
        result.output,
        vec![
            "std.concurrency::sleep: milliseconds must not be negative",
            "std.concurrency::sleep: 999999999ms exceeds the one-hour limit (3600000ms)",
            "ok",
        ]
    );
}

#[test]
fn js_backend_rejects_the_concurrency_module_loudly() {
    // A blocking sleep has no synchronous JavaScript equivalent, so the
    // module is VM-only like std.http and std.networking.
    let source = SourceFile::new(
        "conc_js.nxl",
        "import std.concurrency;\nstd.concurrency.sleep(1);",
    );
    let err =
        nxl_compiler::backend::compile_js(&source).expect_err("concurrency must fail loudly in JS");
    assert_eq!(err.code, "NXL2007");
}
