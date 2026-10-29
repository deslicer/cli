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
- **14** is emitted by `change status` when the plan lifecycle is a terminal
  failure (`failed`, `compile_failed`, `rejected`, `expired`), not as a
  `CliError` variant.
- **20** / **21** come from `change validate` (`--fail-on block|warning`).
- Not logged in (device session missing) exits **4** (auth), not **1**.

## Related

- Per-platform OIDC troubleshooting: [oidc-troubleshooting.md](oidc-troubleshooting.md)
- CI output sinks: [ci-outputs.md](ci-outputs.md)
