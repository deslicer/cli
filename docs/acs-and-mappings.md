# ACS stacks and environment app mappings

These commands talk to Observer through the Deslicer CLI proxy. None of them
approve, execute or deploy a change plan.

## ACS stacks (Splunk Cloud)

| Command | Observer route | Notes |
|---|---|---|
| `acs stacks list` | `GET /api/v1/acs/stacks/credentials` | Stacks are the ones with a stored ACS credential. Observer has no separate stack-list route. |
| `acs credentials list` | `GET /api/v1/acs/stacks/credentials` | Metadata only. Tokens are never returned or printed. |
| `acs credentials set <stack> --token-stdin --yes` | `PUT /api/v1/acs/stacks/{stack}/credentials` | Admin role and a verified human identity. Token comes from stdin or `DESLICER_ACS_TOKEN`, never argv. `--acs-environment staging\|production` (default production). Dry-run without `--yes`. |
| `acs credentials test --name <stack>` | `POST /api/v1/acs/stacks/{stack}/observe` | Read-only. Exit 0 when ACS answers, 1 when the credential is missing or ACS rejects it. |
| `acs observe <stack> [--experience victoria\|classic]` | `POST /api/v1/acs/stacks/{stack}/observe` | Read-only inventory of ACS objects. |

```sh
printf '%s' "$ACS_JWT" | deslicer acs credentials set my-stack --token-stdin --yes
deslicer acs credentials test --name my-stack
deslicer --log-format json acs observe my-stack
```

## Environment app mappings

| Command | Observer route | Notes |
|---|---|---|
| `environments mappings list --environment <env>` | `GET /api/v1/environments/{env}/mappings` | Rows use `app_name`, `host_group_id`, `inventory_group`, `state`, `source`. |
| `environments mappings set ... --group <name\|uuid> --yes` | `PUT /api/v1/environments/{env}/mappings` | Sends `{mappings:[...], replace:false}`. Upsert only. `--group` is resolved to a host-group id via `GET /api/v1/groups`. `--host` must be a host UUID. |
| `environments mappings adopt ... --group <name\|uuid> --yes` | `PUT /api/v1/environments/{env}/mappings` | Upsert without a source path. Observer has no dedicated adopt route. |

Observer accepts direct mapping writes only when the environment mapping
source is `db`. With `repo_yaml` or `distribution`, edit the source instead;
the write returns a conflict.

```sh
deslicer environments mappings set --environment prod --app ta_nix \
  --source-path apps/ta_nix --group indexers          # dry-run
deslicer environments mappings set --environment prod --app ta_nix \
  --source-path apps/ta_nix --group indexers --yes
```
