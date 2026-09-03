#!/usr/bin/env bash
#
# Decide and apply a deslicer-cli semver bump.
#
#   scripts/cut-release.sh plan [--bump auto|patch|minor|major]
#   scripts/cut-release.sh apply 1.4.0
#
# plan writes GitHub Actions outputs when GITHUB_OUTPUT is set.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

emit() {
  local key="$1" value="$2"
  printf '%s=%s\n' "${key}" "${value}"
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    printf '%s=%s\n' "${key}" "${value}" >> "${GITHUB_OUTPUT}"
  fi
}

cargo_version() {
  awk '/^version = "/ { gsub(/"/, "", $3); print $3; exit }' "${ROOT}/Cargo.toml"
}

latest_stable_tag() {
  git -C "${ROOT}" tag -l 'v*.*.*' --sort=-v:refname \
    | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' \
    | head -1
}

tag_version() {
  local tag="${1:-}"
  printf '%s' "${tag#v}"
}

version_gt() {
  local left="$1" right="$2"
  [ "${left}" != "${right}" ] && [ "$(printf '%s\n%s\n' "${right}" "${left}" | sort -V | tail -1)" = "${left}" ]
}

bump_semver() {
  local version="$1" part="$2"
  local major minor patch
  IFS=. read -r major minor patch <<EOF
${version}
EOF
  patch="${patch%%-*}"
  case "${part}" in
    major) echo "$((major + 1)).0.0" ;;
    minor) echo "${major}.$((minor + 1)).0" ;;
    patch) echo "${major}.${minor}.$((patch + 1))" ;;
    *) echo "unknown bump: ${part}" >&2; return 1 ;;
  esac
}

commit_subjects_since() {
  local since="$1"
  if [ -z "${since}" ]; then
    git -C "${ROOT}" log --pretty=%s
    return
  fi
  git -C "${ROOT}" log "${since}..HEAD" --pretty=%s
}

decide_bump() {
  local subjects="$1"
  if printf '%s\n' "${subjects}" | grep -qE '(^| )BREAKING CHANGE|^[a-z]+(\([^)]+\))?!:'; then
    echo major
    return
  fi
  if printf '%s\n' "${subjects}" | grep -qE '^feat(\(|:|!)'; then
    echo minor
    return
  fi
  if printf '%s\n' "${subjects}" | grep -qE '^(fix|perf)(\(|:|!)'; then
    echo patch
    return
  fi
  echo skip
}

apply_version() {
  local version="$1" tmp
  tmp="$(mktemp)"
  awk -v ver="${version}" '
    BEGIN { done = 0 }
    /^version = "/ && !done {
      sub(/version = "[^"]+"/, "version = \"" ver "\"")
      done = 1
    }
    { print }
  ' "${ROOT}/Cargo.toml" > "${tmp}"
  mv "${tmp}" "${ROOT}/Cargo.toml"

  tmp="$(mktemp)"
  awk -v ver="${version}" '
    $0 == "name = \"deslicer-cli\"" { hit = 1 }
    hit && /^version = "/ {
      sub(/version = "[^"]+"/, "version = \"" ver "\"")
      hit = 0
    }
    { print }
  ' "${ROOT}/Cargo.lock" > "${tmp}"
  mv "${tmp}" "${ROOT}/Cargo.lock"
}

plan_release() {
  local bump="${1:-auto}"
  local cargo last_tag last decided next reason

  cargo="$(cargo_version)"
  last_tag="$(latest_stable_tag || true)"
  last="$(tag_version "${last_tag}")"

  if [ "${bump}" = auto ]; then
    decided="$(decide_bump "$(commit_subjects_since "${last_tag}")")"
  else
    decided="${bump}"
  fi

  if [ "${decided}" = skip ]; then
    if [ -n "${last}" ] && version_gt "${cargo}" "${last}"; then
      emit should_release true
      emit needs_bump false
      emit version "${cargo}"
      emit tag "v${cargo}"
      emit reason "Cargo.toml ${cargo} is ahead of ${last_tag}"
      return
    fi
    emit should_release false
    emit needs_bump false
    emit version "${cargo}"
    emit tag "v${cargo}"
    emit reason "no releasable commits since ${last_tag:-the start of history}"
    return
  fi

  if [ -z "${last}" ]; then
    next="${cargo}"
  else
    next="$(bump_semver "${last}" "${decided}")"
  fi

  if version_gt "${cargo}" "${next}" || [ "${cargo}" = "${next}" ]; then
    next="${cargo}"
  fi

  reason="${decided} bump from ${last_tag:-none} -> v${next}"
  if [ "${cargo}" = "${next}" ]; then
    emit should_release true
    emit needs_bump false
    emit version "${next}"
    emit tag "v${next}"
    emit reason "${reason} (Cargo.toml already at ${cargo})"
    return
  fi

  emit should_release true
  emit needs_bump true
  emit version "${next}"
  emit tag "v${next}"
  emit reason "${reason}"
}

usage() {
  cat <<'EOF'
Usage:
  scripts/cut-release.sh plan [--bump auto|patch|minor|major]
  scripts/cut-release.sh apply <version>
EOF
}

main() {
  local cmd="${1:-}" bump="auto"
  shift || true
  case "${cmd}" in
    plan)
      while [ $# -gt 0 ]; do
        case "$1" in
          --bump) bump="$2"; shift 2 ;;
          *) usage >&2; return 1 ;;
        esac
      done
      case "${bump}" in
        auto|patch|minor|major) ;;
        *) echo "invalid --bump ${bump}" >&2; return 1 ;;
      esac
      plan_release "${bump}"
      ;;
    apply)
      [ $# -eq 1 ] || { usage >&2; return 1; }
      apply_version "$1"
      ;;
    *)
      usage >&2
      return 1
      ;;
  esac
}

if [[ "${BASH_SOURCE[0]:-}" == "${0}" ]]; then
  main "$@"
fi
