# Registry plans (`change plan --from-registry`)

```
deslicer change plan --from-registry <app>@<ver> [--target-group <name|uuid>] [--environment <env>] [--no-wait]
```

Plans a registry (Splunkbase) app release for an inventory group. Works under
every change-management mode, including `git_backed` (DAP #1395); git-sourced
changes still go through PRs.

1. `<app>@<ver>` becomes Observer's registry ref `splunkbase:<app>/<ver>`.
   App and version may use letters, digits, `.`, `_`, `-`, `+`.
2. Target group: `--target-group`, else the single destination in the
   environment YAML. Exit 2 when neither resolves; Observer requires a group.
3. `POST /api/v1/plans` with `source_type: "registry"`, `source_reference`,
   `target_group_id`. Observer resolves the artifact from the group's durable
   pin (the group must already pin this release; otherwise Observer returns 4xx).
4. `POST /api/v1/runners/compile/{plan_row_id}` starts the compile-runner;
   the CLI waits for Pending unless `--no-wait`.

The plan is never approved or executed by this command. Approve it in the
portal or with `change approve` (human gate), then deploy.
