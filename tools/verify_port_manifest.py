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
    if data["manifest_version"] != 2:
        raise SystemExit("port manifest version must be 2")
    if data["source_commit"] != data["baseline_commit"]:
        raise SystemExit("source commit must equal the immutable baseline tag commit")
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
    required_fields = {
        "java_packages",
        "java_public_types",
        "java_public_methods",
        "java_dependencies",
        "java_tests",
        "core_behaviors",
        "api_evidence",
        "test_evidence",
        "acceptance_evidence",
        "known_semantic_differences",
    }
    for module in modules:
        missing = sorted(required_fields.difference(module))
        if missing:
            raise SystemExit(
                f"module {module['java_artifact']} lacks audit fields: {missing}"
            )
        if module["status"] == "complete":
            if not module["api_evidence"] or not module["test_evidence"]:
                raise SystemExit(
                    f"complete module lacks evidence: {module['rust_package']}"
                )
    print("validated 82 unique module mappings")


if __name__ == "__main__":
    main()
