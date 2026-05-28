#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
TOOLS="$DIST/tools"
RULES_DIST="$DIST/rules"
BACKEND_SRC="$ROOT/rpf_backend_rs/target/release/rpf_backend_rs"
BACKEND_DST="$TOOLS/rpf_backend_rs"
LAUNCHER_DST="$DIST/redux_rpf_scanner"
RUNTIME_README="$ROOT/README_RUNTIME.md"

mkdir -p "$TOOLS" "$RULES_DIST"

build_launcher_with_cmake() {
    echo "[build] Building C++ launcher with CMake..."
    cmake -S "$ROOT" -B "$ROOT/build"
    cmake --build "$ROOT/build" --config Release

    local built
    built="$(find "$ROOT/build" -type f -name redux_rpf_scanner | head -n 1)"
    if [[ -z "$built" ]]; then
        echo "[build] ERROR: Could not find built redux_rpf_scanner after CMake build." >&2
        exit 1
    fi

    cp "$built" "$LAUNCHER_DST"
}

echo "[build] Building Rust backend..."
cd "$ROOT/rpf_backend_rs"
cargo build --release

echo "[build] Building C++ launcher..."
cd "$ROOT"
if command -v g++ >/dev/null 2>&1; then
    if g++ -std=c++17 -O2 src/cpp/redux_rpf_scanner.cpp -o "$LAUNCHER_DST"; then
        echo "[build] Built launcher with g++."
    else
        echo "[build] g++ build failed. Falling back to CMake..."
        build_launcher_with_cmake
    fi
else
    echo "[build] g++ not found. Falling back to CMake..."
    build_launcher_with_cmake
fi

echo "[build] Copying Rust backend..."
cp "$BACKEND_SRC" "$BACKEND_DST"

echo "[build] Copying example rules..."
if compgen -G "$ROOT/rules/*.example.json" >/dev/null; then
    cp "$ROOT"/rules/*.example.json "$RULES_DIST/"
else
    echo "[build] No example rule files found under $ROOT/rules."
fi

if [[ -f "$RUNTIME_README" ]]; then
    echo "[build] Copying README_RUNTIME.md..."
    cp "$RUNTIME_README" "$DIST/README_RUNTIME.md"
else
    echo "[build] README_RUNTIME.md not found. Skipping copy."
fi

chmod +x "$LAUNCHER_DST" "$BACKEND_DST"

echo "[build] Done."
echo "[build] Dist tree:"
find "$DIST" -print | sort
