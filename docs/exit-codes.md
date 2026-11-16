# Exit codes

Canonical process exit-code contract for the Deslicer CLI. Agents should branch
on these codes; JSON error envelopes also carry `error.exit_code` (see the
agent-native CLI spec).

Do **not** renumber existing codes. `deslicer schema` (when available) republishes
this table.

| code | meaning | retryable |
|---|---|---|
| 0 | ok (also a no-op or idempotent replay) | – |
| 1 | unexpected or internal error (should be rare); also fallback when a multi-part command fails without a more specific part code | no |
| 2 | usage / invalid input / input required / conflicting output flags | no |
| 4 | auth (not logged in, token rejected, device-login timeout / pending) | no\* |
| 5 | repository not allowlisted | no |
| 6 | environment not bound | no |
| 7 | ambiguous binding / git-backed change management | no |
| 8 | unsupported CI platform | no |
| 9 | rate limited (honour Retry-After) | yes |
| 10 | backend or transport transient | yes |
| 11 | plan not found | no |
| 12 | human approval required | no |
| 13 | agent run failed | no |
| 14 | plan lifecycle failed (`change status` terminal failure) | no |
| 15 | endpoint not available on this Observer | no |
| 16 | vcs credential unavailable (reserved; not emitted yet) | no |
| 17 | vcs host rejected (reserved; not emitted yet) | no |
| 18 | review provider error (reserved; not emitted yet) | no |
| 19 | validation report missing (`change validate --report-only` on a plan that exists but has no report) | no |
| 20 | validation blocked | no |
| 21 | validation warning (treated as failure unless policy says otherwise) | no |
| 22 | validation model unavailable | yes |
| 23 | validation timeout | yes |
| 24 | validation unavailable | yes |
| 130 | cancelled (SIGINT) | – |

\* `auth.pending` after `--no-wait` is `retryable: true` in the JSON error envelope
even though exit **4** is the auth class; agents may poll.

## Mapping notes

- Most failures come from `CliError` variants in `src/errors.rs`. An exhaustiveness
  test asserts every variant maps to a row above.
- **2** covers clap usage errors, including bare groups with no subcommand
  (`deslicer change`, `cloud stack`, …). Clap may still print help; the process <!-- pragma: allowlist secret -->
  exits 2. Explicit `--help` / `--version` stay exit 0.
- **14** is emitted by `change status` when the plan lifecycle is a terminal
  failure (`failed`, `compile_failed`, `rejected`, `expired`), not as a
  `CliError` variant.
- **10** is the transport/backend class. The JSON envelope sets `retryable`
  to **false** for DNS NXDOMAIN, connection refused, TLS failures, and invalid
  URLs (`error.details.cause` is `dns`, `refused`, `tls`, or `invalid_url`).
  Timeouts and HTTP 5xx stay retryable. `hint` names the cause and
  `next_command` is `deslicer auth status`. <!-- pragma: allowlist secret -->
- **2** also covers a non-UUID `--plan-id` (rejected before the request) and a
  generic Observer HTTP 400. Exit **8** is only the unsupported CI platform /
  OIDC path. `change plan` without `GITHUB_REPOSITORY` and `GITHUB_SHA` (or the
  GitLab equivalents) is **2** `usage.missing_argument`, not **1**.
- **16** / **17** / **18** are reserved for the multi-VCS proposal
  (`vcs.credential_unavailable`, `vcs.host_rejected`, `review.provider_error`)
  and are not emitted yet.
- **19** is `validation.report_missing` from `change validate --report-only`
  when the plan exists but has no report. An unknown plan on that path is
  still **11** `plan.not_found`.
- **20** / **21** come from `change validate` (`--fail-on block|warning`).
- Not logged in (device session missing) exits **4** (auth), not **1**.
- A device session whose API origin does not match an explicit
  `DESLICER_API_URL` / `--deslicer-api-url` or `OBSERVER_API_URL` / <!-- pragma: allowlist secret -->
  `--observer-api-url` exits **4** `auth.session_origin_mismatch` before any
  authenticated request. `DESLICER_IGNORE_DEVICE_SESSION=1` skips the stored <!-- pragma: allowlist secret -->
  session (including the kernel keyring) without deleting it. Empty `HOME`
  does not isolate the kernel keyring.

## Related

- Per-platform OIDC troubleshooting: [oidc-troubleshooting.md](oidc-troubleshooting.md)
- CI output sinks: [ci-outputs.md](ci-outputs.md)
