# REPL and development server

Two developer-facing commands, what they actually do, and what they do not do.

## `nxl repl` — interactive session

```bash
$ nxl repl
>>> let x = 10;
>>> x * 2
20
>>> fn twice(n: Int) -> Int { return n * 2; }
>>> twice(21)
42
>>> let mut total = 0;
>>> for i in 4 { total = total + i; }
>>> total
6
>>> print("hello");
hello
>>> .help
>>> .exit
```

Behaviour:

- Expressions are evaluated and their value shown; `print(...)` prints only
  what the program printed.
- Declarations (`let`, `fn`, `struct`, `class`, …) stay available to later
  inputs, and side effects from earlier statements are replayed so `total`
  above is `0 + 1 + 2 + 3`.
- Inputs that only printed are not replayed, so their output is not repeated.
- Multi-line blocks are collected until the braces balance, so `fn main() { … }`
  can be typed across lines.
- Errors are reported against your own line and the session continues.
- Commands: `.help`, `.clear` (forget the session), `.exit` / `.quit`.

Limits: the session is replayed rather than kept live, so a program with
non-deterministic input (time, randomness) is re-executed on each evaluation.
There is no history editing, tab completion or multi-line editing yet.

## `nxl dev` — development server

```bash
nxl dev                 # http://127.0.0.1:3000
nxl dev --port 4000     # different port
nxl dev --no-watch      # do not recompile on change
```

The server:

- compiles the project to JavaScript and serves it at `/app.js`,
- serves a host page at `/` and a reload client at `/reload.js`,
- watches every `.nxl` file, recompiles when one changes, and pushes a
  reload over Server-Sent Events at `/reload`,
- shows a readable compile-error page instead of the app when the project
  breaks, and recovers on the next fix,
- reports `503` from `/health` while the project does not compile, so a
  deployment probe sees the truth.

It binds loopback by default and prints the URL it is listening on.

### What `dev` is not

- It is **not** a server-side NXL runtime. The program runs in the browser via
  the JavaScript backend, because NXL has no HTTP types in the language yet and
  so no handler can be written in NXL. Server-side demonstrations live behind
  `nxl serve` (NUF).
- Concurrency (`spawn`, `await`, channels) is **not available in the browser**;
  the compiler refuses it for JavaScript targets. Run such programs with
  `nxl run` on the VM.
- There is no bundling, no minification, no proxying and no HTTPS.

## Testing

`compiler/tests/repl_tests.rs` covers the session semantics;
`compiler/tests/dev_tests.rs` starts a real HTTP server on an ephemeral port
and asserts the page, the script, health, 404s, and that a broken project is
surfaced and then recovers.