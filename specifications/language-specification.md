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

The language includes a core type system with primitives such as:

- Int
- Float
- Bool
- String
- Char
- Byte
- Date
- Time
- Duration
- Void

### 4.2 Composite Types

- arrays
- tuples
- maps / dictionaries
- sets
- structs
- enums
- classes
- traits / interfaces
- option/result types

### 4.3 Type Syntax

```nxl
let count: Int = 10
let name: String = "NEXORA"
let values: Array<Int> = [1, 2, 3]
```

## 5. Variables and Constants

### 5.1 Variables

Variables are declared with `let`.

```nxl
let user = "Ada"
let age: Int = 32
```

Variables are mutable by default unless the `mut` keyword is used to declare mutability separately.

### 5.2 Constants

Constants use `const` and are immutable.

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

- literal patterns
- enum patterns
- tuple patterns
- struct patterns
- wildcard defaults

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

## 12. Generics

```nxl
fn identity<T>(value: T) -> T {
    return value
}
```

Generic constraints may be expressed using traits or interfaces.

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

- `try/catch/finally`
- typed exceptions or errors
- `throw` for explicit failure propagation
- `Result<T, E>` and `Option<T>` patterns

## 14. Async and Await

```nxl
async fn fetchData() -> String {
    let response = await http.get("/api")
    return response.body
}
```

Features include:

- `async fn`
- `await`
- `spawn` for task creation
- `channel` for message passing
- cancellation-aware runtime scheduling

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

- isolated state
- message-based communication
- concurrency boundaries
- scheduling support

## 16. Collections

```nxl
let list = [1, 2, 3]
let map = { "name": "Ava", "role": "dev" }
let set = {1, 2, 3}
```

Collection types include:

- arrays
- lists
- maps / dictionaries
- sets
- tuples

## 17. Strings and Interpolation

```nxl
let greeting = "Hello, ${user.name}!"
let raw = `C:\temp\path`
```

Interpolation is supported in regular string literals and can be used in templates and UI text output.

## 18. Operators

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

Nullability is explicit. Values may be optional and must be handled in type-safe ways.

```nxl
let maybeName: Option<String>
```

## 20. Grammar Intent

The language uses a clean block-based syntax inspired by modern typed languages, but it remains intentionally distinct from them. It supports compact, readable expressions while retaining formal structure and strong typing.

## 21. Summary

NXL is a typed, modern, cross-platform language meant to unify application development across web, backend, database, async systems, UI, and interoperability boundaries. Its syntax is designed to be readable, productive, and extensible while preserving strong type and runtime semantics.

## 22. Compiler Frontend Conformance Status

The Rust compiler frontend parses a broad subset of the syntax above, including declarations, imports, types/generics, control flow, patterns, collections, closures, interpolation, and async/concurrency expressions. Parsing a construct does not mean that it has type-checking, runtime, or backend semantics. Those are implemented in their later production phases.

The current Rust NXL frontend and its tested syntax are described in `compiler/README.md` and `compiler/tests/language_frontend_tests.rs`. Unsupported lowering is reported explicitly. The Python prototype remains an independent reference implementation and is not a conformance oracle for syntax or semantics that it does not implement.

Known specification questions remain: `let` mutability wording conflicts with the separate `mut` modifier; empty `{}` is ambiguous between map and set; the character/string single-quote forms overlap; and interpolation escaping/formatting semantics need a normative definition. P05 must resolve type and mutability rules before those forms are treated as semantically stable.
