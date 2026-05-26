#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
TOOLS="$DIST/tools"

mkdir -p "$TOOLS"

echo "Building Rust backend..."
cd "$ROOT/rpf_backend_rs"
cargo build --release

echo "Building C++ launcher..."
cd "$ROOT"
g++ -std=c++17 -O2 src/cpp/redux_rpf_scanner.cpp -o "$DIST/redux_rpf_scanner"

cp "$ROOT/rpf_backend_rs/target/release/rpf_backend_rs" "$TOOLS/rpf_backend_rs"

echo "Done."
echo "Dist: $DIST"
