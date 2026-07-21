#!/usr/bin/env python3
"""Validate the audited ddd4j-to-ddd4r public API mapping."""

from __future__ import annotations

import argparse
import json
import tomllib
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=Path("public-api-manifest.json"))
    parser.add_argument("--port-manifest", type=Path, default=Path("port-manifest.toml"))
    args = parser.parse_args()

    public_api = json.loads(args.manifest.read_text(encoding="utf-8"))
    port = tomllib.loads(args.port_manifest.read_text(encoding="utf-8"))
    core = next(module for module in port["module"] if module["java_artifact"] == "ddd4j-core")
    java_types = set(core["java_public_types"])
    contracts = public_api["coreContracts"]

    if public_api["manifestVersion"] != 2:
        raise SystemExit("public API manifest version must be 2")
    if public_api["source"]["commit"] != port["baseline_commit"]:
        raise SystemExit("public API manifest must target the immutable baseline commit")
    rust_names = [contract["rust"] for contract in contracts]
    if len(rust_names) != len(set(rust_names)):
        raise SystemExit("duplicate Rust contracts in public API manifest")

    allowed_kinds = {"direct_type", "semantic_adapter", "rust_native_extension"}
    for contract in contracts:
        kind = contract.get("mappingKind")
        if kind not in allowed_kinds:
            raise SystemExit(f"invalid mapping kind for {contract['rust']}: {kind}")
        java_type = contract.get("java")
        if java_type == "new":
            raise SystemExit(f"placeholder Java mapping for {contract['rust']}")
        if kind == "direct_type" and java_type not in java_types:
            raise SystemExit(f"unknown direct Java type for {contract['rust']}: {java_type}")
        if kind == "semantic_adapter" and not contract.get("javaConcept"):
            raise SystemExit(f"semantic adapter lacks Java concept: {contract['rust']}")
        if kind == "rust_native_extension" and java_type is not None:
            raise SystemExit(f"Rust-native contract must not claim a Java type: {contract['rust']}")

    print(f"validated {len(contracts)} audited core contract mappings")


if __name__ == "__main__":
    main()
