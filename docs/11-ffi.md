# FFI Bridges (P17)

Generalized bridge: one honest mechanism per guest rather than a fake universal adapter.

Implemented (subprocess, line-delimited JSON-RPC, `protocol: "1"` frame):
- Python (`ffi::open("python", script)`) — tested
- Node.js (vm.runInThisContext dispatch) — tested
- PHP — tested

Protocol: guest script defines functions; harness reads `{"protocol","function","args"}` lines and writes `{"protocol","result"}` or `{"protocol","error"}`. Guest exceptions become `FfiError::Guest`. Host-side types cross as JSON values (number/string/bool/null/array/object); ownership = full copy per call; calls are serialized per bridge; guest runs out-of-process (security boundary).

`nxl bridge` prints the honest table: C#/.NET pending, C ABI pending native backend, Rust rlib deferred, Go/Java/Dart blocked (no toolchain), WASM blocked (backend P07).
