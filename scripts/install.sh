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
#   GITHUB_TOKEN / GH_TOKEN  optional GitHub API token for latest-tag resolution
#
# Re-running the script updates an existing installation in place. Every
# download is verified against the release's .sha256 sidecar before install.

set -euo pipefail

REPO="deslicer/cli"
BINARY="deslicer"
INSTALL_DIR="${DESLICER_INSTALL_DIR:-/usr/local/bin}"
TMP_DIR=""
PINNED_VERSION_ENV="DESLICER_VERSION" # pragma: allowlist secret
INSTALL_DIR_ENV="DESLICER_INSTALL_DIR" # pragma: allowlist secret

log() { printf '\033[0;34m[deslicer-install]\033[0m %s\n' "$1"; }
fail() { printf '\033[0;31m[deslicer-install]\033[0m %s\n' "$1" >&2; exit 1; }

cleanup_tmp_dir() {
  rm -rf "${TMP_DIR:-}"
}

validate_tag() {
  local tag="$1"
  case "${tag}" in
    v[0-9]*)
      case "${tag}" in
        *[!a-zA-Z0-9.v-]*) return 1 ;;
        */* | *\\* | *" "* | *"?"*) return 1 ;;
      esac
      return 0
      ;;
    *) return 1 ;;
  esac
}

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

resolve_version_from_api() {
  local response tag token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
  local -a curl_args=(-fsSL)

  if [ -n "${token}" ]; then
    curl_args+=(-H "Authorization: Bearer ${token}" -H "Accept: application/vnd.github+json")
  fi

  response="$(curl "${curl_args[@]}" "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null)" || true
  tag="$(printf '%s' "${response}" | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4)"
  if [ -n "${tag}" ] && validate_tag "${tag}"; then
    echo "${tag}"
  fi
}

resolve_version_from_html() {
  local location tag
  location="$(curl -fsSI "https://github.com/${REPO}/releases/latest" 2>/dev/null \
    | awk -F': ' 'tolower($1)=="location" {gsub(/\r/,"",$2); print $2}' | tail -1)" || true
  tag="${location##*/}"
  if [ -n "${tag}" ] && validate_tag "${tag}"; then
    echo "${tag}"
  fi
}

resolve_version() {
  local tag

  if [ -n "${DESLICER_VERSION:-}" ]; then
    echo "${DESLICER_VERSION}"
    return
  fi

  # GitHub's "latest" release excludes prereleases by definition.
  tag="$(resolve_version_from_api)"
  if [ -z "${tag}" ]; then
    tag="$(resolve_version_from_html)"
  fi

  if [ -n "${tag}" ]; then
    echo "${tag}"
    return
  fi

  fail "could not resolve latest release tag (GitHub API rate-limited or unavailable); set ${PINNED_VERSION_ENV}=vX.Y.Z to install a specific release"
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
  local target version artifact base_url current_version=""
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

  TMP_DIR="$(umask 077; mktemp -d)"
  trap cleanup_tmp_dir EXIT

  log "downloading ${artifact}"
  curl -fsSL -o "${TMP_DIR}/${artifact}" "${base_url}/${artifact}"
  curl -fsSL -o "${TMP_DIR}/${artifact}.sha256" "${base_url}/${artifact}.sha256"
  sha256_check "${TMP_DIR}/${artifact}" "${TMP_DIR}/${artifact}.sha256"
  log "checksum verified"

  tar -C "${TMP_DIR}" -xzf "${TMP_DIR}/${artifact}"
  [ -f "${TMP_DIR}/${BINARY}" ] || fail "archive did not contain the ${BINARY} binary"

  if [ -w "${INSTALL_DIR}" ]; then
    install -m 0755 "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
  else
    log "escalating with sudo to write ${INSTALL_DIR}"
    sudo install -m 0755 "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
  fi

  log "installed $("${INSTALL_DIR}/${BINARY}" --version) to ${INSTALL_DIR}/${BINARY}"
  trap - EXIT
  cleanup_tmp_dir
  TMP_DIR=""
}

if [[ "${BASH_SOURCE[0]:-}" == "${0}" ]]; then
  main "$@"
fi
