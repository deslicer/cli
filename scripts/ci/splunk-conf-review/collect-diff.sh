#!/usr/bin/env bash
# Collect changed Splunk conf/meta files and build a CI prompt for Code Review Agent.
#
# Env:
#   BASE_SHA, HEAD_SHA   required (PR base / head)
#   OUT_DIR              default: .deslicer-conf-review
#   MAX_PROMPT_CHARS     default: 30000 (CLI agent prompt ceiling is 32000)
#   INCLUDE_META         default: 1 (include default.meta / local.meta)
#
# Outputs under OUT_DIR:
#   files.txt            changed paths (one per line); empty → no review needed
#   prompt.md            agent prompt
#   skipped              present when there is nothing to review
set -euo pipefail

BASE_SHA="${BASE_SHA:?BASE_SHA is required}"
HEAD_SHA="${HEAD_SHA:?HEAD_SHA is required}"
OUT_DIR="${OUT_DIR:-.deslicer-conf-review}"
MAX_PROMPT_CHARS="${MAX_PROMPT_CHARS:-30000}"
INCLUDE_META="${INCLUDE_META:-1}"
PR_TITLE="${PR_TITLE:-}"
PR_BODY="${PR_BODY:-}"

mkdir -p "${OUT_DIR}"
rm -f "${OUT_DIR}/skipped" "${OUT_DIR}/prompt.md" "${OUT_DIR}/files.txt"

mapfile -t ALL_CHANGED < <(
  git diff --name-only "${BASE_SHA}"..."${HEAD_SHA}" -- \
    | grep -E '\.conf$' || true
)

if [[ "${INCLUDE_META}" == "1" ]]; then
  mapfile -t META_CHANGED < <(
    git diff --name-only "${BASE_SHA}"..."${HEAD_SHA}" -- \
      | grep -E '(^|/)(default|local)\.meta$' || true
  )
  ALL_CHANGED+=("${META_CHANGED[@]:-}")
fi

# Deduplicate while preserving order
declare -A SEEN=()
CHANGED=()
for f in "${ALL_CHANGED[@]:-}"; do
  [[ -z "${f}" ]] && continue
  if [[ -z "${SEEN[$f]:-}" ]]; then
    SEEN[$f]=1
    CHANGED+=("${f}")
  fi
done

if [[ ${#CHANGED[@]} -eq 0 ]]; then
  : > "${OUT_DIR}/files.txt"
  touch "${OUT_DIR}/skipped"
  cat > "${OUT_DIR}/prompt.md" <<'EOF'
# Splunk conf PR review

No `*.conf` (or optional `.meta`) files changed between the base and head SHAs.
Return findings JSON with an empty findings array and ok:true.
EOF
  echo "No conf/meta changes; wrote skipped marker."
  exit 0
fi

printf '%s\n' "${CHANGED[@]}" > "${OUT_DIR}/files.txt"

{
  echo "# Splunk conf PR review"
  echo
  echo "Review the following Splunk configuration changes. Follow the Code Review Agent"
  echo "instructions and the splunk-conf-pr-review skill. Emit findings JSON last."
  echo
  if [[ -n "${PR_TITLE}" ]]; then
    echo "## Pull request"
    echo
    echo "**Title:** ${PR_TITLE}"
    echo
    if [[ -n "${PR_BODY}" ]]; then
      # Cap body excerpt
      BODY_EXCERPT="$(printf '%s' "${PR_BODY}" | head -c 2000)"
      echo "${BODY_EXCERPT}"
      echo
    fi
  fi
  echo "## Changed files"
  echo
  for f in "${CHANGED[@]}"; do
    echo "- \`${f}\`"
  done
  echo
  echo "## Unified diffs"
  echo
} > "${OUT_DIR}/prompt.md"

TRUNCATED=0
for f in "${CHANGED[@]}"; do
  {
    echo "### \`${f}\`"
    echo
    echo '```diff'
    git diff --unified=3 "${BASE_SHA}"..."${HEAD_SHA}" -- "${f}" || true
    echo '```'
    echo
  } >> "${OUT_DIR}/prompt.md"

  CHAR_COUNT="$(wc -c < "${OUT_DIR}/prompt.md" | tr -d ' ')"
  if (( CHAR_COUNT > MAX_PROMPT_CHARS )); then
    TRUNCATED=1
    break
  fi
done

if [[ "${TRUNCATED}" -eq 1 ]]; then
  {
    echo
    echo "> **Note:** Diff content was truncated to stay under the CLI agent prompt"
    echo "> limit (${MAX_PROMPT_CHARS} characters). Review only what appears above."
  } >> "${OUT_DIR}/prompt.md"
  # Hard trim if still over (title/body edge case)
  CHAR_COUNT="$(wc -c < "${OUT_DIR}/prompt.md" | tr -d ' ')"
  if (( CHAR_COUNT > MAX_PROMPT_CHARS )); then
    head -c "${MAX_PROMPT_CHARS}" "${OUT_DIR}/prompt.md" > "${OUT_DIR}/prompt.md.tmp"
    mv "${OUT_DIR}/prompt.md.tmp" "${OUT_DIR}/prompt.md"
    printf '\n\n> **Note:** Prompt hard-truncated at %s characters.\n' "${MAX_PROMPT_CHARS}" >> "${OUT_DIR}/prompt.md"
  fi
fi

echo "Wrote ${#CHANGED[@]} file(s) and prompt to ${OUT_DIR}/"
