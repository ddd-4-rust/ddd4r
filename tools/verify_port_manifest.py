#!/usr/bin/env python3
"""Validate migration-manifest invariants without requiring the Java checkout."""

from __future__ import annotations

import argparse
import tomllib
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=Path("port-manifest.toml"))
    args = parser.parse_args()
    data = tomllib.loads(args.manifest.read_text(encoding="utf-8"))
    modules = data["module"]
    if data["reactor_project_count"] != 82 or len(modules) != 82:
        raise SystemExit("port manifest must contain exactly 82 Reactor projects")
    java = [module["java_artifact"] for module in modules]
    rust = [module["rust_package"] for module in modules]
    if len(java) != len(set(java)):
        raise SystemExit("duplicate Java artifacts in port manifest")
    if len(rust) != len(set(rust)):
        raise SystemExit("duplicate Rust packages in port manifest")
    allowed = {"planned", "scaffolded", "in_progress", "complete", "blocked"}
    invalid = [module for module in modules if module["status"] not in allowed]
    if invalid:
        raise SystemExit(f"invalid module statuses: {invalid}")
    for module in modules:
        if module["status"] == "complete":
            if not module["api_evidence"] or not module["test_evidence"]:
                raise SystemExit(
                    f"complete module lacks evidence: {module['rust_package']}"
                )
    print("validated 82 unique module mappings")


if __name__ == "__main__":
    main()
