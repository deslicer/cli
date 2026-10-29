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

test_should_tag_cargo_as_is() {
  should_tag_cargo_as_is auto 1.7.2 1.7.1 || fail "auto + cargo ahead"
  should_tag_cargo_as_is auto 1.7.2 1.7.2 && fail "equal is not ahead"
  should_tag_cargo_as_is auto 1.7.1 1.7.2 && fail "cargo behind"
  should_tag_cargo_as_is minor 1.7.2 1.7.1 && fail "explicit bump ignores cargo-ahead shortcut"
  should_tag_cargo_as_is auto 1.7.2 "" && fail "no last tag"
  pass "should_tag_cargo_as_is"
}

# Simulates the #192 / #194 / #200 pile-up: exact branch plus run_id suffixes.
test_select_open_bump_branch() {
  local json picked
  json='[
    {"number":192,"headRefName":"release/v1.7.3"},
    {"number":194,"headRefName":"release/v1.7.3-40866227225"},
    {"number":200,"headRefName":"release/v1.7.3-40870527314"},
    {"number":201,"headRefName":"release/v1.8.0"}
  ]'
  picked="$(select_open_bump_branch v1.7.3 "${json}")"
  [ "${picked}" = "release/v1.7.3-40870527314" ] || fail "expected highest 1.7.3 match, got ${picked}"

  picked="$(select_open_bump_branch v1.8.0 "${json}")"
  [ "${picked}" = "release/v1.8.0" ] || fail "exact 1.8.0 match, got ${picked}"

  picked="$(select_open_bump_branch v1.9.0 "${json}")"
  [ -z "${picked}" ] || fail "no match should be empty, got ${picked}"

  # Do not treat release/v1.7.30 as a v1.7.3 match.
  json='[{"number":1,"headRefName":"release/v1.7.30"}]'
  picked="$(select_open_bump_branch v1.7.3 "${json}")"
  [ -z "${picked}" ] || fail "v1.7.30 must not match v1.7.3, got ${picked}"

  pass "select_open_bump_branch"
}

test_bump_semver
test_decide_bump
test_version_gt
test_plan_when_cargo_already_ahead
test_should_tag_cargo_as_is
test_select_open_bump_branch
echo "cut-release tests passed"
