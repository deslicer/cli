#!/usr/bin/env bash
# Restore a deslicer CLI device session for CI (file token store).
#
# Env:
#   DESLICER_DEVICE_SESSION   required — TOML credentials.toml contents, OR
#                             JSON object with StoredSession fields
#                             (cli_session_token, expires_at, tenant_id,
#                             display_name, observer_api_url, optional
#                             tenant_slug / deslicer_api_url).
#   DESLICER_DEVICE_SESSION_JSON  alias for the same value
#   DESLICER_CONFIG_DIR       default: $RUNNER_TEMP/deslicer-config
#
# Side effects: writes credentials.toml (0600), exports DESLICER_CONFIG_DIR
# and DESLICER_TOKEN_STORE=file (also appends to GITHUB_ENV when set).
set -euo pipefail

RAW="${DESLICER_DEVICE_SESSION:-${DESLICER_DEVICE_SESSION_JSON:-}}"
if [[ -z "${RAW}" ]]; then
  echo "DESLICER_DEVICE_SESSION (or DESLICER_DEVICE_SESSION_JSON) is required" >&2
  exit 1
fi

CONFIG_DIR="${DESLICER_CONFIG_DIR:-${RUNNER_TEMP:-/tmp}/deslicer-config}"
mkdir -p "${CONFIG_DIR}"
chmod 700 "${CONFIG_DIR}"
CRED="${CONFIG_DIR}/credentials.toml"

TRIMMED="$(printf '%s' "${RAW}" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
FIRST_CHAR="$(printf '%s' "${TRIMMED}" | head -c 1)"

if [[ "${FIRST_CHAR}" == "{" ]]; then
  export DESLICER_DEVICE_SESSION_PAYLOAD="${TRIMMED}"
  python3 - "${CRED}" <<'PY'
import json
import os
import sys
from pathlib import Path

cred_path = Path(sys.argv[1])
data = json.loads(os.environ["DESLICER_DEVICE_SESSION_PAYLOAD"])
required = [
    "cli_session_token",
    "expires_at",
    "tenant_id",
    "display_name",
    "observer_api_url",
]
missing = [k for k in required if not str(data.get(k, "")).strip()]
if missing:
    raise SystemExit(f"session JSON missing fields: {', '.join(missing)}")

def toml_str(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'

lines = [
    f"cli_session_token = {toml_str(str(data['cli_session_token']))}",
    f"expires_at = {toml_str(str(data['expires_at']))}",
    f"tenant_id = {toml_str(str(data['tenant_id']))}",
    f"display_name = {toml_str(str(data['display_name']))}",
    f"observer_api_url = {toml_str(str(data['observer_api_url']))}",
]
if data.get("tenant_slug"):
    lines.append(f"tenant_slug = {toml_str(str(data['tenant_slug']))}")
if data.get("deslicer_api_url"):
    lines.append(f"deslicer_api_url = {toml_str(str(data['deslicer_api_url']))}")
cred_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
cred_path.chmod(0o600)
PY
  unset DESLICER_DEVICE_SESSION_PAYLOAD
else
  umask 077
  printf '%s\n' "${TRIMMED}" > "${CRED}"
  chmod 600 "${CRED}"
fi

export DESLICER_CONFIG_DIR="${CONFIG_DIR}"
export DESLICER_TOKEN_STORE=file

if [[ -n "${GITHUB_ENV:-}" ]]; then
  {
    echo "DESLICER_CONFIG_DIR=${CONFIG_DIR}"
    echo "DESLICER_TOKEN_STORE=file"
  } >> "${GITHUB_ENV}"
fi

echo "Restored device session to ${CRED} (DESLICER_TOKEN_STORE=file)"
