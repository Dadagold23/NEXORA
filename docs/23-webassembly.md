# NXL WebAssembly Backend

NXL can compile a scalar subset of the language to a **core WebAssembly
binary**. This document is the contract: what compiles, what is refused, what
the host must provide, and where the output deliberately differs from the VM.

## Selecting the target

The manifest picks the artifact `nxlc build` emits:

```toml
[package]
name = "myapp"
target = "wasm"    # "js" (default), "vm", or "wasm"
entry = "src/main.nxl"
```

`nxlc build` then writes `build/debug/<name>.wasm` (or `build/release/…`) —
a complete WASM module with the magic header, no auxiliary files. There is no
`--target` CLI flag; the manifest is the single source of truth.

## Execution contract

The module exports exactly one function:

| Export | Signature | Meaning |
| --- | --- | --- |
| `_start` | `() -> ()` | Runs the program (calls the entry point, drops its result if it has one). A module without an entry point still exports an empty `_start`, so hosts always have the same hook. |

Everything else is imported from the host module `env` (table below). The
module needs **no memory, table or global** — scalars live in WASM locals —
so a host provides only the imports and calls `_start`.

The reference host — used by the parity tests — is the embedded `.mjs` glue in
`compiler/tests/wasm_backend_tests.rs`. Any host that provides the same import
signatures and the same formatting rules gets byte-identical program output.

### Host imports

`print` lowers to one typed helper per argument plus a line terminator; the
host tracks per-line spacing so the joined line matches the VM's
`println!("{}", args.join(" "))` exactly.

| Import | WASM type | Behaviour |
| --- | --- | --- |
| `env.print_i64` | `(i64) -> ()` | Appends a decimal integer (`-` for negatives). |
| `env.print_f64` | `(f64) -> ()` | Appends a float formatted like Rust's `Display`: `NaN`, `inf`, `-inf`, `-0`, otherwise shortest round-trip. |
| `env.print_bool` | `(i32) -> ()` | Appends `true` or `false` (i32 is 0/1). |
| `env.nl` | `() -> ()` | Ends the current line; the next value starts unspaced. |
| `env.math_cbrt` … `env.math_atan` | `(f64) -> f64` | The transcendentals WASM has no native opcode for (`cbrt`, `exp`, `ln`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`). |
| `env.math_atan2`, `env.math_pow` | `(f64, f64) -> f64` | Binary transcendentals. |

## The supported subset

| NXL | WASM |
| --- | --- |
| `Int` | `i64` (signed) |
| `Float` | `f64` |
| `Bool` | `i32` (0/1) |
| `fn` parameters/returns of these types | function types |
| `fn` without `-> T` | no result (like the VM's void functions) |
| `let` / `let mut` / assignment | locals (each name owns one typed local, copied exactly like the VM's `StoreLocal`) |
| `if`/`else`, `while`, `for i in <Int upper bound>` | structured blocks, `block`/`loop` + `br`/`br_if` |
| `break`, `continue`, `return` | `br` to the loop's break/continue labels, `return` |
| `print(...)` | the `env` print imports |
| all 23 `std.math` builtins | native opcodes where WASM has them (`sqrt`, `floor`, `ceil`, `trunc`), exact constant sequences for `round` (`trunc(x + copysign(0.5, x))` — Rust's half-away-from-zero, not banker's rounding), `is_nan`/`is_infinite`/`is_finite`, `to_radians`/`to_degrees`; host imports for the transcendentals. `Int` arguments convert with `f64.convert_i64_s`. |

Everything else is **refused at compile time** with `NXL2007` — a loud
diagnostic rather than wrong bytes: strings (even `print("hi")`), arrays,
structs/enums, closures, channels, tasks (`spawn`/`await`), exceptions
(`try`/`catch`/`throw`), FFI (`extern` blocks), `std.http`/`std.crypto`/`std.concurrency`
and the other VM-only builtins, dynamic dispatch, and an entry point that
takes parameters. A `let` that rebinds a name to a different type is also
refused (the frontend rejects re-`let` with `NXL2005` first; the backend's own
`NXL2007` check is defense-in-depth behind it).

One typed local per name means the backend holds each name to the type of its
first binding within a function; the VM allows rebinding to another type at
runtime, the WASM local cannot.

## Documented divergences from the VM

The VM is authoritative (`nxl run`); these are the only known behavioural
differences, all consequences of native WASM semantics:

1. **Integer division/remainder by zero traps** (as does `i64::MIN / -1`)
   instead of raising a catchable VM error. A program that relies on catching
   `division by zero` must run on the VM.
2. **Integer overflow wraps** (native `i64` wrap-around) where the VM raises a
   catchable "integer overflow" error.
3. **`Remainder` on `Float` is refused at compile time** (NXL2007): the VM
   raises a runtime type error for it (NXL9001), so the backend refuses rather
   than computing a value `nxl run` would reject. (The JavaScript backend
   emits `a % b` for this shape and therefore runs where the VM fails — a
   known JS-backend divergence, not mirrored here.)
4. **Float-to-string formatting** follows the host's shortest round-trip
   (`String(x)` in JS, `Display` in Rust — identical for all values outside
   the extreme exponent ranges ≥ 1e21 and ≤ 1e-7, where JS switches to
   exponent notation and Rust does not). The parity tests stay inside the
   identical range.
5. **Transcendentals** (`sin`, `exp`, `ln`, …) run on the host's math library;
   results can differ from Rust's `f64` methods in the last ULP. Exact-value
   inputs (`exp(0.0)`, `sin(0.0)`, …) are bit-identical everywhere and are
   what the parity tests use.

## Implementation note (dependency deviation)

The binary encoder is **hand-rolled** (~350 lines of LEB128 + section
encoding in `compiler/src/wasm_backend.rs`) rather than pulled from a crate
such as `wasmparser`/`wat`. This is a deliberate deviation from the
"use-vetted-implementations" rule: the emitted subset is small, fully
specified here, byte-for-byte deterministic (recompiling the same program
produces an identical module — pinned by test), and adds zero dependency
surface to the compiler. If the subset grows (memory, tables, globals,
multi-value), revisit and adopt a vetted encoder.

Code generation mirrors the JS backend's control-flow algorithm (loop
recovery → `block $break { loop $cont { … } }`, forward-target inlining,
suppressed join blocks) over the NIR, and the parity suite
(`compiler/tests/wasm_backend_tests.rs`) compares WASM output against the VM
line-for-line on every supported construct.
