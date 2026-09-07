#!/usr/bin/env bash
# Render a GitHub PR review body from findings.json
# Usage: render-review.md.sh findings.json > review.md
set -euo pipefail

FINDINGS_FILE="${1:?usage: render-review.md.sh <findings.json>}"

python3 - "${FINDINGS_FILE}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
doc = json.loads(path.read_text(encoding="utf-8"))
summary = doc.get("summary", "").strip() or "(no summary)"
ok = doc.get("ok", False)
findings = doc.get("findings") or []

print("## Deslicer Code Review Agent — Splunk conf")
print()
print(summary)
print()
print(f"**Result:** `{'pass' if ok else 'fail'}` — "
      f"{sum(1 for f in findings if f.get('severity')=='error')} error(s), "
      f"{sum(1 for f in findings if f.get('severity')=='warning')} warning(s), "
      f"{sum(1 for f in findings if f.get('severity')=='info')} info.")
print()
if not findings:
    print("_No findings._")
    raise SystemExit(0)

print("| Severity | File | Stanza | Key | Message |")
print("| --- | --- | --- | --- | --- |")
for f in findings:
    sev = str(f.get("severity", ""))
    file = str(f.get("file", "")).replace("|", "\\|")
    stanza = str(f.get("stanza") or "").replace("|", "\\|")
    key = str(f.get("key") or "").replace("|", "\\|")
    msg = str(f.get("message") or "").replace("|", "\\|").replace("\n", " ")
    print(f"| `{sev}` | `{file}` | {stanza or '—'} | {key or '—'} | {msg} |")

suggestions = [f for f in findings if f.get("suggestion")]
if suggestions:
    print()
    print("### Suggestions")
    print()
    for f in suggestions:
        loc = f.get("file", "")
        if f.get("stanza"):
            loc += f" / [{f['stanza']}]"
        if f.get("key"):
            loc += f" `{f['key']}`"
        print(f"- **{loc}:** {f['suggestion']}")
PY
