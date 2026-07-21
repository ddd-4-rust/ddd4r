//! Cargo-native architecture conformance engine used by the Clean and COLA adapters.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use cargo_metadata::{MetadataCommand, Package};
use ddd4r_core::module::{ModuleDescriptor, ModuleMaturity};
use serde::{Deserialize, Serialize};
use syn::visit::{self, Visit};
use thiserror::Error;

/// Machine-readable migration descriptor.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    java_artifact: "ddd4j-ddd-rules",
    rust_package: "ddd4r-ddd-rules",
    group: "ddd-rules",
    maturity: ModuleMaturity::InProgress,
};

/// One architectural layer ordered from policy core to infrastructure edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// Enterprise and domain rules.
    Domain,
    /// Use cases and application orchestration.
    Application,
    /// Inbound and outbound interface adapters.
    Adapter,
    /// Frameworks, drivers, and technical implementation.
    Infrastructure,
    /// Package without an explicit or conventional layer marker.
    Unclassified,
}

impl Layer {
    fn parse(value: &str) -> Self {
        match value {
            "domain" => Self::Domain,
            "application" | "app" => Self::Application,
            "adapter" | "adapters" => Self::Adapter,
            "infrastructure" | "infra" => Self::Infrastructure,
            _ => Self::Unclassified,
        }
    }
}

/// Supported dependency policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureStyle {
    /// Clean Architecture dependency rule.
    Clean,
    /// COLA four-layer dependency rule.
    Cola,
}

/// Configuration for one workspace audit.
#[derive(Debug, Clone)]
pub struct ArchitecturePolicy {
    /// Policy family.
    pub style: ArchitectureStyle,
    /// Whether all four architectural layers must be represented.
    pub require_all_layers: bool,
    /// Framework crate names forbidden from the domain layer.
    pub domain_frameworks: BTreeSet<String>,
}

impl ArchitecturePolicy {
    /// Creates the default Clean Architecture policy.
    pub fn clean() -> Self {
        Self::new(ArchitectureStyle::Clean)
    }

    /// Creates the default COLA policy.
    pub fn cola() -> Self {
        Self::new(ArchitectureStyle::Cola)
    }

    fn new(style: ArchitectureStyle) -> Self {
        Self {
            style,
            require_all_layers: true,
            domain_frameworks: [
                "actix-web",
                "axum",
                "cargo_metadata",
                "diesel",
                "lapin",
                "poem",
                "rbatis",
                "redis",
                "rocket",
                "salvo",
                "sea-orm",
                "sqlx",
                "tokio",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

/// Stable architecture violation emitted by the checker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Violation {
    /// Stable rule identifier suitable for CI allow/deny policies.
    pub rule: String,
    /// Package that violates the rule.
    pub package: String,
    /// Resolved layer of the source package.
    pub layer: Layer,
    /// Referenced package or crate when applicable.
    pub dependency: Option<String>,
    /// Human-readable diagnostic.
    pub message: String,
}

/// Complete result of one workspace audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureReport {
    /// Evaluated policy family.
    pub style: ArchitectureStyle,
    /// Number of workspace packages inspected.
    pub checked_packages: usize,
    /// All detected violations in stable order.
    pub violations: Vec<Violation>,
}

impl ArchitectureReport {
    /// Returns whether the workspace satisfies the policy.
    pub fn is_compliant(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Failure to load or parse a workspace under audit.
#[derive(Debug, Error)]
pub enum CheckerError {
    /// Cargo metadata could not be loaded.
    #[error("failed to load Cargo metadata: {0}")]
    Metadata(#[from] cargo_metadata::Error),
    /// A Rust source could not be read.
    #[error("failed to read Rust source {path}: {source}")]
    ReadSource {
        /// Source path.
        path: PathBuf,
        /// I/O failure.
        source: std::io::Error,
    },
    /// A Rust source could not be parsed by `syn`.
    #[error("failed to parse Rust source {path}: {source}")]
    ParseSource {
        /// Source path.
        path: PathBuf,
        /// Parser failure.
        source: syn::Error,
    },
}

/// Cargo Metadata plus `syn` based architecture checker.
#[derive(Debug, Clone)]
pub struct ArchitectureChecker {
    policy: ArchitecturePolicy,
}

impl ArchitectureChecker {
    /// Creates a checker for a policy.
    pub const fn new(policy: ArchitecturePolicy) -> Self {
        Self { policy }
    }

    /// Audits a Cargo workspace from its root manifest.
    pub fn check(
        &self,
        manifest_path: impl AsRef<Path>,
    ) -> Result<ArchitectureReport, CheckerError> {
        let metadata = MetadataCommand::new()
            .manifest_path(manifest_path.as_ref())
            .no_deps()
            .exec()?;
        let members = metadata.workspace_members.iter().collect::<BTreeSet<_>>();
        let packages = metadata
            .packages
            .iter()
            .filter(|package| members.contains(&package.id))
            .collect::<Vec<_>>();
        let layers = packages
            .iter()
            .map(|package| (package.name.as_str(), package_layer(package)))
            .collect::<BTreeMap<_, _>>();
        let mut violations = self.missing_layer_violations(&packages, &layers);

        for package in &packages {
            let source_layer = layers[package.name.as_str()];
            let source_root = package
                .manifest_path
                .parent()
                .map(|path| path.join("src"))
                .unwrap_or_default();
            let used_crates = referenced_crates(source_root.as_std_path())?;
            for dependency in &package.dependencies {
                let crate_name = dependency
                    .rename
                    .as_deref()
                    .unwrap_or(&dependency.name)
                    .replace('-', "_");
                if !used_crates.contains(&crate_name) {
                    continue;
                }
                if let Some(target_layer) = layers.get(dependency.name.as_str()).copied()
                    && !layer_dependency_allowed(source_layer, target_layer)
                {
                    violations.push(Violation {
                        rule: "layer-dependency-direction".to_owned(),
                        package: package.name.to_string(),
                        layer: source_layer,
                        dependency: Some(dependency.name.clone()),
                        message: format!(
                            "{:?} package {} must not depend on {:?} package {}",
                            source_layer, package.name, target_layer, dependency.name
                        ),
                    });
                }
                if source_layer == Layer::Domain
                    && self.policy.domain_frameworks.contains(&dependency.name)
                {
                    violations.push(Violation {
                        rule: "domain-framework-independence".to_owned(),
                        package: package.name.to_string(),
                        layer: source_layer,
                        dependency: Some(dependency.name.clone()),
                        message: format!(
                            "domain package {} directly references framework crate {}",
                            package.name, dependency.name
                        ),
                    });
                }
            }
        }
        violations.sort_by(|left, right| {
            (&left.rule, &left.package, &left.dependency).cmp(&(
                &right.rule,
                &right.package,
                &right.dependency,
            ))
        });
        Ok(ArchitectureReport {
            style: self.policy.style,
            checked_packages: packages.len(),
            violations,
        })
    }

    fn missing_layer_violations(
        &self,
        packages: &[&Package],
        layers: &BTreeMap<&str, Layer>,
    ) -> Vec<Violation> {
        if !self.policy.require_all_layers {
            return Vec::new();
        }
        let present = layers.values().copied().collect::<BTreeSet<_>>();
        [
            Layer::Domain,
            Layer::Application,
            Layer::Adapter,
            Layer::Infrastructure,
        ]
        .into_iter()
        .filter(|layer| !present.contains(layer))
        .map(|layer| Violation {
            rule: "required-layer".to_owned(),
            package: packages.first().map_or_else(
                || "<workspace>".to_owned(),
                |package| package.name.to_string(),
            ),
            layer,
            dependency: None,
            message: format!("workspace does not contain a {layer:?} package"),
        })
        .collect()
    }
}

fn package_layer(package: &Package) -> Layer {
    if let Some(layer) = package
        .metadata
        .get("ddd4r")
        .and_then(|ddd4r| ddd4r.get("layer"))
        .and_then(serde_json::Value::as_str)
    {
        return Layer::parse(layer);
    }
    let manifest = package.manifest_path.as_str().to_ascii_lowercase();
    let tokens = package
        .name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .chain(manifest.split(|character: char| !character.is_ascii_alphanumeric()));
    for token in tokens {
        let layer = Layer::parse(token);
        if layer != Layer::Unclassified {
            return layer;
        }
    }
    Layer::Unclassified
}

const fn layer_dependency_allowed(source: Layer, target: Layer) -> bool {
    match source {
        Layer::Domain => matches!(target, Layer::Domain | Layer::Unclassified),
        Layer::Application => {
            matches!(
                target,
                Layer::Domain | Layer::Application | Layer::Unclassified
            )
        }
        Layer::Adapter => !matches!(target, Layer::Infrastructure),
        Layer::Infrastructure | Layer::Unclassified => true,
    }
}

#[derive(Default)]
struct CrateReferenceVisitor {
    crates: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for CrateReferenceVisitor {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if let syn::UseTree::Path(path) = &item.tree {
            let name = path.ident.to_string();
            if !matches!(
                name.as_str(),
                "crate" | "self" | "super" | "std" | "core" | "alloc"
            ) {
                self.crates.insert(name);
            }
        }
        visit::visit_item_use(self, item);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.leading_colon.is_none()
            && let Some(segment) = path.segments.first()
        {
            let name = segment.ident.to_string();
            if !matches!(
                name.as_str(),
                "crate" | "self" | "super" | "std" | "core" | "alloc"
            ) {
                self.crates.insert(name);
            }
        }
        visit::visit_path(self, path);
    }
}

fn referenced_crates(source_root: &Path) -> Result<BTreeSet<String>, CheckerError> {
    let mut source_files = Vec::new();
    collect_rust_sources(source_root, &mut source_files)?;
    let mut visitor = CrateReferenceVisitor::default();
    for path in source_files {
        let source = fs::read_to_string(&path).map_err(|source| CheckerError::ReadSource {
            path: path.clone(),
            source,
        })?;
        let syntax = syn::parse_file(&source).map_err(|source| CheckerError::ParseSource {
            path: path.clone(),
            source,
        })?;
        visitor.visit_file(&syntax);
    }
    Ok(visitor.crates)
}

fn collect_rust_sources(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), CheckerError> {
    if !root.is_dir() {
        return Ok(());
    }
    let entries = fs::read_dir(root).map_err(|source| CheckerError::ReadSource {
        path: root.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| CheckerError::ReadSource {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
    output.sort();
    Ok(())
}
