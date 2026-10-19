# FFI Bridges (P17)

Generalized bridge: one honest mechanism per guest rather than a fake universal adapter.

## C ABI (`extern` blocks) — implemented (VM only)

NXL can call C functions in dynamically loaded shared libraries:

```
extern "kernel32.dll" {
    fn GetTickCount64() -> Int;
    fn lstrlenA(s: String) -> Int as "lstrlenA";
}
print(GetTickCount64());
print(lstrlenA("hello"));
```

- **Types.** `Int` → `int64_t`, `Float` → `double`, `String` → NUL-terminated `const char *`, `Bool` → integer-class 0/1; an omitted `-> T` means `void`. Parameters and returns are scalars only; arity is capped at 4 (compile-time error, NXL2001). Compound types, pointers, varargs, and C++ mangling are out of scope.
- **Naming.** The function name as written is the NXL-visible callee; `as "symbol"` aliases the C symbol when it differs.
- **Backends.** Only the VM (`nxl run`, `nxlc run`) performs foreign calls. The JavaScript and Rust backends reject `ForeignCall` with **NXL2007** — no native codegen links libraries yet.
- **Runtime errors are catchable.** Library load failure = `NXL11013`, missing symbol = `NXL11014`, argument marshalling (wrong runtime type, interior NUL byte) = `NXL11015`, bad return (NULL string pointer) = `NXL11016`, unsupported signature reaching the runtime = `NXL11017`. The caught value is the string `"error[CODE]: message"`.
- **Compile-time errors.** Unsupported parameter/return type or arity > 4 = NXL2001; wrong argument count = NXL2009; argument whose NXL type doesn't match the declared scalar = NXL2001; duplicate extern name = NXL2005.
- **Safety.** An `extern` signature is trusted exactly like linking a native binary: a wrong declaration is undefined behaviour, not a catchable error. Library names and symbols must be string literals (no runtime composition). Libraries load once per process and are never unloaded. Foreign code has full process privileges — there is no sandbox.

Limitations (documented, deliberate): negative 32-bit `int` C returns are sign-extended only for 64-bit C types — a C function returning a 32-bit `int` with the high bit set is misread as a positive value (upper bits zero-extended); callbacks into NXL, struct/union by value, `long double`, `wchar_t`, and static linking are deferred.

## Guest-language bridges (subprocess JSON-RPC) — implemented

Protocol: guest script defines functions; harness reads `{"protocol","function","args"}` lines and writes `{"protocol","result"}` or `{"protocol","error"}`. Guest exceptions become `FfiError::Guest`. Host-side types cross as JSON values (number/string/bool/null/array/object); ownership = full copy per call; calls are serialized per bridge; guest runs out-of-process (security boundary).

Implemented (subprocess, line-delimited JSON-RPC, `protocol: "1"` frame):
- Python (`ffi::open("python", script)`) — tested
- Node.js (vm.runInThisContext dispatch) — tested
- PHP — tested

`nxl bridge` prints the honest table: C#/.NET pending, Rust rlib deferred, Go/Java/Dart blocked (no toolchain), WASM blocked (backend P07).
