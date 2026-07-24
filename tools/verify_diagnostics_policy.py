#!/usr/bin/env python3
"""Prevent privileged diagnostics and remote exporters from leaking into the facade."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path


PROFILER_CRATES = {
    "ddd4r-diagnostics-heap",
    "ddd4r-diagnostics-pprof",
    "ddd4r-diagnostics-tokio-console",
    "ddd4r-diagnostics-tracing",
}
EXPLICIT_EXPORT_CRATES = {"ddd4r-observability-otlp"}


def load(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    facade = load(root / "crates" / "ddd4r" / "Cargo.toml")
    dependencies = set(facade.get("dependencies", {}))
    leaked = sorted((PROFILER_CRATES | EXPLICIT_EXPORT_CRATES) & dependencies)
    if leaked:
        print(
            f"explicit diagnostics/exporters leaked into ddd4r facade dependencies: {leaked}",
            file=sys.stderr,
        )
        return 1

    default_features = set(facade.get("features", {}).get("default", []))
    if "observability" not in default_features:
        print("ddd4r default features must include observability", file=sys.stderr)
        return 1

    for crate in sorted(PROFILER_CRATES):
        manifest_path = root / "crates" / crate / "Cargo.toml"
        if not manifest_path.is_file():
            print(f"missing explicit diagnostics crate: {crate}", file=sys.stderr)
            return 1

    for crate in sorted(EXPLICIT_EXPORT_CRATES):
        manifest_path = root / "crates" / crate / "Cargo.toml"
        if not manifest_path.is_file():
            print(f"missing explicit exporter crate: {crate}", file=sys.stderr)
            return 1
        exporter = load(manifest_path)
        exporter_dependencies = set(exporter.get("dependencies", {}))
        if "ddd4r-observability" not in exporter_dependencies:
            print(
                f"{crate} must extend ddd4r-observability instead of replacing it",
                file=sys.stderr,
            )
            return 1

    console = load(
        root / "crates" / "ddd4r-diagnostics-tokio-console" / "Cargo.toml"
    )
    features = console.get("features", {})
    if features.get("default") != []:
        print("Tokio Console must compile no runtime backend by default", file=sys.stderr)
        return 1
    if "dep:console-subscriber" not in features.get("runtime", []):
        print("Tokio Console runtime feature must be explicit", file=sys.stderr)
        return 1

    print("diagnostics and exporter compile-time isolation policy valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
