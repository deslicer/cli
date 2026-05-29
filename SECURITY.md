# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| latest `v1.x` release | yes |
| older major/minor | no |

## Reporting a Vulnerability

Please report security issues **privately** — do not open a public GitHub issue.

- **Email:** security@deslicer.ai (preferred)
- **Engineering contact:** engineering@deslicer.ai

Include steps to reproduce, affected versions, and impact if known. We aim to acknowledge reports within **2 business days** and will coordinate disclosure once a fix is available.

Signed release artifacts (cosign keyless + SLSA L3 provenance) are published on each [GitHub Release](https://github.com/deslicer/cli/releases). Verify downloads with the attached `.sig`, `.cert`, and `multiple.intoto.jsonl` before use in production pipelines.
