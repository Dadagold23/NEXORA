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
- serves the project's own page at `/`: `public/index.nxl` first, then
  `public/index.html`, and finally its own generated page when a project ships
  no front end,
- serves every other file under `public/` at its own path,
- injects `/app.js` and `/reload.js` into a page that does not load them, so a
  hand-written page does not silently end up empty,
- serves a reload client at `/reload.js`,
- watches every `.nxl` file, recompiles when one changes, and pushes a
  reload over Server-Sent Events at `/reload`,
- shows a readable compile-error page instead of the app when the project
  breaks, and recovers on the next fix,
- reports `503` from `/health` while the project does not compile, so a
  deployment probe sees the truth.

Paths under `public/` are canonicalised before they are read, so `..` cannot
climb out of the directory and a symlink pointing outside it is refused.

It binds loopback by default and prints the URL it is listening on.

### Web projects

`nxl new web <name>` scaffolds a project whose page is itself a NXL program.
`public/index.nxl` is compiled and run on the server and whatever it prints is
served as HTML, so the page imports the same modules as the app:

```nxl
from app.config import app_name;

fn main() {
    print(`<h1>`);
    print(app_name());
    print(`</h1>`);
}
```

Change `app_name()` and both the app and the page show the new name. Run
`nxl run public/index.nxl` to see the page without starting a server.

`public/index.html` still works if a project really does want a hand-written
page; the NXL page takes precedence, and a page that fails to compile reports
why rather than silently falling back to the generated one.

In the browser the page stays live: NXL has no DOM types, so `print()`
compiles to `console.log` and the template's `public/bootstrap.js` takes it over
before the program runs, turning the boot log into the page:

| The program prints   | The page does                             |
| -------------------- | ----------------------------------------- |
| `booting <name>`     | sets the hero heading and the document title |
| `tagline <text>`     | sets the hero sub-heading                 |
| `route  <path>`      | appends to the served-routes list         |
| `feature  <text>`    | appends to the feature list               |
| `READY`              | clears the splash screen                  |

The splash screen is cleared when the program says it is ready, not after a
timer. If the program throws, the splash stays up and the reason is shown, so a
broken app never looks healthy.

`scripts/verify-web-page.js <project>` checks all of that without a browser by
running `bootstrap.js` and the compiled bundle against a minimal DOM; the test
suite runs it against the scaffolded template.

### What `dev` is not

- It is **not** a server-side NXL runtime. The program runs in the browser via
  the JavaScript backend, because NXL has no HTTP types in the language yet and
  so no handler can be written in NXL. Server-side demonstrations live behind
  `nxl serve` (NUF).
- It is **not** a page builder. It serves whatever is under `public/`, renders
  `public/index.nxl` when there is one, and does not compile templates,
  process assets or rewrite HTML.
- The page is rendered by running a program on the VM, so a page cannot use
  concurrency either, and rendering is per request rather than cached.
- Concurrency (`spawn`, `await`, channels) is **not available in the browser**;
  the compiler refuses it for JavaScript targets. Run such programs with
  `nxl run` on the VM.
- There is no bundling, no minification, no proxying and no HTTPS.

## Testing

`compiler/tests/repl_tests.rs` covers the session semantics;
`compiler/tests/dev_tests.rs` starts a real HTTP server on an ephemeral port
and asserts the page, the script, health, 404s, that `public/` is served, that
paths cannot escape it, and that a broken project is surfaced and then recovers.