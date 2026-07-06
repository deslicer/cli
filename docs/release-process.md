# Release process

This document describes how `deslicer-cli` versions move from a git tag to installable artifacts on GitHub Releases, Homebrew, and crates.io.

## Overview

```
git tag vX.Y.Z
       │
       ▼
  release.yml ──► build 5 targets + SHA256 sidecars
       │          cosign keyless signatures (Sigstore OIDC)
       │          SLSA provenance attestation
       │          GitHub Release (archives + sigs + certs)
       │          floating tag v1 ──► latest v1.x compatible release
       │
       ├──► homebrew.yml (on: release published)
       │         └── PR to deslicer/homebrew-tap
       │
       └──► crates-publish.yml (on: release published)
                 └── cargo publish -p deslicer-cli (OIDC Trusted Publishing)
```

## Prerequisites

1. **Cargo.toml version** matches the tag (tag `v1.2.3` → `version = "1.2.3"`).
2. **crates.io Trusted Publishing** configured for `github.com/deslicer/cli` / `crates-publish.yml`.
3. **`HOMEBREW_TAP_TOKEN`** secret on the repo — PAT or GitHub App token with write access to `deslicer/homebrew-tap`.
4. Changelog / release notes prepared (GitHub auto-generates notes; edit after publish if needed).

## Step 1 — Tag and push

```bash
# Ensure main is clean and version bumped in Cargo.toml
git checkout main
git pull

git tag v1.0.0
git push origin v1.0.0
```

Tag pattern **`v*.*.*`** triggers `.github/workflows/release.yml`.

## Step 2 — release.yml (automatic)

### Build matrix

| Target | OS runner | Archive |
|--------|-----------|---------|
| `x86_64-unknown-linux-musl` | ubuntu-latest | `deslicer-x86_64-unknown-linux-musl.tar.gz` |
| `aarch64-unknown-linux-musl` | ubuntu-latest | `deslicer-aarch64-unknown-linux-musl.tar.gz` |
| `x86_64-apple-darwin` | macos-latest | `deslicer-x86_64-apple-darwin.tar.gz` |
| `aarch64-apple-darwin` | macos-latest | `deslicer-aarch64-apple-darwin.tar.gz` |
| `x86_64-pc-windows-msvc` | windows-latest | `deslicer-x86_64-pc-windows-msvc.zip` |

Each archive has a **`.sha256`** sidecar file.

### Signing and provenance

The `publish` job (only job with `id-token: write`):

- Installs **cosign** and signs each archive with **keyless Sigstore** (`cosign sign-blob --yes`). No secret beyond `GITHUB_TOKEN` for the Release itself.
- Creates/updates the **GitHub Release** attached to the tag.
- Moves the floating **`v1`** tag to the release commit (`git push origin refs/tags/v1 --force`).

The `provenance` job invokes **SLSA Level 3** generator (`slsa-framework/slsa-github-generator`). Pin the reusable workflow SHA before production use (see workflow TODO comment).

## Step 3 — Downstream publish (automatic on `release: published`)

### Homebrew (`homebrew.yml`)

- Downloads darwin + linux release tarballs.
- Computes SHA256 checksums.
- Opens a PR against [deslicer/homebrew-tap](https://github.com/deslicer/homebrew-tap) updating `Formula/deslicer.rb`.

Manual dispatch is also available:

```bash
gh workflow run homebrew.yml -f version=v1.0.0
```

### crates.io (`crates-publish.yml`)

- Verifies `Cargo.toml` version matches the release tag.
- Runs `cargo publish -p deslicer-cli --locked` using **Trusted Publishing** (GitHub OIDC → crates.io). No `CARGO_REGISTRY_TOKEN` secret.

## Floating `v1` tag policy

The **`v1`** tag always points at the latest **`v1.*.*`** release commit. It is force-moved on every stable v1.x publish so consumers can pin:

```bash
curl -fsSL .../releases/download/v1/deslicer-x86_64-unknown-linux-musl.tar.gz
```

Semver-breaking **`v2.0.0`** will introduce a separate floating `v2` tag policy when that major ships.

## Verification checklist

After a release:

- [ ] GitHub Release contains all five archives, `.sha256` files, and cosign `.sig`/`.cert` pairs
- [ ] `cosign verify-blob` succeeds against Sigstore for at least one artifact
- [ ] Homebrew tap PR merged (or dispatched manually)
- [ ] `deslicer-cli` version visible on https://crates.io/crates/deslicer-cli
- [ ] `v1` tag points to the new commit

## Rollback

1. **GitHub Release** — mark pre-release or delete the release (does not remove the git tag).
2. **Homebrew** — revert the tap PR or pin the previous formula version.
3. **crates.io** — yank the broken crate version (`cargo yank -p deslicer-cli --version X.Y.Z`); publish a patch release.
4. **`v1` tag** — manually reset to the last good commit if needed.

## CI-only validation

Every push/PR to `main` runs `.github/workflows/ci.yml` (fmt, clippy, tests). Dependency policy scanning (`cargo deny`, `cargo audit`) runs non-blocking until `deny.toml` lands.

## Edge builds on merge to main

Every merge to `main` also runs `.github/workflows/build-main.yml`, which compiles the same five release targets and uploads the raw binaries as workflow artifacts (14-day retention). This catches cross-compile breakage between releases and gives maintainers pre-release binaries to smoke-test:

```bash
gh run list --workflow build-main.yml --limit 1
gh run download <run-id> --name deslicer-edge-aarch64-apple-darwin
```

Edge artifacts are unsigned and never published to a Release — distribution stays tag-gated through `release.yml`.

## How users update

| Install channel | Update command |
|-----------------|----------------|
| curl script / manual download | `deslicer update` (self-update; verifies the `.sha256` sidecar) |
| Homebrew | `brew upgrade deslicer` |
| cargo | `cargo install deslicer-cli --force` |
| Windows | Download the new `.zip` from GitHub Releases |

`deslicer update` resolves the latest **stable** tag via the GitHub API (prereleases are excluded), so publishing an `-rc` tag is safe: self-updaters and Homebrew both skip it.
