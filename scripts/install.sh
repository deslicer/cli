#!/usr/bin/env bash
#
# deslicer CLI installer / updater.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/deslicer/cli/main/scripts/install.sh | bash
#
# Environment overrides:
#   DESLICER_VERSION      release tag to install (default: latest stable, e.g. v1.2.3)
#   DESLICER_INSTALL_DIR  destination directory (default: /usr/local/bin)
#
# Re-running the script updates an existing installation in place. Every
# download is verified against the release's .sha256 sidecar before install.

set -euo pipefail

REPO="deslicer/cli"
BINARY="deslicer"
INSTALL_DIR="${DESLICER_INSTALL_DIR:-/usr/local/bin}"

log() { printf '\033[0;34m[deslicer-install]\033[0m %s\n' "$1"; }
fail() { printf '\033[0;31m[deslicer-install]\033[0m %s\n' "$1" >&2; exit 1; }

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}" in
    Linux)
      case "${arch}" in
        x86_64) echo "x86_64-unknown-linux-musl" ;;
        aarch64 | arm64) echo "aarch64-unknown-linux-musl" ;;
        *) fail "unsupported Linux architecture: ${arch}" ;;
      esac
      ;;
    Darwin)
      case "${arch}" in
        x86_64) echo "x86_64-apple-darwin" ;;
        arm64) echo "aarch64-apple-darwin" ;;
        *) fail "unsupported macOS architecture: ${arch}" ;;
      esac
      ;;
    *)
      fail "unsupported OS: ${os} (on Windows, download the .zip from https://github.com/${REPO}/releases)"
      ;;
  esac
}

resolve_version() {
  if [ -n "${DESLICER_VERSION:-}" ]; then
    echo "${DESLICER_VERSION}"
    return
  fi
  # GitHub's "latest" release excludes prereleases by definition.
  local tag
  tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4)" || true
  [ -n "${tag}" ] || fail "could not resolve latest release tag from GitHub API"
  echo "${tag}"
}

sha256_check() {
  # $1 = file, $2 = sidecar containing "<hex>  <name>"
  local expected actual
  expected="$(awk '{print $1}' "$2")"
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$1" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$1" | awk '{print $1}')"
  else
    fail "neither sha256sum nor shasum found; cannot verify download"
  fi
  [ "${expected}" = "${actual}" ] || fail "checksum mismatch: expected ${expected}, got ${actual}"
}

main() {
  local target version artifact base_url tmp_dir current_version=""
  target="$(detect_target)"
  version="$(resolve_version)"
  artifact="${BINARY}-${target}.tar.gz"
  base_url="https://github.com/${REPO}/releases/download/${version}"

  if command -v "${BINARY}" >/dev/null 2>&1; then
    current_version="$("${BINARY}" --version 2>/dev/null | awk '{print $2}')" || true
    if [ "v${current_version}" = "${version}" ]; then
      log "${BINARY} ${version} is already installed and up to date"
      exit 0
    fi
    log "updating ${BINARY} ${current_version:-unknown} -> ${version}"
  else
    log "installing ${BINARY} ${version} (${target})"
  fi

  tmp_dir="$(umask 077; mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' EXIT

  log "downloading ${artifact}"
  curl -fsSL -o "${tmp_dir}/${artifact}" "${base_url}/${artifact}"
  curl -fsSL -o "${tmp_dir}/${artifact}.sha256" "${base_url}/${artifact}.sha256"
  sha256_check "${tmp_dir}/${artifact}" "${tmp_dir}/${artifact}.sha256"
  log "checksum verified"

  tar -C "${tmp_dir}" -xzf "${tmp_dir}/${artifact}"
  [ -f "${tmp_dir}/${BINARY}" ] || fail "archive did not contain the ${BINARY} binary"

  if [ -w "${INSTALL_DIR}" ]; then
    install -m 0755 "${tmp_dir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
  else
    log "escalating with sudo to write ${INSTALL_DIR}"
    sudo install -m 0755 "${tmp_dir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
  fi

  log "installed $("${INSTALL_DIR}/${BINARY}" --version) to ${INSTALL_DIR}/${BINARY}"
}

main "$@"
