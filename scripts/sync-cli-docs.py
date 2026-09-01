#!/usr/bin/env python3
"""Copy curated CLI docs into deslicer/docs products/cli/.

Reads docs/sync-manifest.toml. Does not publish to docs.deslicer.io
(that site allowlists products/enterprise/ only).
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

FOOTER_PREFIX = "<!-- Synced from deslicer/cli@"
CLI_README_ROW = (
    "| **[CLI](products/cli/README.md)** | Deslicer CLI (`deslicer`) — "
    "Path A / A2 / B change plans | "
    "[![Available](https://img.shields.io/badge/Available-6911AB?style=flat-square)]"
    "(products/cli/README.md) |\n"
)


def load_manifest(path: Path) -> dict:
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    if "page" not in data or "sync" not in data:
        raise SystemExit(f"{path}: missing [sync] or [[page]]")
    return data


def first_heading(text: str) -> str:
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("# "):
            return stripped[2:].strip()
    return ""


def write_page(source: Path, dest: Path, sha: str) -> None:
    body = source.read_text(encoding="utf-8").rstrip() + "\n"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(
        f"{body}\n{FOOTER_PREFIX}{sha} — edit the source in deslicer/cli, not here. -->\n",
        encoding="utf-8",
    )


def write_product_readme(dest_dir: Path, pages: list[dict], cli_root: Path) -> None:
    rows = []
    for index, page in enumerate(pages, start=1):
        source = cli_root / page["source"]
        title = first_heading(source.read_text(encoding="utf-8")) or page["id"]
        rows.append(f"| {index} | [{title}]({page['dest']}) |")
    dest_dir.joinpath("README.md").write_text(
        "\n".join(
            [
                "# Deslicer CLI (`deslicer`)",
                "",
                "Vendor-neutral CI client for planning, approving, and shipping",
                "Splunk changes via DAP.",
                "",
                "Chapters are copied from [deslicer/cli](https://github.com/deslicer/cli)",
                "`docs/`. Edit the source there. `deslicer docs <topic>` prints the",
                "GitHub URL (or `DESLICER_DOCS_BASE_URL` when a hosted `/cli` tree exists).",
                "",
                "| Chapter | Title |",
                "|:-------:|:------|",
                *rows,
                "",
            ]
        )
        + "\n",
        encoding="utf-8",
    )


def insert_docs_readme_row(docs_root: Path) -> None:
    readme = docs_root / "README.md"
    if not readme.is_file():
        return
    text = readme.read_text(encoding="utf-8")
    if "products/cli/README.md" in text:
        return
    needle = "| **[Enterprise](products/enterprise/README.md)**"
    idx = text.find(needle)
    if idx < 0:
        return
    line_end = text.find("\n", idx)
    if line_end < 0:
        return
    updated = text[: line_end + 1] + CLI_README_ROW + text[line_end + 1 :]
    readme.write_text(updated, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli-root", type=Path, default=Path.cwd())
    parser.add_argument("--docs-root", type=Path, required=True)
    parser.add_argument("--sha", required=True, help="git SHA stamped into footers")
    args = parser.parse_args()

    cli_root = args.cli_root.resolve()
    docs_root = args.docs_root.resolve()
    manifest_path = cli_root / "docs" / "sync-manifest.toml"
    manifest = load_manifest(manifest_path)
    dest_dir = docs_root / manifest["sync"]["dest_dir"]
    pages = manifest["page"]

    sha = args.sha.strip()
    if not sha or any(ch in sha for ch in " \n\r\t"):
        raise SystemExit("refusing empty or whitespace-bearing --sha")

    for page in pages:
        source = cli_root / page["source"]
        if not source.is_file():
            raise SystemExit(f"missing source {source}")
        dest = dest_dir / page["dest"]
        if ".." in Path(page["dest"]).parts:
            raise SystemExit(f"illegal dest {page['dest']}")
        write_page(source, dest, sha)

    write_product_readme(dest_dir, pages, cli_root)
    insert_docs_readme_row(docs_root)
    print(f"synced {len(pages)} pages -> {dest_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
