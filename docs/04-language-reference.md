# Language Reference (implemented subset)

The canonical description is [specifications/nxl-grammar.ebnf](../specifications/nxl-grammar.ebnf). The parser in `compiler/src/parser.rs` accepts the following forms (tested by `compiler/tests/language_frontend_tests.rs`):

- variables: `let x = 1;`, `let mut i = 0;`, `let name: Int = "x";`
- constants/mutability: `const`, `mut`
- functions: `fn add(a: Int, b: Int) -> Int { return a + b; }`, async variants
- control flow: `if cond { } else { }`, `while cond { }`, `loop { }` with `break`/`continue`
- `for i in iter { }`, `match v { ... }`, `try { } catch { } finally { }`, `throw`
- types: `struct`, `class`, `actor`, `interface`, `trait`, `enum`, `impl`, type aliases
- generics on declarations, typed channels `channel`, closures in expressions
- modules: `module path;`, `import` with alias/selected, `export`
- concurrency: `async fn`, `await`, `spawn`
- strings with interpolation, char literals, numeric bases, raw strings, comments

Semantics NOT yet enforced (P04/P05 status): ownership, borrowing, lifetimes, full generics. Parser accepts the syntax; the compiler reports unsupported lowering rather than pretending they run.

## Phase 1 — executable semantics (errors + strings)

Implemented end-to-end (lowered in `nir.rs`, executed by the VM and the
JavaScript backend, tested in `language_frontend_tests.rs`):

- string interpolation: `"hi ${name}, 1+2=${1 + 2}"` (any value converts via `ToString`; `Void` is rejected with NXL2006)
- string concatenation: `"a" + "b"` (source-level `+` on `String` operands)
- `throw <value>` — raises any value as an exception
- `try { } catch e { }` — bare catch binds the value
- `try { } catch e: Int { }` — annotated catches match the runtime type name (`Int`/`Float`/`Bool`/`String`/`Void`); no match rethrows
- `finally { }` — runs on normal, caught, and uncaught paths
- runtime errors (e.g. division by zero, calling an undefined function) are catchable values; uncaught failures exit non-zero as `runtime error: <value>` (NXL9001)
- limits (NXL2007/NXL2013 with explicit messages): `return`/`break`/`continue` across a `try` boundary; a catch after a bare catch; `for-in` iteration in the VM (still use `while`); `try`/`catch` in the Rust backend

Still parsed-but-not-lowered (later phases): `struct`/`class`/`enum`/`actor` semantics, `impl` methods, modules/imports resolution, `async`/`await`/`spawn` execution, `yield`/`unsafe`, advanced match patterns.
## Phase 2 — iteration + data (VM authoritative; JS backend for expressions)

Implemented end-to-end in `nir.rs`/`vm.rs`/`backend.rs` (JS), tested in
`language_frontend_tests.rs`:

- `for x in n` iterates `0..n`; `for x in arr` iterates elements; anything
  else throws a catchable "not iterable" error at runtime
- arrays: `[1, 2]` literals (uniform element types; mixed literals rejected
  with NXL2001), `a[i]` reads (catchable out-of-bounds), `a[i] = v` and
  `a[i] += v` writes through `mut` bindings (value semantics: the binding is
  rebound to the updated copy)
- structs: `User { name: "Ada"; }` construction validates exact fields and
  field types; `u.name` reads (catchable unknown field); `u.name = v` writes
  through `mut` bindings; struct fields separate with `;`, not `,`
- enums: `Color.Red`, `Choice.Some(1)` (arity and payload types validated);
  structural `==`/`!=`
- `match` runs only the first matching arm: literals, `_`, bindings, guards,
  `|` alternatives (literal-only), enum variants (`Red()`, `Some(x)` with
  typed payload bindings — a bare `Red` binds, use `Red()` to match),
  struct patterns (`Point { x: 1, y: q }`); statically provable type
  mismatches are NXL3001, dynamic values fall through
- gradual operators: `Unknown`-typed values (untyped params, dynamic reads)
  allow arithmetic/comparison/equality with runtime checking; statically bad
  combinations still fail with NXL2001; `%` works on integers
- limits: tuple/array/set/map literals, tuple/array patterns, bindings in
  `|`-patterns, nested compound patterns beyond one level, `new`, `?.`,
  `for-in` and `try` in the Rust backend, `for-in` loops in emitted JS
  (backend control-flow phase)

## Phase 3 — objects + methods (VM authoritative; JS backend for calls)

Implemented end-to-end in `nir.rs`/`vm.rs`/`backend.rs` (JS), tested in
`language_frontend_tests.rs`:

- classes: `class Counter { let count: Int = 0; fn inc() { ... } }`;
  construction is `Counter()` (auto-constructor from constructor parameters
  plus field defaults) or `Counter(args)` through `init` (arity validated)
- methods: `u.greet("hi")` dispatches dynamically on the receiver's runtime
  type; receiver mutations persist for variable receivers (value semantics:
  aliases do not share state); unknown methods throw a catchable
  `unknown method 'm' on type 'T'`; `self` is passed mutably into every
  method (rebinding `self` itself is rejected)
- `impl` blocks add methods to any nominal type
  (`impl S { ... }`, `impl Greeter for S { ... }`); `interface`/`trait`
  satisfaction is validated (missing/arity-mismatched methods fail loudly)
- actors construct and call synchronously like classes (async dispatch is a
  later phase); enum/struct values accept `impl` methods too
- struct patterns also match class/actor instances read-only
  (`case Person { name: n } => ...`)
- JS backend emits `Type$method` functions plus a method registry with the
  same `[return, self]` write-back protocol as the VM
- limits: `new`, static/singleton calls on types, `?.`, first-class method
  values, inheritance, generics on types, method calls in the Rust backend

## Phase 4 — modules (multi-file programs)

Implemented in `project.rs`/`nir.rs`/`hir.rs`, tested in
`language_frontend_tests.rs` (`project_tests`):

- layout: `import app.math` loads `src/app/math.nxl` next to the entry
  file (only the named file loads; no package files); `as` renames the
  alias; `from app.math import add` binds `add` directly
- access: `math.add(..)` via alias, `app.math.add(..)` via full path
  (longest match wins); types, enum variants, and constructors resolve the
  same way (`math.User { ... }` needs a from-import: object literals only
  spell bare names, while `math.User(..)` and `math.Color.Red` work);
  from-imported nullary variants read as values
- linking: entry keeps the root namespace; other modules own `p::Name`
  (`#` stays reserved for methods); diamonds load once in dependency
  order; cycles fail with NXL2002; missing files are tolerated (use sites
  fail with NXL2002, like single-file mode)
- `nxl run`/`build`/`check` are project-aware through the entry file;
  `nxl test` files stay self-contained; `catch e: T` resolves `T`
  through imports
- limits: no relative imports, no `module`-declaration enforcement
  (informational), no privacy (everything public), no stdlib files yet,
  module globals need bare names, no re-exports

## Phase 5 - concurrency (VM threads; JS honest errors)

Implemented in `nir.rs`/`vm.rs`, tested in `language_frontend_tests.rs` (concurrency tests). Threads run NXL functions; each thread has isolated state (globals included) so programs are data-race-free by construction; threads share values through channels only:

- `spawn f(args)` runs a function on a worker thread and returns a task handle; `spawn obj.method(args)` too (receiver cloned in); anything else evaluates now and wraps in a completed task
- `await t` blocks until the task completes; task failures re-raise catchably (`task failed: ...`); awaiting a plain value returns it
- `channel()` creates a queue: `c.send(v)` never blocks, `c.recv()` blocks until a value arrives or the channel closes (catchable on closed+empty), `c.close()` seals it; `for v in ch` streams until close
- `async { ... }` snapshots visible bindings by value, runs as a worker function, and returns its task; `async fn` is a marker (plain function otherwise)
- actors construct with a `mailbox` channel field (`a.mailbox.send(..)` / `a.mailbox.recv()`); methods stay synchronous: an actor loop is `while true` over the mailbox inside a spawned task
- the run joins outstanding tasks before exiting (first background failure fails the run); awaiting cycles and never-completing tasks hang: always close channels and await
- limits: no `select`, no timeouts/sleep, no cancellation, no shared mutable state (by design), JS/Rust backends reject async with NXL2007 (channel construction emits a stub so sync actor use still compiles to JS)

## Phase 6 - backend control flow

The VM stays authoritative; the JavaScript backend now emits real control flow, so `nxl run` (VM) and the emitted Node.js program agree on loops:

- `while` lowers to a labelled `while (true)` with the condition as `if (!cond) break`; `continue`/`break` become labelled `continue`/`break`
- `for ... in` lowers to a native `for (const __nxl_item of __nxl_iter(seq))`; an `Int` counts `0..n-1`, an array yields its elements, anything else throws `"X is not iterable"` exactly like the VM
- nested loops, loops inside functions and class methods, loops inside `try`, `return` from a loop, and `if` inside a loop all emit natively (each case is checked against the VM by the test suite)
- a name now owns its JavaScript variable, so a rebinding read inside a loop or a function sees the current value (this also fixed stale reads of loop counters)
- loops are recovered from loop metadata recorded by lowering (`Function::loops`), not by guessing at the CFG; control flow outside that shape is refused with NXL2007 instead of mis-emitted

Rust target (`compile_native`): `throw` becomes a panic carrying the rendered message, and `try`/`catch`/`finally` emit `catch_unwind` with a `finally` block that runs on both paths (an uncaught error still propagates). Generated Rust is compiled and executed by the test suite when `rustc` is available. Everything else - branching, loops, `for ... in`, composites, methods, match patterns, concurrency, `catch e: Type` - is refused with NXL2007 and a pointer to a working target instead of silently dropping code.
