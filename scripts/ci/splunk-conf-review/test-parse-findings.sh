#!/usr/bin/env bash
# Smoke-test parse-findings.py against fixtures.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
PARSER="${ROOT}/parse-findings.py"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

python3 "${PARSER}" "${ROOT}/fixtures/agent-stdout-ok.txt" -o "${TMP}/ok.json"
test "$(python3 -c "import json;print(json.load(open('${TMP}/ok.json'))['ok'])")" = "True"

if python3 "${PARSER}" "${ROOT}/fixtures/agent-stdout-error.txt" -o "${TMP}/err.json"; then
  echo "expected non-zero exit for error findings" >&2
  exit 1
fi

if python3 "${PARSER}" "${ROOT}/fixtures/agent-stdout-missing-json.txt" -o "${TMP}/missing.json"; then
  echo "expected non-zero exit for missing JSON" >&2
  exit 1
fi

echo "parse-findings fixtures OK"
