#!/usr/bin/env bash
set -euo pipefail

# Reminder: chmod +x scripts/build_linux.sh scripts/package_linux.sh
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"

bash "$ROOT/scripts/build_linux.sh"

echo "Package complete. Dist: $DIST"
