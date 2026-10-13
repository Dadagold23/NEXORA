# Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `error[NXL12007]: expected field name` on struct | Parser wants no trailing comma in field list | Write `struct S { id: Int }` (no trailing comma) |
| `error[NXL12007]: cannot assign to immutable binding 'i'` | `let` is immutable | Use `let mut i = ...` |
| `error[NDB14005]: driver 'postgres' is NOT IMPLEMENTED` | Only SQLite is live | Switch `[database].driver` to `"sqlite"` |
| `nxl db:migrate` reports "migration '...' failed and was rolled back" | SQL error inside up-section | Fix the SQL; the partial transaction is undone and not recorded |
| `error[NXL17002]: bridge I/O error` from FFI | Guest program closed early | Ensure your script defines real functions and never calls `exit()` except via `__exit__` |
| PHP bridge returns `invalid JSON from guest` | Bytes emitted before `<?php` | The harness must start exactly with `<?php` |
| `nxl debug` shows zero breakpoint hits | Breakpoint line never executes | Pick a line inside the executed region; see `lines executed` output |
| `nxl bench` writes under `%TEMP%` | Expected | Bench artifacts are in an OS temp dir and cleaned up |
