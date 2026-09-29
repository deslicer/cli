# Maintainer source map

Recheck these files when CLI contracts change. Paths are relative to the
`deslicer/cli` repository; this file is not required at skill runtime.

| Contract | Source |
| --- | --- |
| Init flags, order, partial writes | `src/commands/init/mod.rs` |
| Auto provider / origin detection | `src/commands/init/provider.rs` |
| Binding auth and platform limits | `src/commands/init/bind.rs` |
| Pinned templates and offline cache | `src/commands/init/templates.rs` |
| Overwrite and README behavior | `src/commands/init/write.rs` |
| Tenant YAML generation | `src/commands/init/environment.rs`, `src/environment_yaml.rs` |
| Print-only GitHub configuration | `src/commands/init/github_env_recipe.rs` |
| Remote creation, refresh, status | `src/commands/repo/{bootstrap,refresh,status,session}.rs` |
| Inventory preview and validation | `src/commands/inventory/{sync,validate}.rs` |
| Operator setup and YAML contract | `docs/repo-init-and-enroll.md`, `docs/environments.md` |
| Auth precedence, login and identity semantics | `src/auth_resolution.rs`, `src/commands/auth/{login,status,whoami,logout}.rs` |
| Portal selection, token-only config, device storage | `src/cli.rs`, `src/observer_token.rs`, `src/device_flow.rs`, `src/token_store.rs` |

GitHub command syntax was checked against installed `gh` help for `auth login`,
`auth status`, `auth setup-git`, and `repo create`. Recheck installed help for
the remaining API/secret/variable/workflow commands before operating them.

When prose and implementation disagree, check the installed version and source;
do not turn a documentation example into authorization to overwrite or deploy.
