#!/usr/bin/env python3
"""Extract and validate Code Review Agent findings JSON from agent stdout.

Exit codes:
  0 — valid JSON and no severity==error findings
  1 — missing/invalid JSON, schema failure, or one or more error findings
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


FENCE_RE = re.compile(r"```(?:json)?\s*(\{.*?\})\s*```", re.DOTALL | re.IGNORECASE)
SEVERITIES = {"error", "warning", "info"}


def extract_json_blob(text: str) -> dict[str, Any]:
    text = text.strip()
    if not text:
        raise ValueError("empty agent stdout")

    match = FENCE_RE.search(text)
    if match:
        return json.loads(match.group(1))

    # Fallback: last top-level object
    start = text.rfind("{")
    if start < 0:
        raise ValueError("no JSON object found in agent stdout")
    decoder = json.JSONDecoder()
    obj, _ = decoder.raw_decode(text[start:])
    if not isinstance(obj, dict):
        raise ValueError("trailing JSON is not an object")
    return obj


def validate(doc: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if "summary" not in doc or not isinstance(doc["summary"], str) or not doc["summary"].strip():
        errors.append("missing non-empty string field 'summary'")
    if "ok" not in doc or not isinstance(doc["ok"], bool):
        errors.append("missing boolean field 'ok'")
    if "findings" not in doc or not isinstance(doc["findings"], list):
        errors.append("missing list field 'findings'")
        return errors

    has_error = False
    for i, finding in enumerate(doc["findings"]):
        if not isinstance(finding, dict):
            errors.append(f"findings[{i}] is not an object")
            continue
        sev = finding.get("severity")
        if sev not in SEVERITIES:
            errors.append(f"findings[{i}].severity must be one of {sorted(SEVERITIES)}")
        if sev == "error":
            has_error = True
        if not isinstance(finding.get("file"), str) or not finding["file"].strip():
            errors.append(f"findings[{i}].file must be a non-empty string")
        if not isinstance(finding.get("message"), str) or not finding["message"].strip():
            errors.append(f"findings[{i}].message must be a non-empty string")

    if isinstance(doc.get("ok"), bool) and doc["ok"] is True and has_error:
        errors.append("ok is true but findings contain severity=error")
    if isinstance(doc.get("ok"), bool) and doc["ok"] is False and not has_error:
        # Soft inconsistency: treat as schema warning → fail closed
        errors.append("ok is false but no severity=error findings")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "stdout_file",
        nargs="?",
        help="Path to agent stdout (default: stdin)",
    )
    parser.add_argument(
        "-o",
        "--output",
        default="findings.json",
        help="Where to write normalized findings JSON",
    )
    args = parser.parse_args()

    raw = (
        Path(args.stdout_file).read_text(encoding="utf-8")
        if args.stdout_file
        else sys.stdin.read()
    )

    try:
        doc = extract_json_blob(raw)
    except (ValueError, json.JSONDecodeError) as exc:
        print(f"parse-findings: fail-closed: {exc}", file=sys.stderr)
        return 1

    schema_errors = validate(doc)
    if schema_errors:
        for err in schema_errors:
            print(f"parse-findings: {err}", file=sys.stderr)
        return 1

    out_path = Path(args.output)
    out_path.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")

    error_count = sum(1 for f in doc["findings"] if f.get("severity") == "error")
    warning_count = sum(1 for f in doc["findings"] if f.get("severity") == "warning")
    print(
        f"parse-findings: wrote {out_path} "
        f"(errors={error_count} warnings={warning_count} ok={doc['ok']})"
    )
    return 1 if error_count else 0


if __name__ == "__main__":
    sys.exit(main())
