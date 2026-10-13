# NEXORA example applications

## hello — Hello World CLI
bash cargo run --quiet -- check ../examples/hello/src/main.nxl
(`nxl check` exercises the full lexer/parser/NIR pipeline.)

## blog — NDB database app
bash
TMP=$(mktemp -d) && cp -r examples/blog/* "$TMP" && cd "$TMP"
cargo run --manifest-path ../../compiler/Cargo.toml -- db:fresh --seed
cargo run --manifest-path ../../compiler/Cargo.toml -- db:migrate

## api — REST-style server demo
bash
cd examples/api
cargo run --manifest-path ../../compiler/Cargo.toml -- serve   # blocks
curl http://127.0.0.1:8080/api/hello

## NOT IMPLEMENTED targets
CRUD over NDB models, auth, WebSocket clients, desktop/WASM — wait on P07 native backend. Do NOT demo them with fake shells.
