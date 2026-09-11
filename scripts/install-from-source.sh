#!/usr/bin/env bash
#
# Build deslicer from git and install the binary locally.
#
# From a clone (uses the current checkout, including uncommitted fixes):
#   ./scripts/install-from-source.sh
#
# From anywhere (clones main into a temp dir):
#   curl -fsSL https://raw.githubusercontent.com/deslicer/cli/main/scripts/install-from-source.sh | bash
#
# Environment:
#   DESLICER_INSTALL_DIR  destination (default: directory of an existing deslicer,
#                         otherwise ~/.local/bin)
#   DESLICER_REF          git ref to clone when not already in the repo (default: main)
#   DESLICER_REPO_URL     clone URL (default: https://github.com/deslicer/cli.git)

set -euo pipefail

REPO_URL="${DESLICER_REPO_URL:-https://github.com/deslicer/cli.git}"
REF="${DESLICER_REF:-main}"
BINARY="deslicer"
TMP_CLONE=""

log() { printf '\033[0;34m[deslicer-source]\033[0m %s\n' "$1"; }
fail() { printf '\033[0;31m[deslicer-source]\033[0m %s\n' "$1" >&2; exit 1; }

cleanup() {
  rm -rf "${TMP_CLONE:-}"
}

require_rust() {
  if command -v cargo >/dev/null 2>&1; then
    return
  fi
  fail "cargo not found. Install Rust via https://rustup.rs and re-run."
}

repo_root() {
  local here
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  if [ -f "${here}/Cargo.toml" ] && grep -q '^name = "deslicer-cli"' "${here}/Cargo.toml"; then
    echo "${here}"
    return 0
  fi
  return 1
}

resolve_install_dir() {
  local existing
  if [ -n "${DESLICER_INSTALL_DIR:-}" ]; then
    echo "${DESLICER_INSTALL_DIR}"
    return
  fi
  if existing="$(command -v "${BINARY}" 2>/dev/null)"; then
    dirname "${existing}"
    return
  fi
  echo "${HOME}/.local/bin"
}

install_binary() {
  local src="$1" dest_dir="$2"
  mkdir -p "${dest_dir}"
  if [ -w "${dest_dir}" ]; then
    install -m 0755 "${src}" "${dest_dir}/${BINARY}"
    return
  fi
  log "escalating with sudo to write ${dest_dir}"
  sudo install -m 0755 "${src}" "${dest_dir}/${BINARY}"
}

prepare_tree() {
  local root
  if [[ "${BASH_SOURCE[0]:-}" == "${0}" ]] && root="$(repo_root)"; then
    echo "${root}"
    return
  fi

  require_git
  TMP_CLONE="$(umask 077; mktemp -d)"
  trap cleanup EXIT
  log "cloning ${REPO_URL} (${REF})"
  git clone --depth 1 --branch "${REF}" "${REPO_URL}" "${TMP_CLONE}"
  echo "${TMP_CLONE}"
}

require_git() {
  command -v git >/dev/null 2>&1 || fail "git is required to install from source"
}

main() {
  local root dest bin
  require_rust
  dest="$(resolve_install_dir)"
  root="$(prepare_tree)"

  log "building release binary in ${root}"
  (cd "${root}" && cargo build --release --locked)
  bin="${root}/target/release/${BINARY}"
  [ -f "${bin}" ] || fail "cargo build did not produce ${bin}"

  install_binary "${bin}" "${dest}"
  log "installed $("${dest}/${BINARY}" --version) to ${dest}/${BINARY}"
  log "this is a source build, not a GitHub Release. Use deslicer update only after a tagged release exists."
}

if [[ "${BASH_SOURCE[0]:-}" == "${0}" ]]; then
  main "$@"
fi
