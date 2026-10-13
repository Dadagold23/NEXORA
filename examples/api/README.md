# api example — NUF demo server

```bash
cd examples/api
cargo run --manifest-path ../../compiler/Cargo.toml -- serve
```

Then in another terminal:

```bash
curl http://127.0.0.1:8080/api/health
curl http://127.0.0.1:8080/api/hello
curl -X POST http://127.0.0.1:8080/api/echo
```

WebSocket: `wscat -c ws://127.0.0.1:8080/ws/echo`

This is the framework's built-in demo routes. NXL-authored `route { }` blocks are NOT IMPLEMENTED (blocked on the native backend, P07).
