#!/usr/bin/env bash
# Shell tests for scripts/install-from-source.sh helpers.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/install-from-source.sh
source "${ROOT}/scripts/install-from-source.sh"

fail() { echo "FAIL: $1" >&2; exit 1; }
pass() { echo "ok - $1"; }

test_explicit_install_dir() {
  DESLICER_INSTALL_DIR="/tmp/deslicer-source-test"
  [ "$(resolve_install_dir)" = "/tmp/deslicer-source-test" ] || fail "explicit dest"
  pass "resolve_install_dir honors DESLICER_INSTALL_DIR"
}

test_explicit_install_dir
echo "install-from-source tests passed"
