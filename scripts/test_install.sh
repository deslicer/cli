#!/usr/bin/env bash
#
# Shell tests for scripts/install.sh: EXIT trap cleanup and version resolution.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/install.sh
source "${ROOT}/scripts/install.sh"

INSTALL_DIR_ENV="DESLICER_INSTALL_DIR" # pragma: allowlist secret

TEST_ROOT="$(mktemp -d)"
MOCK_BIN="${TEST_ROOT}/mock-bin"
INSTALL_DIR="${TEST_ROOT}/install"
ARCHIVE_DIR="${TEST_ROOT}/archive"
ARTIFACT="${BINARY}-x86_64-unknown-linux-musl.tar.gz"

cleanup() {
  rm -rf "${TEST_ROOT}"
}
trap cleanup EXIT

pass() { printf 'PASS: %s\n' "$1"; }
fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }

setup_archive() {
  mkdir -p "${ARCHIVE_DIR}/staging" "${INSTALL_DIR}"
  cat > "${ARCHIVE_DIR}/staging/${BINARY}" <<EOF
#!/bin/sh
if [ "\$1" = "--version" ]; then
  printf '%s v1.3.1\n' "${BINARY}"
fi
EOF
  chmod +x "${ARCHIVE_DIR}/staging/${BINARY}"
  tar -C "${ARCHIVE_DIR}/staging" -czf "${ARCHIVE_DIR}/${ARTIFACT}" "${BINARY}"
  sha256sum "${ARCHIVE_DIR}/${ARTIFACT}" > "${ARCHIVE_DIR}/${ARTIFACT}.sha256"
}

write_mock_curl() {
  local api_mode="$1"
  mkdir -p "${MOCK_BIN}"
  cat > "${MOCK_BIN}/curl" <<MOCK
#!/usr/bin/env bash
set -euo pipefail

output=""
url=""
header_only=0
args=("\$@")
i=0
while [ "\$i" -lt "\${#args[@]}" ]; do
  arg="\${args[\$i]}"
  case "\$arg" in
    -o)
      i=\$((i + 1))
      output="\${args[\$i]}"
      ;;
    -[^o-]*[Ii]*)
      header_only=1
      ;;
    http://*|https://*)
      url="\$arg"
      ;;
  esac
  i=\$((i + 1))
done

if [[ "\$url" == *api.github.com*releases/latest* ]]; then
  if [ "${api_mode}" = "403" ]; then
    printf 'rate limit exceeded\n' >&2
    exit 22
  fi
  printf '{"tag_name":"v9.9.9"}\n'
  exit 0
fi

if [[ "\$url" == *github.com*releases/latest* ]] && [ "\$header_only" = 1 ]; then
  printf 'HTTP/2 302\r\nlocation: https://github.com/${REPO}/releases/tag/v8.8.8\r\n\r\n'
  exit 0
fi

if [[ "\$url" == *releases/download/v1.3.1/${ARTIFACT}.sha256 ]]; then
  cp "${ARCHIVE_DIR}/${ARTIFACT}.sha256" "\$output"
  exit 0
fi

if [[ "\$url" == *releases/download/v1.3.1/${ARTIFACT} ]]; then
  cp "${ARCHIVE_DIR}/${ARTIFACT}" "\$output"
  exit 0
fi

printf 'unexpected curl request: %s\n' "\$url" >&2
exit 1
MOCK
  chmod +x "${MOCK_BIN}/curl"
}

run_installer() {
  local api_mode="$1"
  write_mock_curl "${api_mode}"
  export "${PINNED_VERSION_ENV}=v1.3.1"
  export "${INSTALL_DIR_ENV}=${INSTALL_DIR}"
  PATH="${MOCK_BIN}:${PATH}" bash "${ROOT}/scripts/install.sh"
}

test_successful_install_exits_zero() {
  setup_archive
  run_installer 403
  [ -x "${INSTALL_DIR}/${BINARY}" ] || fail "binary was not installed"
  pass "successful install exits 0 with pinned version"
}

test_api_403_does_not_block_pinned_install() {
  setup_archive
  run_installer 403
  pass "403 on releases/latest does not block pinned-version install"
}

test_resolve_version_html_fallback() {
  unset "${PINNED_VERSION_ENV}"
  write_mock_curl 403
  local resolved
  resolved="$(PATH="${MOCK_BIN}:${PATH}" resolve_version)"
  [ "${resolved}" = "v8.8.8" ] || fail "expected HTML fallback tag v8.8.8, got ${resolved}"
  pass "resolve_version falls back to HTML redirect when API is rate-limited"
}

test_resolve_version_pinned_skips_network() {
  export "${PINNED_VERSION_ENV}=v1.3.1"
  local resolved
  resolved="$(resolve_version)"
  [ "${resolved}" = "v1.3.1" ] || fail "expected pinned version v1.3.1, got ${resolved}"
  pass "resolve_version returns pinned version without network"
}

test_successful_install_exits_zero
test_api_403_does_not_block_pinned_install
test_resolve_version_html_fallback
test_resolve_version_pinned_skips_network
