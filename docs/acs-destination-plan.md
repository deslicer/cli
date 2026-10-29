# ACS destination plans (`change plan --destination acs-stack:<stack>`)

Splunk Cloud changes go through `change`, like every other Splunk change.
There is no separate `acs plan` verb.

```bash
deslicer change plan --destination acs-stack:scv-prod --environment prod --json
```

## What the CLI sends

1. `POST /api/v1/plans` with `stack_kind: "cloud"`, `acs_stack_name`, and
   `source_type: "manual"`. Cloud plans are allowed in every change-management mode.
2. `POST /api/v1/acs/stacks/{stack}/project-plan` with:
   - `environment_name`: the resolved environment
   - `mapping_source: "repo_yaml"`
   - `apps[]`: `app_name`, `source_path`, `inventory_group`, and `files` (relative path → text)
   - `plan_id`: the row id from step 1. Observer attaches the projected items to it.

The plan stays Pending until a human approves it. The CLI never approves or executes it.

## How inputs are resolved

| Input | Source |
|---|---|
| Environment | `--environment`. Otherwise the active session's tenant slug, or the only `.deslicer/environments/*.yml`. The CLI prints the name, where it came from, and the file path on stderr. |
| Destination group | In that file: `inventory_group: <stack>`, then `cloud`, then the only group with apps. |
| Apps | Each `source_path` directory under the repo root. Text files are included and dotfiles are skipped. Binary files are ignored. Each app is capped at 2000 files and 16 MiB. |

## Exit codes

- `0`: the plan was created. With `--json`, stdout has one object with `plan_id`,
  `id`, `status`, `stack`, `environment_name`, `environment_file`,
  `inventory_group`, `apps`, `item_count`, and `items`.
- `2`: the environment or apps could not be resolved. Causes include no environment, a missing file, no destination with apps, ambiguous groups, a missing or empty `source_path`, or a path that escapes the repo. Nothing is sent to Observer, and an empty `apps` list is never sent.
- Other codes are the usual Observer HTTP mappings.
