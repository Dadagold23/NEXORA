# NXL Build & Deployment Engineering

How a source repository becomes a running application, how it is configured
per environment, and how it is rolled out and rolled back without destroying
data. This covers [Phase 13](../advanced.txt) requirements.

> Honesty rule: every section records what is IMPLEMENTED vs NOT IMPLEMENTED.

## Build profiles

**IMPLEMENTED.** `nxl build` writes a debug artifact; `nxl build --release`
writes a release one. The profile is reported in `--json` output.

| Profile | Output | Use |
| --- | --- | --- |
| debug (default) | `build/debug/<name>.js` | development, keeps readable output |
| release | `build/release/<name>.js` | deployment, optimised |

A separate *testing* profile (debug symbols + no minification) is **NOT
IMPLEMENTED**; use `build` when a test run needs readable output.

## Application artifact and runtime dependencies

**IMPLEMENTED.** A deployable NXL application is:

```
<project>/
  nxl.toml           manifest: package, [web], [deploy], [env.*]
  src/               NXL source
  build/<profile>/   the compiled JavaScript artifact
```

The artifact is plain JavaScript. Its only runtime dependency is a JavaScript
runtime (Node 18+). The NXL compiler is a *build* dependency, not a runtime
one: the deployed host does not need `nxc`.

## Environment separation

**IMPLEMENTED.** `NXL_ENV` selects the deployment environment; the known
values are `development` (default), `test`, and `production`. Any other value
is refused at startup (NXL12011) rather than silently falling back.

`nxl.toml` can hold per-environment overrides. `[env.<name>]` keys are merged
over the base `[web]` keys when `NXL_ENV` names that environment:

```toml
[web]
host = "127.0.0.1"
port = 8080

[env.production]
host = "0.0.0.0"
port = 80
```

## Startup validation

**IMPLEMENTED.** `nxl serve` and `nxl dev` validate configuration before
binding, reporting every problem at once (NXL12011):

- `web.port` must be a valid, non-zero port number.
- `web.host` must be a non-empty, whitespace- and slash-free host.
- `[deploy] required_env` lists environment variables the application refuses
  to start without. Missing ones are reported **by name**; values never reach
  a diagnostic or a log line.

```toml
[deploy]
required_env = ["DATABASE_URL", "SESSION_SECRET"]
```

```bash
export DATABASE_URL="postgres://…"   # supplied by the service manager
export SESSION_SECRET="…"
nxl serve                            # refuses to start if either is unset
```

**IMPLEMENTED.** Graceful shutdown: `nxl serve` drains in-flight requests on
Ctrl+C (SIGINT) and, on Unix, SIGTERM, then exits.

**NOT IMPLEMENTED:** config-file format other than `nxl.toml`; secret-vault
integration (values are read from the process environment only).

## Structured logging, health checks, error reporting

**IMPLEMENTED.**

- The access log records only `method path status`. The query string and
  headers are never logged, because both routinely carry tokens and session
  identifiers.
- `GET /api/health` is the deployment gate: a rollout is live once it answers
  200 through the proxy.
- Runtime errors render with their NXL/NDB error code so a log line is
  actionable.

**NOT IMPLEMENTED:** machine-readable (JSON) log lines; metrics endpoint.

## Secrets and least privilege

**IMPLEMENTED.**

- Connection diagnostics omit the password (`display_target`); driver error
  text is passed through the secret redactor before it reaches the operator.
- Only environment variable *names* appear in diagnostics, never values.
- The reference systemd unit runs as an ephemeral `DynamicUser` with
  `NoNewPrivileges`, a private `/tmp`, a read-only filesystem outside the
  app directory, and reads secrets from a root-owned, mode-`0600` env file.

**Rule:** never commit a password to `nxl.toml`. The `[database] password`
key exists for local development only.

## Deployment templates

**IMPLEMENTED.** Reference templates live in [`templates/deploy/`](../templates/deploy/):

| File | Purpose |
| --- | --- |
| `nxl.service` | systemd unit (least-privilege, env-file secrets, graceful stop) |
| `nginx.conf` | reverse proxy with the `/api/health` gate |
| `Dockerfile` | container image — evaluated, **not** mandatory |

Containers are one packaging choice; NXL artifacts also run under the systemd
unit or directly under Node. Nothing in the language or toolchain assumes a
container.

**NOT IMPLEMENTED:** an `nxl deploy <target>` command for remote targets; a
defined catalogue of approved target environments.

## Deployment procedure

1. **Build** on CI or a build host: `nxc build --release`. The artifact is
   reproducible from the same source and lockfile.
2. **Stage** the artifact and `nxl.toml` into a release directory
   (`/opt/nxl-app/releases/<timestamp>`), keeping the previous release.
3. **Configure** the service: the env file (`/etc/nxl/app.env`) supplies
   secrets; `NXL_ENV=production` selects the overlay.
4. **Gate**: `curl -f http://127.0.0.1/api/health` must answer 200.
5. **Switch** the live symlink to the new release and restart the service.
6. **Verify** the health gate again; on failure, roll back.

## Smoke tests and rollback

**IMPLEMENTED (as procedure).** After switching:

```bash
curl -f http://127.0.0.1/api/health   # must return 200
```

To roll back:

1. Repoint the live symlink at the previous release directory and restart.
2. If the release included a migration, roll the schema back to match the old
   code:
   ```bash
   nxl db:status        # which migrations are ahead
   nxl db:rollback      # step back one migration; repeat as needed
   ```

**Destructive commands are guarded** (see below), so a rollback script cannot
drop a production database by accident.

## Destructive-command guards

**IMPLEMENTED.** `db:drop`, `db:fresh`, and `db:refresh` permanently destroy
data, so they refuse to run without explicit confirmation (NXL12010):

- **Interactive** (a terminal): the command prompts, naming the target:
  `db:drop will permanently destroy app.db. Type 'yes' to continue:`
  Anything other than `yes` aborts; nothing is changed.
- **Non-interactive** (scripts, CI): the command fails immediately. Pass
  `--yes` to confirm: `nxl db:fresh --seed --yes`.

A deployment tool must never silently destroy production data.

## Containers (evaluated, not mandatory)

**EVALUATED.** `templates/deploy/Dockerfile` is a working reference. The
runtime image needs only the built artifact and a JavaScript runtime. Use
containers if your platform requires them; do not if it does not.

## Platform packaging and installers

**IMPLEMENTED (test-time only).** The release scripts build installer
artifacts and run a smoke test against a freshly scaffolded project. See
[18-contributing-release.md](18-contributing-release.md).

## Summary of error codes

| Code | Meaning |
| --- | --- |
| NXL12010 | destructive command needs confirmation (`--yes` or prompt) |
| NXL12011 | startup validation failed (config or missing env vars) |
