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
