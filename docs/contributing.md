# Contributing

Thank you for contributing to [deslicer/cli](https://github.com/deslicer/cli).

## Prerequisites

- **Rust 1.88.0** — `rustup` honors `rust-toolchain.toml` automatically when you run any `cargo` command in the repo.
- **rustfmt** and **clippy** components (declared in `rust-toolchain.toml`).

```bash
git clone https://github.com/deslicer/cli.git
cd cli
cargo build
```

No manual `rustup install` step is required if you have rustup installed.

## Development workflow

```bash
# Format
cargo fmt --all

# Lint (must pass in CI)
cargo clippy --all-targets -- -D warnings

# Test
cargo test --all-features

# Run locally
cargo run -- auth status --ci-platform local
```

To exercise the full plan flow against a local or remote Observer API, see [local-testing.md](local-testing.md).

## Code standards

- **No `.unwrap()` or `.expect()` in non-test code** — the workspace enforces `-D clippy::unwrap_used` / `expect_used` posture. Use explicit error handling with `?` and typed errors.
- Match existing module layout: `src/commands/`, `src/ci/`, thin command handlers delegating to shared pipeline code.
- Security-sensitive paths (OIDC, token handling) must not log secrets (REQ-LOG-007).
- Files end with exactly one newline; no trailing whitespace.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(change): add verify polling timeout
fix(auth): reject empty OIDC audience override
docs: expand GitLab id_tokens example
chore(ci): pin actions/checkout v4 SHA
```

## Pull requests

1. Branch from `main`.
2. Ensure CI passes: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --all-features`.
3. Describe security impact for auth/OIDC changes.
4. Link related Deslicer platform issues when applicable.

## Project layout

```
cli/
├── src/
│   ├── main.rs          # entrypoint
│   ├── cli.rs           # clap definitions
│   ├── commands/        # auth + change subcommands
│   └── ci/              # per-platform OIDC adapters
├── tests/               # integration tests
├── .github/workflows/   # CI, release, Homebrew, crates.io
└── docs/                # user-facing documentation
```

## Release

Maintainers only — see [release-process.md](release-process.md).

## License

By contributing, you agree that your contributions are licensed under the Apache-2.0 license.
