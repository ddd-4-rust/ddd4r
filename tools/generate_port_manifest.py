#!/usr/bin/env python3
"""Generate the ddd4j Reactor to ddd4r package migration manifest."""

from __future__ import annotations

import argparse
import re
import subprocess
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class ReactorProject:
    path: str
    artifact: str
    packaging: str


SPECIAL_NAMES = {
    "ddd4j-data-jpa": "ddd4r-data-seaorm",
    "ddd4j-data-mybatis": "ddd4r-data-rbatis",
    "ddd4j-data-mybatisplus": "ddd4r-data-rbatisplus",
}

IN_PROGRESS = {
    "ddd4j",
    "ddd4j-annotation",
    "ddd4j-core",
    "ddd4j-kit",
    "ddd4j-cache",
    "ddd4j-ddd-rules",
    "ddd4j-ddd-rules-clean",
    "ddd4j-ddd-rules-cola",
    "ddd4j-data",
    "ddd4j-data-jpa",
    "ddd4j-data-mybatis",
}

API_EVIDENCE = {
    "ddd4j": ["crates/ddd4r/src/lib.rs"],
    "ddd4j-annotation": ["crates/ddd4r-annotation/src/lib.rs"],
    "ddd4j-core": ["crates/ddd4r-core/src"],
    "ddd4j-kit": ["crates/ddd4r-kit/src/lib.rs"],
    "ddd4j-cache": ["crates/ddd4r-cache/src/lib.rs"],
    "ddd4j-ddd-rules": ["modules/ddd-rules/ddd4r-ddd-rules/src/lib.rs"],
    "ddd4j-ddd-rules-clean": [
        "modules/ddd-rules/ddd4r-ddd-rules-clean/src/lib.rs"
    ],
    "ddd4j-ddd-rules-cola": [
        "modules/ddd-rules/ddd4r-ddd-rules-cola/src/lib.rs"
    ],
    "ddd4j-data": ["modules/data/ddd4r-data/src/lib.rs"],
    "ddd4j-data-jpa": ["modules/data/ddd4r-data-seaorm/src/lib.rs"],
    "ddd4j-data-mybatis": ["modules/data/ddd4r-data-rbatis/src/lib.rs"],
}

TEST_EVIDENCE = {
    "ddd4j": ["crates/ddd4r/tests/derive_contract.rs"],
    "ddd4j-annotation": ["crates/ddd4r/tests/derive_contract.rs"],
    "ddd4j-core": [
        "crates/ddd4r-core/tests/context_scope_contract.rs",
        "crates/ddd4r-core/tests/core_contract.rs",
        "crates/ddd4r-core/tests/port_contract.rs",
        "crates/ddd4r-core/tests/projection_contract.rs",
        "crates/ddd4r-outbox/src/lib.rs#tests",
    ],
    "ddd4j-kit": ["crates/ddd4r-kit/src/lib.rs#tests"],
    "ddd4j-cache": ["crates/ddd4r-cache/src/lib.rs#tests"],
    "ddd4j-ddd-rules": [
        "modules/ddd-rules/ddd4r-ddd-rules/tests/architecture_contract.rs"
    ],
    "ddd4j-ddd-rules-clean": [
        "modules/ddd-rules/ddd4r-ddd-rules/tests/architecture_contract.rs"
    ],
    "ddd4j-ddd-rules-cola": [
        "modules/ddd-rules/ddd4r-ddd-rules/tests/architecture_contract.rs"
    ],
    "ddd4j-data": [
        "modules/data/ddd4r-data-rbatis/tests/sqlite_conformance.rs",
        "modules/data/ddd4r-data-seaorm/tests/sqlite_conformance.rs",
        "modules/data/ddd4r-data-sqlx/tests/sqlite_conformance.rs",
    ],
    "ddd4j-data-jpa": [
        "modules/data/ddd4r-data-seaorm/tests/sqlite_conformance.rs"
    ],
    "ddd4j-data-mybatis": [
        "modules/data/ddd4r-data-rbatis/tests/sqlite_conformance.rs"
    ],
}

CORE_BEHAVIORS = {
    "ddd4j": ["workspace facade and prelude exports"],
    "ddd4j-annotation": ["DDD derive macros and generated property metadata"],
    "ddd4j-core": [
        "domain model and aggregate event buffering",
        "task-local-first repository and runtime registries",
        "active-record compatibility facade",
        "strongly typed query AST",
        "CQRS command and projection contracts",
        "event sourcing, unit of work, and outbox ports",
    ],
    "ddd4j-kit": ["canonical serialization helpers"],
    "ddd4j-cache": ["TTL cache, CAS, and statistics"],
    "ddd4j-ddd-rules": ["Cargo Metadata and syn architecture conformance engine"],
    "ddd4j-ddd-rules-clean": ["Clean Architecture layer and framework rules"],
    "ddd4j-ddd-rules-cola": ["COLA layer and framework rules"],
    "ddd4j-data": [
        "shared backend capability model and executable repository conformance suite",
        "aggregate and outbox atomic commit and rollback conformance suite",
    ],
    "ddd4j-data-jpa": [
        "SeaORM CRUD, query, page, optimistic lock, and explicit transaction adapter",
        "transactional aggregate and outbox persistence",
    ],
    "ddd4j-data-mybatis": [
        "RBatis CRUD, query, page, optimistic lock, and explicit transaction adapter",
        "transactional aggregate and outbox persistence",
    ],
}

PACKAGE_RE = re.compile(r"^\s*package\s+([\w.]+)\s*;", re.MULTILINE)
PUBLIC_TYPE_RE = re.compile(
    r"\bpublic\s+(?:(?:abstract|final|sealed|non-sealed|static)\s+)*"
    r"(class|interface|enum|record|@interface)\s+(\w+)"
)
PUBLIC_METHOD_RE = re.compile(
    r"\bpublic\s+(?:(?:abstract|default|final|native|static|synchronized)\s+)*"
    r"(?:<[^>{};]+>\s*)?(?:[\w.$?<>\[\],]+\s+)+(\w+)\s*\(",
    re.MULTILINE,
)
INTERFACE_METHOD_HEADER_RE = re.compile(
    r"^(?![\s\S]*\b(?:class|interface|enum|record)\b)"
    r"(?![\s\S]*\b(?:private|protected)\b)"
    r"[\s\S]*?(?:<[^;{}]+>\s*)?"
    r"[\w.$?<>,\[\] @&]+\s+(\w+)\s*\([^;{}]*\)"
    r"\s*(?:throws\s+[^;{}]+)?$"
)


def local_name(element: ET.Element) -> str:
    return element.tag.rsplit("}", 1)[-1]


def direct_child(element: ET.Element, name: str) -> ET.Element | None:
    return next((child for child in element if local_name(child) == name), None)


def direct_text(element: ET.Element, name: str, default: str) -> str:
    child = direct_child(element, name)
    return child.text.strip() if child is not None and child.text else default


def collect(source: Path) -> list[ReactorProject]:
    source = source.resolve()
    projects: list[ReactorProject] = []

    def visit(directory: Path) -> None:
        pom = directory / "pom.xml"
        if not pom.is_file():
            raise FileNotFoundError(f"missing Reactor POM: {pom}")
        root = ET.parse(pom).getroot()
        relative = "." if directory == source else directory.relative_to(source).as_posix()
        artifact = direct_text(root, "artifactId", "ddd4j")
        projects.append(
            ReactorProject(
                path=relative,
                artifact=artifact,
                packaging=direct_text(root, "packaging", "jar"),
            )
        )
        modules = direct_child(root, "modules")
        if modules is None:
            return
        for module in modules:
            if local_name(module) == "module" and module.text:
                visit((directory / module.text.strip()).resolve())

    visit(source)
    return projects


def rust_name(artifact: str) -> str:
    return SPECIAL_NAMES.get(artifact, artifact.replace("ddd4j", "ddd4r", 1))


def rust_path(project: ReactorProject) -> str:
    name = rust_name(project.artifact)
    if project.path == ".":
        return "."
    first = project.path.split("/", 1)[0]
    if first in {"ddd4j-bom", "ddd4j-dependencies", "ddd4j-annotation", "ddd4j-core", "ddd4j-kit", "ddd4j-cache", "ddd4j-parent"}:
        return f"crates/{name}"
    groups = {
        "ddd4j-ddd-rules": "ddd-rules",
        "ddd4j-data": "data",
        "ddd4j-mq": "mq",
        "ddd4j-web": "web",
        "ddd4j-auth": "auth",
        "ddd4j-runtime": "runtime",
        "ddd4j-extensions": "extensions",
        "ddd4j-samples": "samples",
    }
    return f"modules/{groups[first]}/{name}"


def git_value(source: Path, *args: str) -> str:
    return subprocess.check_output(
        ["git", *args], cwd=source, text=True, stderr=subprocess.DEVNULL
    ).strip()


def quote(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def string_array(values: list[str]) -> str:
    return "[" + ", ".join(quote(value) for value in values) + "]"


def java_sources(source: Path, project: ReactorProject, kind: str) -> list[Path]:
    directory = source if project.path == "." else source / project.path
    root = directory / "src" / kind / "java"
    return sorted(root.rglob("*.java")) if root.is_dir() else []


def strip_java_non_code(source: str) -> str:
    """Blank comments and literals while preserving offsets and newlines."""
    result: list[str] = []
    index = 0
    state = "code"
    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""
        if state == "code":
            if char == "/" and following == "*":
                result.extend((" ", " "))
                index += 2
                state = "block_comment"
                continue
            if char == "/" and following == "/":
                result.extend((" ", " "))
                index += 2
                state = "line_comment"
                continue
            if char == '"':
                result.append(" ")
                index += 1
                state = "string"
                continue
            if char == "'":
                result.append(" ")
                index += 1
                state = "character"
                continue
            result.append(char)
            index += 1
            continue
        if state == "block_comment":
            if char == "*" and following == "/":
                result.extend((" ", " "))
                index += 2
                state = "code"
            else:
                result.append("\n" if char == "\n" else " ")
                index += 1
            continue
        if state == "line_comment":
            if char == "\n":
                result.append("\n")
                state = "code"
            else:
                result.append(" ")
            index += 1
            continue
        if char == "\\" and following:
            result.extend((" ", "\n" if following == "\n" else " "))
            index += 2
            continue
        if (state == "string" and char == '"') or (
            state == "character" and char == "'"
        ):
            result.append(" ")
            index += 1
            state = "code"
            continue
        result.append("\n" if char == "\n" else " ")
        index += 1
    return "".join(result)


def interface_public_methods(source: str, type_end: int) -> set[str]:
    """Collect declarations located directly in an interface body."""
    body_start = source.find("{", type_end)
    if body_start < 0:
        return set()
    methods: set[str] = set()
    depth = 1
    declaration_start = body_start + 1
    index = declaration_start
    while index < len(source) and depth:
        char = source[index]
        if char == "{" and depth == 1:
            header = source[declaration_start:index].strip()
            match = INTERFACE_METHOD_HEADER_RE.match(header)
            if match:
                methods.add(match.group(1))
            depth += 1
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 1:
                declaration_start = index + 1
        elif char == ";" and depth == 1:
            header = source[declaration_start:index].strip()
            match = INTERFACE_METHOD_HEADER_RE.match(header)
            if match:
                methods.add(match.group(1))
            declaration_start = index + 1
        index += 1
    return methods


def inspect_java_api(source: Path, project: ReactorProject) -> tuple[list[str], list[str], list[str]]:
    packages: set[str] = set()
    public_types: set[str] = set()
    public_methods: set[str] = set()
    for java_file in java_sources(source, project, "main"):
        text = strip_java_non_code(java_file.read_text(encoding="utf-8"))
        package_match = PACKAGE_RE.search(text)
        package_name = package_match.group(1) if package_match else ""
        if package_name:
            packages.add(package_name)
        type_matches = list(PUBLIC_TYPE_RE.finditer(text))
        types = [(match.group(1), match.group(2)) for match in type_matches]
        for _, type_name in types:
            public_types.add(f"{package_name}.{type_name}" if package_name else type_name)
        owner = types[0][1] if types else java_file.stem
        methods: set[str]
        if types and types[0][0] in {"interface", "@interface"}:
            methods = interface_public_methods(text, type_matches[0].end())
        else:
            methods = set(PUBLIC_METHOD_RE.findall(text))
        for method_name in methods:
            public_methods.add(
                f"{package_name}.{owner}#{method_name}" if package_name else f"{owner}#{method_name}"
            )
    return sorted(packages), sorted(public_types), sorted(public_methods)


def direct_dependencies(source: Path, project: ReactorProject) -> list[str]:
    directory = source if project.path == "." else source / project.path
    root = ET.parse(directory / "pom.xml").getroot()
    dependencies = direct_child(root, "dependencies")
    if dependencies is None:
        return []
    result: set[str] = set()
    for dependency in dependencies:
        if local_name(dependency) != "dependency":
            continue
        group = direct_text(dependency, "groupId", "")
        artifact = direct_text(dependency, "artifactId", "")
        if artifact:
            result.add(f"{group}:{artifact}" if group else artifact)
    return sorted(result)


def java_tests(source: Path, project: ReactorProject) -> list[str]:
    return [path.relative_to(source).as_posix() for path in java_sources(source, project, "test")]


def render(source: Path, projects: list[ReactorProject]) -> str:
    commit = git_value(source, "rev-parse", "HEAD")
    branch = git_value(source, "branch", "--show-current")
    baseline_commit = git_value(source, "rev-list", "-n", "1", "ddd4r-port-baseline-2026-07-21")
    lines = [
        "# Generated by tools/generate_port_manifest.py; do not edit module rows manually.",
        "manifest_version = 2",
        'source_repository = "https://github.com/ddd-4-java/ddd4j"',
        f"source_checkout = {quote(str(source))}",
        f"source_branch = {quote(branch)}",
        f"source_commit = {quote(commit)}",
        'baseline_tag = "ddd4r-port-baseline-2026-07-21"',
        f"baseline_commit = {quote(baseline_commit)}",
        f"reactor_project_count = {len(projects)}",
        "",
    ]
    for index, project in enumerate(projects, start=1):
        status = "in_progress" if project.artifact in IN_PROGRESS else "scaffolded"
        packages, public_types, public_methods = inspect_java_api(source, project)
        api_evidence = API_EVIDENCE.get(project.artifact, [])
        test_evidence = TEST_EVIDENCE.get(project.artifact, [])
        lines.extend(
            [
                "[[module]]",
                f"index = {index}",
                f"java_artifact = {quote(project.artifact)}",
                f"java_path = {quote(project.path)}",
                f"java_packaging = {quote(project.packaging)}",
                f"java_packages = {string_array(packages)}",
                f"java_public_types = {string_array(public_types)}",
                f"java_public_methods = {string_array(public_methods)}",
                f"java_dependencies = {string_array(direct_dependencies(source, project))}",
                f"java_tests = {string_array(java_tests(source, project))}",
                f"core_behaviors = {string_array(CORE_BEHAVIORS.get(project.artifact, []))}",
                f"rust_package = {quote(rust_name(project.artifact))}",
                f"rust_path = {quote(rust_path(project))}",
                f"status = {quote(status)}",
                f"api_evidence = {string_array(api_evidence)}",
                f"test_evidence = {string_array(test_evidence)}",
                f"acceptance_evidence = {string_array([*api_evidence, *test_evidence])}",
                'known_semantic_differences = []',
                "",
            ]
        )
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("port-manifest.toml"))
    args = parser.parse_args()
    projects = collect(args.source)
    if len(projects) != 82:
        raise SystemExit(f"expected 82 Reactor projects, found {len(projects)}")
    args.output.write_text(render(args.source.resolve(), projects), encoding="utf-8")


if __name__ == "__main__":
    main()
