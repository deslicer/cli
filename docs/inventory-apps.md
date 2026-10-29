# Fleet apps (`inventory apps`, `inventory apps adopt`)

## List

```
deslicer inventory apps [--host <hostname|uuid>] [--group <inventory-group>] [--log-format json]
```

Reads Observer `GET /api/v1/insights/apps` (all pages, 2000 apps each) and
prints one row per (app, installed host): `NAME VERSION HOST LAST_SEEN`.
JSON rows: `name`, `version`, `host`, `host_id`, `last_seen`.

- `--host` keeps apps installed on that host (hostname, case-insensitive, or
  host UUID). Exit 2 when no enrolled host matches. Same filter as the portal
  Apps inventory host filter (DAI #2858).
- `--group` keeps hosts in that Ansible inventory group
  (`GET /api/v1/inventory`). Exit 2 when the group does not exist.

## Adopt as Git-managed

```
deslicer inventory apps adopt <app> --host <hostname|uuid> [--group <name|uuid>] \
  [--scope apps|deployment-apps] [--source-path apps/<app>] [--yes]
```

Without `--yes` it prints a dry run and calls nothing. With `--yes` it checks
the app is installed on the host (exit 2 otherwise), then sends
`POST /api/v1/app-provenance/adopt`:

```json
{"scope":"apps","app_name":"<app>","host_id":"<uuid>","management_mode":"managed",
 "content_source":"git_tree","source_path":"apps/<app>"}
```

Observer requires admin scope and a verified human (device login,
`deslicer auth login`); API keys alone get 403. This matches the portal
"Adopt / Git Managed" action (DAI #2994).
