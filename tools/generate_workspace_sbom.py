#!/usr/bin/env python3
"""Generate one deterministic CycloneDX SBOM for the complete Cargo workspace."""

from __future__ import annotations

import argparse
import copy
import datetime as dt
import hashlib
import json
import os
import subprocess
import sys
import uuid
from pathlib import Path
from typing import Any


GENERATED_NAME = "ddd4r-workspace-part.json"
REPOSITORY = "https://github.com/ddd-4-rust/ddd4r"


def command(*args: str, cwd: Path) -> str:
    completed = subprocess.run(
        args,
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return completed.stdout.strip()


def workspace_metadata(root: Path) -> dict[str, Any]:
    return json.loads(
        command(
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--all-features",
            "--no-deps",
            cwd=root,
        )
    )


def stable_ref(package: dict[str, Any]) -> str:
    return f"pkg:cargo/{package['name']}@{package['version']}"


def normalize_tree(value: Any, refs: dict[str, str], packages: dict[tuple[str, str], str]) -> Any:
    if isinstance(value, dict):
        normalized = {
            key: normalize_tree(item, refs, packages) for key, item in value.items()
        }
        name = normalized.get("name")
        version = normalized.get("version")
        purl = normalized.get("purl")
        if (
            isinstance(name, str)
            and isinstance(version, str)
            and isinstance(purl, str)
            and "download_url=file:" in purl
            and (name, version) in packages
        ):
            normalized["purl"] = packages[(name, version)]
        return normalized
    if isinstance(value, list):
        return [normalize_tree(item, refs, packages) for item in value]
    if isinstance(value, str):
        if value.startswith("pkg:cargo/") and "?download_url=file:" in value:
            base, query = value.split("?", 1)
            _, separator, fragment = query.partition("#")
            return f"{base}#{fragment}" if separator else base
        for old_ref, new_ref in refs.items():
            if value == old_ref:
                return new_ref
            if value.startswith(f"{old_ref} "):
                return f"{new_ref}#{value[len(old_ref) + 1:]}"
        return value
    return value


def merge_components(
    boms: list[dict[str, Any]], refs: dict[str, str], packages: dict[tuple[str, str], str]
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    components: dict[str, dict[str, Any]] = {}
    dependencies: dict[str, set[str]] = {}

    for bom in boms:
        root_component = normalize_tree(
            copy.deepcopy(bom["metadata"]["component"]), refs, packages
        )
        components[root_component["bom-ref"]] = root_component

        for component in normalize_tree(
            copy.deepcopy(bom.get("components", [])), refs, packages
        ):
            components.setdefault(component["bom-ref"], component)

        for dependency in normalize_tree(
            copy.deepcopy(bom.get("dependencies", [])), refs, packages
        ):
            dependencies.setdefault(dependency["ref"], set()).update(
                dependency.get("dependsOn", [])
            )

    ordered_components = [components[key] for key in sorted(components)]
    ordered_dependencies = [
        {"ref": key, "dependsOn": sorted(dependencies[key])}
        for key in sorted(dependencies)
    ]
    return ordered_components, ordered_dependencies


def iso_timestamp(epoch: int) -> str:
    return (
        dt.datetime.fromtimestamp(epoch, tz=dt.timezone.utc)
        .isoformat(timespec="seconds")
        .replace("+00:00", "Z")
    )


def generate(root: Path, output: Path, expected_packages: int) -> dict[str, Any]:
    metadata = workspace_metadata(root)
    members = set(metadata["workspace_members"])
    workspace_packages = [
        package for package in metadata["packages"] if package["id"] in members
    ]
    if len(workspace_packages) != expected_packages:
        raise RuntimeError(
            f"expected {expected_packages} workspace packages, found {len(workspace_packages)}"
        )

    emitted = [Path(package["manifest_path"]).parent / GENERATED_NAME for package in workspace_packages]
    existing = [path for path in emitted if path.exists()]
    if existing:
        raise RuntimeError(f"refusing to overwrite generated inputs: {existing[0]}")

    epoch = int(
        os.environ.get("SOURCE_DATE_EPOCH")
        or command("git", "show", "-s", "--format=%ct", "HEAD", cwd=root)
    )
    environment = os.environ.copy()
    environment["SOURCE_DATE_EPOCH"] = str(epoch)
    try:
        subprocess.run(
            [
                "cargo",
                "cyclonedx",
                "--format",
                "json",
                "--all-features",
                "--target",
                "all",
                "--spec-version",
                "1.5",
                "--override-filename",
                GENERATED_NAME.removesuffix(".json"),
            ],
            cwd=root,
            env=environment,
            check=True,
        )
        missing = [path for path in emitted if not path.is_file()]
        if missing:
            raise RuntimeError(f"cargo-cyclonedx did not emit {missing[0]}")
        boms = [json.loads(path.read_text(encoding="utf-8")) for path in emitted]
    finally:
        for path in emitted:
            path.unlink(missing_ok=True)

    refs: dict[str, str] = {}
    package_refs: dict[tuple[str, str], str] = {}
    for package, bom in zip(workspace_packages, boms, strict=True):
        new_ref = stable_ref(package)
        refs[bom["metadata"]["component"]["bom-ref"]] = new_ref
        package_refs[(package["name"], package["version"])] = new_ref

    components, dependencies = merge_components(boms, refs, package_refs)
    workspace_version = metadata["packages"][0]["version"]
    commit = command("git", "rev-parse", "HEAD", cwd=root)
    workspace_ref = f"urn:ddd4r:workspace@{workspace_version}"
    dependencies.append(
        {
            "ref": workspace_ref,
            "dependsOn": sorted(stable_ref(package) for package in workspace_packages),
        }
    )
    dependencies.sort(key=lambda dependency: dependency["ref"])

    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, f'{REPOSITORY}@{commit}')}",
        "version": 1,
        "metadata": {
            "timestamp": iso_timestamp(epoch),
            "tools": [
                {
                    "vendor": "CycloneDX",
                    "name": "cargo-cyclonedx",
                    "version": "0.5.9",
                },
                {
                    "vendor": "ddd-4-rust",
                    "name": "generate_workspace_sbom.py",
                    "version": "1",
                },
            ],
            "authors": [{"name": "ddd-4-rust contributors"}],
            "component": {
                "type": "framework",
                "bom-ref": workspace_ref,
                "name": "ddd4r-workspace",
                "version": workspace_version,
                "licenses": [{"expression": "MIT OR Apache-2.0"}],
                "externalReferences": [{"type": "vcs", "url": REPOSITORY}],
            },
            "properties": [
                {"name": "ddd4r:git-commit", "value": commit},
                {
                    "name": "ddd4r:workspace-package-count",
                    "value": str(len(workspace_packages)),
                },
            ],
        },
        "components": components,
        "dependencies": dependencies,
    }
    encoded = json.dumps(bom, ensure_ascii=False, indent=2) + "\n"
    forbidden = (str(root), "file://")
    if any(value in encoded for value in forbidden):
        raise RuntimeError("SBOM contains a local filesystem reference")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(encoded, encoding="utf-8")
    return {
        "output": str(output),
        "sha256": hashlib.sha256(encoded.encode()).hexdigest(),
        "workspace_packages": len(workspace_packages),
        "components": len(components),
        "dependencies": len(dependencies),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default="dist/ddd4r-workspace.cdx.json")
    parser.add_argument("--expected-packages", type=int, default=84)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    output = (root / args.output).resolve()
    try:
        result = generate(root, output, args.expected_packages)
    except (OSError, subprocess.CalledProcessError, RuntimeError, ValueError) as error:
        print(f"SBOM generation failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
