#!/usr/bin/env python3
"""Generate the audited ddd4j-to-ddd4r core public API mapping."""

from __future__ import annotations

import argparse
import json
import tomllib
from pathlib import Path


DIRECT_CONTRACTS = [
    ("io.ddd4j.core.ddd.model.DomainModel", "ddd4r_core::domain::DomainModel"),
    ("io.ddd4j.core.ddd.model.Entity", "ddd4r_core::domain::Entity"),
    ("io.ddd4j.core.ddd.model.ValueObject", "ddd4r_core::domain::ValueObject"),
    ("io.ddd4j.core.ddd.model.AggregateRoot", "ddd4r_core::domain::AggregateRoot"),
    (
        "io.ddd4j.core.ddd.model.DomainObjectMapper",
        "ddd4r_core::mapper::DomainObjectMapper",
    ),
    ("io.ddd4j.core.ddd.event.DomainEvent", "ddd4r_core::event::DomainEvent"),
    (
        "io.ddd4j.core.ddd.event.DomainEventPublisher",
        "ddd4r_core::event::DomainEventPublisher",
    ),
    (
        "io.ddd4j.core.ddd.repository.Repository",
        "ddd4r_core::repository::Repository",
    ),
    (
        "io.ddd4j.core.ddd.repository.RepositoryRegistry",
        "ddd4r_core::repository::RepositoryRegistry",
    ),
    (
        "io.ddd4j.core.ddd.repository.EventSourcingRepository",
        "ddd4r_core::event_sourcing::EventSourcingRepository",
    ),
    ("io.ddd4j.core.cqrs.command.Command", "ddd4r_core::command::Command"),
    (
        "io.ddd4j.core.cqrs.command.CommandExecutor",
        "ddd4r_core::command::CommandExecutor",
    ),
    ("io.ddd4j.core.cqrs.command.CommandBus", "ddd4r_core::command::CommandBus"),
    ("io.ddd4j.core.cqrs.query.PropertyRef", "ddd4r_core::query::PropertyRef"),
    ("io.ddd4j.core.api.Page", "ddd4r_core::query::Page"),
    (
        "io.ddd4j.core.cqrs.readmodel.EventChunk",
        "ddd4r_core::projection::EventChunk",
    ),
    (
        "io.ddd4j.core.cqrs.readmodel.EventChunkReader",
        "ddd4r_core::projection::EventChunkReader",
    ),
    (
        "io.ddd4j.core.cqrs.readmodel.ProjectionPosition",
        "ddd4r_core::projection::ProjectionPosition",
    ),
    (
        "io.ddd4j.core.cqrs.readmodel.ProjectionPositionRepository",
        "ddd4r_core::projection::ProjectionPositionRepository",
    ),
    (
        "io.ddd4j.core.cqrs.readmodel.ProjectionService",
        "ddd4r_core::projection::ProjectionService",
    ),
    (
        "io.ddd4j.core.cqrs.readmodel.ProjectionRunner",
        "ddd4r_core::projection::ProjectionRunner",
    ),
]

SEMANTIC_CONTRACTS = [
    ("ReadModel", "io.ddd4j.core.cqrs.readmodel", "ddd4r_core::projection::ReadModel"),
    (
        "Projection",
        "io.ddd4j.core.cqrs.readmodel.ProjectionView",
        "ddd4r_core::projection::Projection",
    ),
    (
        "Condition",
        "io.ddd4j.core.cqrs.query.LambdaCondition",
        "ddd4r_core::query::Condition",
    ),
    ("Operator", "io.ddd4j.core.cqrs.query.Query", "ddd4r_core::query::Operator"),
    ("Order", "io.ddd4j.core.cqrs.query.Query", "ddd4r_core::query::Order"),
    (
        "PageRequest",
        "io.ddd4j.core.cqrs.query.Query",
        "ddd4r_core::query::PageRequest",
    ),
]

RUST_NATIVE_CONTRACTS = [
    ("RuntimeRegistry", "ddd4r_core::runtime::RuntimeRegistry"),
    ("ContextScope", "ddd4r_core::context::ContextScope"),
    ("UnitOfWork", "ddd4r_core::uow::UnitOfWork"),
    ("OutboxStore", "ddd4r_core::uow::OutboxStore"),
]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port-manifest", type=Path, default=Path("port-manifest.toml"))
    parser.add_argument("--output", type=Path, default=Path("public-api-manifest.json"))
    args = parser.parse_args()

    port = tomllib.loads(args.port_manifest.read_text(encoding="utf-8"))
    core = next(module for module in port["module"] if module["java_artifact"] == "ddd4j-core")
    java_types = core["java_public_types"]
    java_methods = core["java_public_methods"]

    contracts = []
    for java_name, rust_name in DIRECT_CONTRACTS:
        contracts.append(
            {
                "mappingKind": "direct_type",
                "java": java_name,
                "javaMethods": sorted(
                    method for method in java_methods if method.startswith(f"{java_name}#")
                ),
                "rust": rust_name,
                "status": "in_progress",
            }
        )
    for name, java_concept, rust_name in SEMANTIC_CONTRACTS:
        contracts.append(
            {
                "mappingKind": "semantic_adapter",
                "java": java_concept if java_concept in java_types else None,
                "javaConcept": java_concept,
                "javaMethods": sorted(
                    method for method in java_methods if method.startswith(f"{java_concept}#")
                ),
                "rust": rust_name,
                "status": "in_progress",
                "compatibilityName": name,
            }
        )
    for name, rust_name in RUST_NATIVE_CONTRACTS:
        contracts.append(
            {
                "mappingKind": "rust_native_extension",
                "java": None,
                "javaMethods": [],
                "rust": rust_name,
                "status": "rust_native_extension",
                "compatibilityName": name,
            }
        )

    output = {
        "manifestVersion": 2,
        "source": {
            "repository": port["source_repository"],
            "commit": port["source_commit"],
            "baselineTag": port["baseline_tag"],
        },
        "coreContracts": contracts,
        "evidence": core["acceptance_evidence"],
    }
    args.output.write_text(
        json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
