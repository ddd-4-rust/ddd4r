#!/usr/bin/env python3
"""Create honest, compiling package boundaries for mapped Reactor projects.

The generated crates intentionally report ModuleMaturity::Scaffolded. They are
not considered implemented and the 1.0 gate rejects that status.
"""

from __future__ import annotations

import argparse
import tomllib
from pathlib import Path


def group_for(path: str) -> str:
    if path.startswith("modules/"):
        return path.split("/", 2)[1]
    return "foundation"


def cargo_toml(name: str, java_artifact: str) -> str:
    return f'''[package]
name = "{name}"
description = "ddd4r compatibility package for {java_artifact}"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
publish = false

[dependencies]
ddd4r-core = {{ path = "{relative_core(name)}", version = "=0.1.0-alpha.1" }}

[lints]
workspace = true
'''


def relative_core(name: str) -> str:
    # Foundation crates live under crates/<name>; grouped modules are one level deeper.
    return "../ddd4r-core" if name in {"ddd4r-bom", "ddd4r-dependencies", "ddd4r-parent"} else "../../../crates/ddd4r-core"


def lib_rs(java_artifact: str, rust_package: str, group: str) -> str:
    return f'''//! Compatibility boundary for `{java_artifact}`.
//!
//! This package is scaffolded and cannot be published until its entry in
//! `port-manifest.toml` contains API and test evidence.

#![forbid(unsafe_code)]

use ddd4r_core::module::{{ModuleDescriptor, ModuleMaturity}};

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {{
    java_artifact: "{java_artifact}",
    rust_package: "{rust_package}",
    group: "{group}",
    maturity: ModuleMaturity::Scaffolded,
}};
'''


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=Path("port-manifest.toml"))
    args = parser.parse_args()
    data = tomllib.loads(args.manifest.read_text(encoding="utf-8"))
    created = 0
    for module in data["module"]:
        path = Path(module["rust_path"])
        if path == Path(".") or path.exists():
            continue
        source = path / "src"
        source.mkdir(parents=True)
        name = module["rust_package"]
        (path / "Cargo.toml").write_text(
            cargo_toml(name, module["java_artifact"]), encoding="utf-8"
        )
        (source / "lib.rs").write_text(
            lib_rs(module["java_artifact"], name, group_for(module["rust_path"])),
            encoding="utf-8",
        )
        created += 1
    print(f"created {created} mapped package boundaries")


if __name__ == "__main__":
    main()
