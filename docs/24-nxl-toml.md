# The `nxl.toml` project manifest

Every NXL project is a directory with `nxl.toml` at its root. This document is
the complete specification: every section, every key, which command consumes
it, and what happens when it is wrong. The parser is deliberately strict — a
typo must fail loudly, not silently change what the project does.

## Sections at a glance

| Section | Consumed by | Purpose |
| --- | --- | --- |
| `[package]` | `check`, `build`, `run`, `test`, `doctor`, `pkg` | identity, entry point, build target |
| `[web]` | `serve`, `dev` | bind address, layout, 404 page |
| `[env.<name>]` | `serve`, `dev` (via `NXL_ENV`) | per-environment overrides of `[web]` |
| `[[route]]` | `serve`, `dev` | the HTTP routes the project serves |
| `[database]` | `db:migrate`, `db:fresh`, `db:seed`, `db:rollback` | NDB connection |
| `[dependencies]` | `pkg install/add/remove/update`, project resolution | NXP packages |

## `[package]`

```toml
[package]
name = "my-app"          # string, default "project"
version = "0.1.0"        # string, default "0.1.0"
description = "..."      # optional string
authors = "Ada, Bob"     # optional, comma-separated
entry = "src/main.nxl"   # default "src/main.nxl"
edition = "2024"         # default "2024"
target = "js"            # "js" (default) | "vm" | "wasm"
```

| Key | Default | Meaning |
| --- | --- | --- |
| `name` | `project` | Project identity; used by `pkg publish` and reports. |
| `version` | `0.1.0` | Reported and published with the package. |
| `description` | — | One-line summary, shown by `pkg info`. |
| `authors` | — | Comma-separated list, published with the package. |
| `entry` | `src/main.nxl` | The source file `check`/`build`/`run` compile. |
| `edition` | `2024` | Recorded for edition gating. Every program compiles as edition 2024 today; the field exists so a future edition can be selected explicitly. |
| `target` | `js` | Which artifact `nxlc build` emits: the JavaScript bundle (`js`), VM bytecode (`vm`), or a core-WebAssembly module (`wasm`, scalar subset — see docs/23-webassembly.md). |

**Validation:** an unknown key in `[package]` fails with
`error[NXL12012]: [package] unknown key '<key>' (known keys: ...)`, and an
unknown `target` value fails with `error[NXL12012]: [package] target: unknown
target '<value>' (available: js, vm, wasm)`. An unknown target used to fall
back to `js` silently; it no longer does.

## `[web]`

```toml
[web]
host = "127.0.0.1"       # default "127.0.0.1"
port = 8080              # default 8080
layout = "main.nxlhtml"  # optional: wrap every page in this layout
not_found = "404.nxlhtml" # optional: real-404 page for unmatched paths
```

`port` that is not a number, a bind address that is not reachable, or
required deployment environment variables that are unset all fail `serve`
startup with `error[NXL12011]: startup validation failed: ...` — the server
never starts half-configured (docs/21-deployment.md covers the environment
side).

## `[env.<name>]`

```toml
[env.production]
host = "0.0.0.0"
```

A `[env.<name>]` section overrides the base `[web]` keys when that environment
is active. The environment comes from the `NXL_ENV` variable and defaults to
`development`. `NXL_ENV` naming a section the manifest does not declare is an
error — a typo like `NXL_ENV=productionn` must not quietly serve development
configuration.

## `[[route]]`

```toml
[[route]]
method = "GET"           # GET (default) or POST
path = "/posts/:id"      # must start with "/"; ":name" becomes a variable
page = "post.nxlhtml"    # required: the page template to render
validate = ["name", "email"]  # POST only: required form fields
```

| Key | Rules |
| --- | --- |
| `method` | `GET` or `POST` (case-insensitive); anything else is a line-numbered error. |
| `path` | Must start with `/`. A `:segment` binds a variable of that name for the page (e.g. `id` above). |
| `page` | Required. A route without a page is an error. |
| `validate` | List of form field names that must be present. A POST submission missing one is answered with **422** and the page redrawn with `error` set. |

Every problem with a route — unknown key, unknown table (`[[anything]]` other
than `[[route]]`), missing page — is reported with its line number:
`line N: unknown route key '...' (method, path, page, validate)`. There is no
convention that invents routes behind your back; the routes in the manifest
are the routes the server serves. `GET /openapi.json` describes them.

## `[database]`

```toml
[database]
driver = "sqlite"                    # sqlite | postgres | mysql | mariadb
path = "database/database.sqlite"    # file path (sqlite)
host = "127.0.0.1"                   # server address (server drivers)
port = "5432"
```

Consumed by the `db:*` commands. An unsupported driver fails with
`error[NDB14005]: driver '<d>' is not supported (supported: sqlite, postgres,
mysql, mariadb)` — see docs/06-ndb.md.

## `[dependencies]`

```toml
[dependencies]
some-package = "1.0.0"
```

Consumed by the package commands (`pkg install`, `pkg add`, `pkg remove`,
`pkg update`); `nxlc new` resolves and locks the declared dependencies before
it returns. The registry is currently a local directory (`$NXL_REGISTRY` or
`~/.nexora/registry`) — a hosted registry is not implemented (docs/09-nxp.md).

## What is *not* a manifest key

- **Build profile** — comes from the command line: `nxlc build --release`
  (default is the debug profile). There is no `profile` key.
- **Build target on the CLI** — `nxlc build` has no `--target` flag; the
  artifact is chosen by `target` in `[package]`.
- **`driver`/`path` in `[package]`** — they belong in `[database]` and are
  rejected in `[package]` by the unknown-key check.

## Error codes

| Code | Meaning |
| --- | --- |
| `NXL12012` | Invalid `[package]` key or target value. |
| `NXL12011` | Startup validation failed (web config, environment). |
| `NDB14005` | Unsupported database driver. |
| line-numbered `SiteError` | Invalid `[[route]]` / `[web]` route table. |
