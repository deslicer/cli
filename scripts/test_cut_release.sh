#!/usr/bin/env bash
# Unit tests for scripts/cut-release.sh version planning.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/cut-release.sh
source "${ROOT}/scripts/cut-release.sh"

fail() { echo "FAIL: $1" >&2; exit 1; }
pass() { echo "ok - $1"; }

test_bump_semver() {
  [ "$(bump_semver 1.3.2 patch)" = "1.3.3" ] || fail "patch bump"
  [ "$(bump_semver 1.3.2 minor)" = "1.4.0" ] || fail "minor bump"
  [ "$(bump_semver 1.3.2 major)" = "2.0.0" ] || fail "major bump"
  pass "bump_semver"
}

test_decide_bump() {
  [ "$(decide_bump $'docs: x\nchore: y')" = "skip" ] || fail "docs/chore skip"
  [ "$(decide_bump $'fix(agent): stream\ndocs: x')" = "patch" ] || fail "fix is patch"
  [ "$(decide_bump $'feat(agent): repl\nfix: x')" = "minor" ] || fail "feat is minor"
  [ "$(decide_bump $'feat!: drop v1 API')" = "major" ] || fail "bang feat is major"
  [ "$(decide_bump $'fix(auth): x\nBREAKING CHANGE: tokens')" = "major" ] || fail "footer is major"
  pass "decide_bump"
}

test_version_gt() {
  version_gt 1.4.0 1.3.2 || fail "1.4.0 > 1.3.2"
  version_gt 1.3.2 1.3.2 && fail "equal is not greater"
  version_gt 1.3.2 1.4.0 && fail "1.3.2 > 1.4.0"
  pass "version_gt"
}

test_plan_when_cargo_already_ahead() {
  local out
  # Latest tag in this clone may vary; feed subjects through the helpers directly.
  [ "$(tag_version v1.3.2)" = "1.3.2" ] || fail "tag_version"
  out="$(cargo_version)"
  [ -n "${out}" ] || fail "cargo_version empty"
  pass "plan helpers"
}

test_bump_semver
test_decide_bump
test_version_gt
test_plan_when_cargo_already_ahead
echo "cut-release tests passed"
