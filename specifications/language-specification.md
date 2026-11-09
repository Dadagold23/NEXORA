# NXL Language Specification

## 1. Purpose

NXL is the primary language of the NEXORA platform. It is designed to unify application logic, database access, async execution, UI composition, cross-platform runtime behavior, and foreign interop under a single language model.

NXL is intentionally not a literal copy of existing languages. It adopts useful ideas from established ecosystems while retaining a distinct syntax and semantics.

## 2. Design Goals

- readability and concise authoring
- type safety and explicit semantics
- predictability across native, web, and embedded targets
- support for async, concurrent, and distributed programs
- database-first development ergonomics
- clear interoperability boundaries with foreign systems
- strong tooling support and compiler diagnostics

## 3. Lexical Model

### 3.1 Keywords

The following reserved words are core to the language:

```
let const mut ref borrow unsafe
fn async await spawn actor channel
if else match when case
for while loop break continue
return yield try catch finally throw
module import export from as
struct class interface trait enum
impl type use where
true false null
new in is and or not
```

### 3.2 Identifiers

Identifiers begin with a letter or underscore and continue with letters, digits, or underscores.

```text
identifier := letter_or_underscore { letter_or_digit_or_underscore }
```

Identifiers are case-sensitive.

### 3.3 Comments

```nxl
// single-line comment
/* multi-line comment */
```

Comments are ignored by the compiler except as metadata for tooling and documentation.

### 3.4 Strings

NXL supports:

- single-quoted strings
- double-quoted strings
- raw strings for embedded text
- multiline strings
- interpolation in string literals

```nxl
"hello"
'hello'
`raw text`
"User: ${name}"
```

### 3.5 Numbers

NXL supports integer, floating-point, and numeric literal conventions.

```nxl
42
-10
3.14159
1_000_000
0xFF
0b101010
```

## 4. Types

### 4.1 Primitive Types

Implemented: `Int`, `Float`, `Bool`, `String`, `Void`.

Not implemented: `Char`, `Byte`, `Date`, `Time`, `Duration`. This
specification reserves the names, but the compiler does not accept them as
type annotations today; an annotation using one is reported as an unknown type
(`NXL2004`). Character and single-quote *literals* are parsed, but they are
strings, not values of a distinct `Char` type.

### 4.2 Composite Types

- arrays — Implemented (`Array<T>` and `[T]` annotations, literals, indexing,
  iteration; element type is checked)
- tuples — Parsed but not lowered (no tuple values or destructuring)
- maps / dictionaries — Parsed but not lowered (map literals report
  `NXL2007`); the runtime and `std.map` builtins expose map values to library
  code, but there is no source-level map literal semantics
- sets — Parsed but not lowered
- structs — Implemented (construction, field reads/writes, patterns)
- enums — Implemented (variants, payloads, matching)
- classes — Implemented (auto-constructor, methods, mutation)
- traits / interfaces — Partially implemented: declarations and method
  contracts are checked, `impl Trait for Type` works, and call-site bounds are
  enforced (see §12); there are no trait-typed values or trait objects
- option/result types — Partially implemented: `Option<T>`, `Optional<T>` and
  `Result<T, E>` are recognized as type annotations with structural identity,
  but there is no construction syntax, pattern, or runtime representation for
  them yet

### 4.3 Type Syntax

```nxl
let count: Int = 10
let name: String = "NEXORA"
let values: Array<Int> = [1, 2, 3]
```

## 5. Variables and Constants

### 5.1 Variables

Variables are declared with `let` and are immutable by default. Adding `mut`
after `let` makes the binding assignable.

```nxl
let user = "Ada"
let mut age: Int = 32
```

Assignment to a binding that was not declared `mut` is rejected; the compiler
does not silently promote it.

### 5.2 Constants

Constants use `const` and are immutable. They are currently lowered as
immutable bindings; there is no constant folding or compile-time evaluation, so
initializers still run at the point of declaration.

```nxl
const PI = 3.14159
```

## 6. Ownership and References

NXL keeps ordinary assignment simple: values are copied unless ownership is
transferred explicitly. References are also explicit and are never implicitly
dereferenced.

```nxl
let x = value          // owned binding
let y = move x         // explicit ownership transfer; `x` is unusable afterwards
let a = borrow x       // immutable reference (&T)
let b = ref x          // immutable reference (&T)
let c = borrow mut x   // mutable reference (&mut T)
let d = ref mut x      // mutable reference (&mut T)

print(*a)              // read through a reference
*c = new_value         // write through a reference
```

Rules:

- Assignment copies the value. `move x` transfers ownership out of `x`; using
  `x` before it is reassigned is an error (`NXL2016`).
- A reference is non-owning. It may be read with `*r`, written with `*r = v`,
  held in a local binding, and passed to a function parameter typed `&T` or
  `&mut T`. Direct place updates through a mutable reference
  (`(*r).field = v`, `(*r)[i] = v`) update the owner; deeper paths such as
  `(*r).inner.field = v` are rejected rather than silently dropped.
- Borrows are lexical: a borrow lasts until the end of the block that created
  it. There is no non-lexical lifetime inference.
- Several immutable borrows of the same binding may coexist. A mutable borrow
  is exclusive: no other borrow, read, write, or move of the owner is allowed
  while it is active. Conflicts are reported as `NXL2015`.
- The owner must be mutable to be borrowed mutably (`let mut`).
- A reference may not escape the borrow scope. Returning a reference, storing
  one in a struct field, enum payload, array literal or array element, or
  capturing one in a closure or `async` block is reported as `NXL2017`. This
  applies to reference annotations as well as to inferred reference values.
- Borrowing is limited to plain variable bindings. Borrowing a field, an array
  element, or a temporary is intentionally unsupported and produces a
  diagnostic.
- Reference parameters are borrowed for the duration of the call, so the
  lexical borrow at the call site lasts until the end of the enclosing block.
  Return values are owned values; returning a reference is rejected.
- A reference argument may only be passed to a slot whose *declared* type is a
  reference (`&T` or `&mut T`). Passing one to an unannotated generic
  parameter, a dynamically typed parameter, a value produced by an unresolved
  dynamic call, or any other untrackable slot is rejected with `NXL2017`. The
  declared slot type decides, not the substituted one: inside
  `fn id<T>(x: T)`, `T` bound to `&Int` is still untrackable, because the body
  was lowered without reference semantics. This rule applies to direct calls,
  generic calls, method calls, indirect/closure calls, and closure bodies.
- There are no lifetime parameters and no unsafe escape hatch in the current
  type checker. Unsafe code is parsed as syntax only and has no semantics yet.

Passing these checks is not, by itself, a proof of memory safety. The checker
reasons about lexical scopes and binding names; it does not perform alias
analysis through aggregates, FFI, or concurrent shared state. Type-erased
dynamic paths (channel sends through an `Unknown`-typed receiver, FFI calls,
`Unknown`-typed builtins) are outside the escape analysis: the runtime keeps a
borrowed cell alive, so this cannot dangle, but it is not covered by the lexical
guarantee above.

## 7. Functions and Closures

### 7.1 Function Declaration

```nxl
fn add(a: Int, b: Int) -> Int {
    return a + b
}
```

### 7.2 Expression-bodied Functions

```nxl
fn square(x: Int) -> Int => x * x
```

### 7.3 Closures

```nxl
let multiply = fn (a: Int, b: Int) -> Int {
    a * b
}
```

Closures capture lexical state and may be passed as first-class values.
Closure bodies receive the same ownership, borrow and escape checks as named
functions; capturing a reference is rejected with `NXL2017`, and a reference
parameter passed to an untrackable closure slot is rejected the same way.

## 8. Control Flow

### 8.1 Conditions

```nxl
if condition {
    // body
} else if otherCondition {
    // alternate
} else {
    // fallback
}
```

### 8.2 Loops

```nxl
for item in items {
    print(item)
}

while condition {
    update()
}
```

### 8.3 Break and Continue

```nxl
for value in collection {
    if value == 0 {
        continue
    }
    if value > limit {
        break
    }
}
```

## 9. Pattern Matching

Pattern matching is a first-class control flow construct.

```nxl
match value {
    case 0 => "zero"
    case 1 | 2 => "small"
    case _ => "other"
}
```

Pattern matching supports:

- literal patterns — Implemented
- enum patterns — Implemented, including typed payload bindings
- tuple patterns — Parsed but not lowered
- struct patterns — Implemented (one level of compound pattern)
- wildcard defaults — Implemented
- `|` alternatives — Implemented for literal alternatives only; bindings
  inside an alternative are not supported
- guards — Implemented

Nested compound patterns beyond one level and array patterns are parsed but
not lowered.

## 10. Modules and Imports

```nxl
module http

import std.io
import app.models.user as user
from app.db import UserRepository
```

Rules:

- modules are named namespaces
- imports are explicit
- exports are declared intentionally
- package boundaries are enforced by the module system

## 11. Structs, Classes, Traits, Interfaces

### 11.1 Structs

```nxl
struct User {
    id: Int
    name: String
}
```

### 11.2 Classes

```nxl
class Person {
    let name: String

    fn init(name: String) {
        self.name = name
    }
}
```

### 11.3 Traits and Interfaces

```nxl
trait Printable {
    fn print() -> String
}

interface Repository<T> {
    fn save(item: T)
}
```

Implementation status: declarations, `impl Trait for Type` blocks and
`interface`/`trait` method contracts are checked, and `impl` methods dispatch.
There is no inheritance, no trait object, and no type-level privacy; every
declaration is public. Generic constraints are described in §12.

## 12. Generics

```nxl
fn identity<T>(value: T) -> T {
    return value
}
```

Type arguments may be written explicitly (`identity<Int>(1)`) or inferred from
arguments and struct/enum payloads. Generic substitutions apply to signatures,
fields, payloads and annotations; the compiler does not monomorphize code, so a
generic body is checked against erased parameters.

Constraints use a trailing `where` clause:

```nxl
fn keep<T>(value: T) -> T where T: Greeter {
    return value
}
```

- The constrained name must be one of the function's declared type parameters.
- Each bound must name a declared `trait` or `interface`; unknown, non-trait,
  self-referential and duplicate constraints are rejected with `NXL2014`.
- Bounds are enforced at every explicit or inferred instantiation using the
  program's `impl Bound for Type` blocks. A missing implementation is
  `NXL2014`; it is never silently accepted.
- Inline bounds (`<T: Bound>`) are not supported and produce `NXL1001` with a
  help message pointing at the `where` form.
- Constraints on a type declaration (`struct Box<T> where T: Bound`) are
  rejected with `NXL2014`, because no code path enforces them.

Not implemented: lifetime parameters, associated types, default type
parameters, and constructing an instance of a generic type from the enclosing
function's own type parameters (`fn make<A>() -> Pair<A>` is `NXL2014`).

## 13. Error Handling

NXL uses typed errors and explicit handling.

```nxl
try {
    let x = riskyCall()
    return x
} catch err: DbError {
    log(err)
    throw err
} finally {
    cleanup()
}
```

The language supports:

- `try/catch/finally` — Implemented
- typed exceptions or errors — Partially implemented: an annotated catch
  matches the *runtime type name* (`Int`, `Float`, `Bool`, `String`, `Void`);
  user-declared error types are not matched structurally
- `throw` for explicit failure propagation — Implemented (any value)
- `Result<T, E>` and `Option<T>` patterns — Parsed but not lowered; these
  names work as annotations only

## 14. Async and Await

```nxl
async fn fetchData() -> String {
    let response = await http.get("/api")
    return response.body
}
```

Features include:

- `async fn` — Implemented on the VM as a marker over an ordinary function
- `await` — Implemented on the VM; awaiting a non-task value passes it through
- `spawn` for task creation — Implemented on the VM
- `channel` for message passing — Implemented on the VM
- cancellation-aware runtime scheduling — Experimental: cancellation is
  cooperative through closed channels and bounded waits
  (`timeout`/`recv_timeout`); there is no preemption or cancellation token
  value in the language

The JavaScript and Rust backends reject async/concurrency constructs with
`NXL2007` rather than emitting incorrect code.

## 15. Actors and Concurrency

```nxl
actor Counter {
    let count: Int = 0

    fn increment() {
        self.count += 1
    }
}
```

Actors provide:

- isolated state — Experimental: threads have isolated state, but an actor
  called directly from the current thread shares that thread's state
- message-based communication — Implemented through a `mailbox` channel field
  and a `spawn`ed receive loop; there is no built-in actor scheduler
- concurrency boundaries — Implemented on the VM only; JS and Rust reject
  actor constructs with `NXL2007`
- scheduling support — Not implemented as a language feature; scheduling is
  explicit (`spawn`, channels)

## 16. Collections

```nxl
let list = [1, 2, 3]
let map = { "name": "Ava", "role": "dev" }
let set = {1, 2, 3}
```

Collection types include:

- arrays — Implemented
- lists — Partially implemented: arrays provide the list value; the runtime's
  `std.list` builtins operate on list values, but `List<T>` is not a distinct
  source-level type
- maps / dictionaries — Parsed but not lowered at the syntax level; map values
  are produced by library calls (for example `std.http` responses and
  `std.map` builtins)
- sets — Parsed but not lowered
- tuples — Parsed but not lowered

Map and set literals report `NXL2007` rather than being misinterpreted as
arrays or structs.

## 17. Strings and Interpolation

```nxl
let greeting = "Hello, ${user.name}!"
let raw = `C:\temp\path`
```

Interpolation is supported in regular string literals and can be used in templates and UI text output.

## 18. Operators

Implementation status: arithmetic, comparison, logical, bitwise, shift and
compound-assignment operators are parsed, lowered and executed on the VM and by
the JavaScript backend for their valid operand types. Bitwise and shift
operators work on `Int`. The null-safe access operator is parsed but not
lowered.

### Arithmetic

```text
+ - * / %
```

### Comparison

```text
== != < <= > >=
```

### Logical

```text
and or not
```

### Bitwise

```text
& | ^ << >>
```

### Null-safe and Access

```text
?.
```

Parsed but not lowered (`NXL2007`); it has no semantics yet.

### Assignment

```text
= += -= *= /= %= ||= &&=
```

## 19. Standard Semantics

### 19.1 Expression Model

NXL is expression-oriented, but statements are also valid where appropriate. Most control flow constructs return values.

### 19.2 Scope

Blocks create lexical scope. Variables are scoped to the nearest block, function, or module.

### 19.3 Evaluation Order

Evaluation in NXL is left-to-right for function arguments and index/field access chains unless overridden by specific runtime semantics.

### 19.4 Nullability and Optionals

Nullability is explicit in the type system. `Option<T>` and its alias
`Optional<T>` are recognized as annotations and resolve to one canonical
identity, including when nested (`Option<Array<Optional<Int>>>`). `Result<T, E>`
is likewise an annotation.

Not implemented: optional construction (there is no `Some`/`None`, `Ok`/`Err`
syntax or runtime value), null literals (`null` is parsed but not lowered),
optional pattern matching, and null-safe access.

```nxl
let maybeName: Option<String>
```

## 20. Grammar Intent

The language uses a clean block-based syntax inspired by modern typed languages, but it remains intentionally distinct from them. It supports compact, readable expressions while retaining formal structure and strong typing.

## 21. Summary

NXL is a typed, modern, cross-platform language meant to unify application development across web, backend, database, async systems, UI, and interoperability boundaries. Its syntax is designed to be readable, productive, and extensible while preserving strong type and runtime semantics.

## 22. Compiler Frontend Conformance Status

This section is evidence-based. Each construct is labeled with one of:
Implemented, Partially implemented, Experimental, Parsed but not lowered,
Unsupported, or Not implemented. "Parsed" never implies type checking,
lowering, or runtime support.

- Lexing and parsing — Implemented for the documented forms, including
  declarations, modules/imports, types and generics, control flow, patterns,
  collections, closures, interpolation, and async/concurrency expressions.
- Lowering to NIR — Implemented for the executable subset described in
  `docs/04-language-reference.md`.
- Type checking — Partially implemented: primitive and composite types,
  generic substitution, reference/ownership rules, and `where` bounds are
  checked; there is no trait-object type, no full flow typing, and dynamic
  values remain gradual.
- NIR verification — Partially implemented: the verifier checks control-flow
  structure, value definitions, signatures, operand and result types for the
  instruction kinds the lowerer emits, and handler placement. It is not a
  proof of memory safety.
- Backends — VM implemented (authoritative); JavaScript implemented for the
  documented subset; Rust implemented for a small straight-line subset and
  refuses everything else with `NXL2007`.
- Diagnostics — Partially implemented: the compiler reports a single primary
  diagnostic per compilation with an optional help; it does not recover to
  report multiple independent errors in one run.
- Memory safety — Not claimed. The ownership checker reasons about lexical
  scopes and binding names; there is no alias analysis through aggregates,
  FFI, or concurrent shared state, and no lifetime inference or NLL.

The current Rust NXL frontend and its tested syntax are described in
`compiler/README.md` and `compiler/tests/language_frontend_tests.rs`.
Unsupported lowering is reported explicitly. The Python prototype remains an
independent reference implementation and is not a conformance oracle for syntax
or semantics that it does not implement.

Open specification questions: empty `{}` is ambiguous between map and set (both
forms are parsed but not lowered, so this is not yet observable); single-quote
strings are ordinary strings, so the reserved `Char` type has no literal form;
and interpolation escaping/formatting semantics are not normatively defined.
