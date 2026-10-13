#!/usr/bin/env bash
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/compiler"

echo "== examples/hello =="
cargo run --quiet -- check "$ROOT/examples/hello/src/main.nxl"

echo "== examples/blog =="
TMP="$(mktemp -d)"
cp -r "$ROOT/examples/blog/nxl.toml" "$TMP/"
cp -r "$ROOT/examples/blog/database" "$TMP/database"
(cd "$TMP" && cargo run --manifest-path "$ROOT/compiler/Cargo.toml" --quiet -- db:fresh --seed)
(cd "$TMP" && cargo run --manifest-path "$ROOT/compiler/Cargo.toml" --quiet -- db:migrate)
rm -rf "$TMP"

echo "== examples/api =="
echo "start 'nxl serve' from examples/api and curl http://localhost:8080/api/health"
