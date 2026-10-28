# Plan-runner E2E (enroll → worker → draft → approve)

Operator runbook for stabilizing the CLI plan-runner path used in workshop
dry-runs (Homestasis / Amsterdam). Commands already exist; this page stitches
them into one checklist with expected failure modes.

Related: [repo-init-and-enroll.md](repo-init-and-enroll.md),
[quickstart.md](quickstart.md), [bundle-flow.md](bundle-flow.md),
[oidc-troubleshooting.md](oidc-troubleshooting.md).
Workshop lab pin (when selected): `.agents/skills/deslicer-cli-propose-inspect/WORKSHOP.md`.
DAP companion: [deslicer-automation-platform#1140](https://github.com/deslicer/deslicer-automation-platform/issues/1140).

## Prerequisites

- CLI installed (`deslicer --version`)
- Device session for enroll/worker: `deslicer auth login`
- Known demo/lab tenant (Observer URL + portal). Example workshop profile
  values live in `WORKSHOP.md` and must be confirmed live before a dry-run
- Target inventory group that already exists (`deslicer groups list`)
- Tenant **worker plane** enabled in the portal (bootstrap enrollment fails otherwise)

## Happy path (script)

Replace placeholders. Prefer Path B (`--source-dir`) for laptop smoke when the
tenant cannot clone the repo; use Path A/A2 when CI or GitHub App is ready.

```bash
# 1) Auth
deslicer auth login
deslicer auth whoami
deslicer auth status

# 2) Enroll a bootstrap token (prints once; write to a 0600 file in CI)
deslicer enroll create --purpose bootstrap --name "workshop-smoke" \
  --write-file /tmp/deslicer-enroll.token
deslicer enroll list

# 3) Worker install recipe (does not SSH; run the printed steps on the host)
deslicer worker instructions --format shell --token-file /tmp/deslicer-enroll.token

# 4) Portal: approve the host under Pending Approvals, then confirm inventory
deslicer groups list
deslicer inventory list   # or inventory sync/validate for A2 tenants

# 5) Draft a plan (Path B laptop smoke)
deslicer change plan \
  --environment "$DESLICER_ENVIRONMENT" \
  --target-group "$TARGET_GROUP" \
  --source-dir . \
  --name "plan-runner e2e smoke"
# Capture PLAN_ID from human/JSON output

# 6) Inspect
deslicer change show --plan-id "$PLAN_ID"
deslicer change status --plan-id "$PLAN_ID" --timeout-secs 60
deslicer change validate --plan-id "$PLAN_ID" --report-only

# 7) Approve (human identity required — portal and/or CLI)
# Portal: Approve change request
deslicer change approve --plan-id "$PLAN_ID"

# 8) Optional execute (needs an online worker / lease)
deslicer change deploy --plan-id "$PLAN_ID"
deslicer change status --plan-id "$PLAN_ID" --timeout-secs 120
deslicer change verify --plan-id "$PLAN_ID"
```

Cleanup when finished:

```bash
deslicer enroll revoke --jti "$ENROLL_JTI"
rm -f /tmp/deslicer-enroll.token
```

## Smoke checklist (Homestasis / Amsterdam dry-run)

Use as a pass/fail sheet. Abort on the first hard failure.

| # | Check | Pass when | Abort if |
|---|-------|-----------|----------|
| 1 | Auth | `auth whoami` shows identity; `auth status` ok | Exit 1 “not logged in” / exit 4 auth rejected |
| 2 | Enroll create | Token printed once or written via `--write-file` | Worker plane disabled; device session missing |
| 3 | Worker instructions | Recipe prints without error | Recipe fetch fails / wrong product channel |
| 4 | Host approved | Host visible in `groups list` / inventory | Host stuck in Pending Approvals |
| 5 | Draft plan | Plan id returned; status progresses past compile | Auth errors; empty/`no_changes` with unexpected intent |
| 6 | Show / status / validate | Commands return; validate report readable | Hang; timeout without message; validation 22–24 |
| 7 | Approve | Plan moves to approved / pending deploy | Exit **12** MFA / human approval required |
| 8 | Deploy (optional) | Progress advances or clear “worker offline” signal | Silent hang; jobs never start |

Record Observer/worker blockers discovered live against
[DAP #1140](https://github.com/deslicer/deslicer-automation-platform/issues/1140)
or a new DAP issue, and link them from the CLI issue
[#121](https://github.com/deslicer/cli/issues/121).

## Failure modes

| Symptom | Likely cause | Exit / signal | What to do |
|---------|--------------|---------------|------------|
| `not logged in` on enroll/worker | No device session | 1 | `deslicer auth login` (tools token alone is not enough for enroll) |
| `API token rejected` / `OIDC token rejected` | Bad credentials | **4** | Fix `DESLICER_API_TOKEN` or OIDC; see [oidc-troubleshooting.md](oidc-troubleshooting.md) |
| Worker plane not enabled | Tenant setting | 1 (Other) | Enable worker plane in the portal, then recreate bootstrap token |
| Approve needs human identity | MFA / missing Environment reviewer | **12** | Approve in portal or gate CI with a reviewing Environment |
| Plan status `no_changes` | Compile found nothing to apply | **0** | Confirm `--source-dir` / group intent; not a deploy failure |
| `change status` waits then times out | Rollout not moving | timeout message | Check worker online / lease; portal execution jobs |
| Deploy / verify never completes | Worker offline or unleased | lifecycle / timeout | Host enrollment + worker service; portal Pending Approvals |
| Ambiguous / git-backed binding | Wrong plan source for tenant | **7** | See git-backed guidance from `change plan`; Path A2 vs App |

## Demo tenant pin

Do not invent secrets. When a workshop profile is explicitly selected, use the
Observer URL, environment, and target group from
`.agents/skills/deslicer-cli-propose-inspect/WORKSHOP.md`, then confirm with
`deslicer groups list` before drafting a plan.

Live verification against a lab tenant remains a follow-up (device session +
worker host). This runbook is the offline contract for operators and agents.
