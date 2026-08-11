use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ProjectLoadError;
use bray_diagnostics::DiagnosticProjectManifestField;

pub(crate) const MANIFEST_FORMAT_REVISION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManifestRevision {
    Revision1,
}

impl ManifestRevision {
    const fn from_number(number: u32) -> Option<Self> {
        match number {
            MANIFEST_FORMAT_REVISION => Some(Self::Revision1),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct ManifestRevisionProbe {
    format: u32,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceManifest {
    pub format: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<WorkspacePackageMetadataManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatter_configuration: Option<String>,
    pub output_root: String,
    pub targets: Vec<TargetManifest>,
    pub packages: Vec<WorkspacePackageManifest>,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspacePackageMetadataManifest {
    pub version: String,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TargetManifest {
    pub name: String,
    pub identity: String,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspacePackageManifest {
    pub path: String,
    pub role: PackageRoleManifest,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PackageRoleManifest {
    Root,
    Vendored,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageManifest {
    pub format: u32,
    pub identity: String,
    pub version: PackageVersionManifest,
    #[serde(default)]
    pub features: Vec<String>,
    pub source_roots: Vec<SourceRootManifest>,
    pub products: Vec<ProductManifest>,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub(crate) enum PackageVersionManifest {
    Explicit(String),
    Inherited(WorkspacePackageVersionManifest),
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspacePackageVersionManifest {
    pub workspace: bool,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceRootManifest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DependencyManifest {
    pub package: String,
    pub product: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<TargetPredicateManifest>,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProductManifest {
    pub name: String,
    pub kind: ProductKindManifest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_library: Option<String>,
    pub source_roots: Vec<String>,
    pub targets: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<DependencyManifest>,
    pub outputs: Vec<OutputKindManifest>,
    #[serde(default)]
    pub platform_services: Vec<PlatformServiceManifest>,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlatformServiceManifest {
    pub role: String,
    pub declaration: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProductKindManifest {
    Executable,
    Library,
    Test,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OutputKindManifest {
    Assembly,
    BackendIr,
    BackendBitcode,
    RelocatableObject,
    ExecutableModule,
    DebugCompanion,
    PackageInterface,
    PackageImplementation,
    DependencyMetadata,
    Executable,
    StaticLibrary,
    SharedLibrary,
    LinkedCompanion,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub(crate) enum TargetPredicateValueManifest {
    String(String),
    Usize(u64),
    Boolean(bool),
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub(crate) enum TargetPredicateManifest {
    All(AllPredicateManifest),
    Any(AnyPredicateManifest),
    Not(NotPredicateManifest),
    Equals(EqualsPredicateManifest),
    NotEquals(NotEqualsPredicateManifest),
    In(InPredicateManifest),
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AllPredicateManifest {
    pub all: Vec<TargetPredicateManifest>,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnyPredicateManifest {
    pub any: Vec<TargetPredicateManifest>,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotPredicateManifest {
    pub not: Box<TargetPredicateManifest>,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EqualsPredicateManifest {
    pub property: String,
    pub equals: TargetPredicateValueManifest,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotEqualsPredicateManifest {
    pub property: String,
    pub not_equals: TargetPredicateValueManifest,
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InPredicateManifest {
    pub property: String,
    #[serde(rename = "in")]
    pub values: Vec<TargetPredicateValueManifest>,
}

pub(crate) fn decode_workspace_manifest(
    source: &str,
    path: &Path,
) -> Result<WorkspaceManifest, ProjectLoadError> {
    match manifest_revision(source, path)? {
        ManifestRevision::Revision1 => decode_revision_1(source, path),
    }
}

pub(crate) fn decode_package_manifest(
    source: &str,
    path: &Path,
) -> Result<PackageManifest, ProjectLoadError> {
    match manifest_revision(source, path)? {
        ManifestRevision::Revision1 => decode_revision_1(source, path),
    }
}

/// Decodes and writes one workspace manifest in the canonical supported serialization.
pub fn canonicalize_workspace_manifest(
    source: &str,
    path: &Path,
) -> Result<String, ProjectLoadError> {
    let mut manifest = decode_workspace_manifest(source, path)?;

    manifest.normalize();

    encode_manifest(&manifest, path)
}

/// Decodes and writes one package manifest in the canonical supported serialization.
pub fn canonicalize_package_manifest(
    source: &str,
    path: &Path,
) -> Result<String, ProjectLoadError> {
    let mut manifest = decode_package_manifest(source, path)?;

    manifest.normalize();

    encode_manifest(&manifest, path)
}

fn manifest_revision(source: &str, path: &Path) -> Result<ManifestRevision, ProjectLoadError> {
    let probe = serde_json::from_str::<ManifestRevisionProbe>(source)
        .map_err(|error| parse_manifest_error(path, &error))?;

    ManifestRevision::from_number(probe.format).ok_or_else(|| {
        ProjectLoadError::unsupported_format(
            path.to_path_buf(),
            DiagnosticProjectManifestField::WorkspaceFormat,
            u64::from(probe.format),
            u64::from(MANIFEST_FORMAT_REVISION),
        )
    })
}

fn decode_revision_1<T>(source: &str, path: &Path) -> Result<T, ProjectLoadError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(source).map_err(|error| parse_manifest_error(path, &error))
}

fn encode_manifest<T>(manifest: &T, path: &Path) -> Result<String, ProjectLoadError>
where
    T: Serialize,
{
    serde_json::to_string_pretty(manifest)
        .map(|source| format!("{source}\n"))
        .map_err(|_| ProjectLoadError::ParseManifest {
            path: path.to_path_buf(),
            kind: bray_diagnostics::DiagnosticDocumentParseKind::Serialization,
            line: None,
            column: None,
        })
}

fn parse_manifest_error(path: &Path, error: &serde_json::Error) -> ProjectLoadError {
    let kind = match error.classify() {
        serde_json::error::Category::Io => bray_diagnostics::DiagnosticDocumentParseKind::Input,
        serde_json::error::Category::Syntax => {
            bray_diagnostics::DiagnosticDocumentParseKind::Syntax
        }
        serde_json::error::Category::Data => bray_diagnostics::DiagnosticDocumentParseKind::Schema,
        serde_json::error::Category::Eof => {
            bray_diagnostics::DiagnosticDocumentParseKind::UnexpectedEnd
        }
    };

    ProjectLoadError::ParseManifest {
        path: path.to_path_buf(),
        kind,
        line: u64::try_from(error.line()).ok(),
        column: u64::try_from(error.column()).ok(),
    }
}

impl WorkspaceManifest {
    fn normalize(&mut self) {
        self.targets.sort_unstable();

        for package in &mut self.packages {
            package.features.sort_unstable();
            package.features.dedup();
        }

        self.packages.sort_unstable();
    }
}

impl PackageManifest {
    fn normalize(&mut self) {
        self.features.sort_unstable();
        self.features.dedup();
        self.source_roots.sort_unstable();

        for product in &mut self.products {
            product.normalize();
        }

        self.products.sort_unstable();
    }
}

impl ProductManifest {
    fn normalize(&mut self) {
        self.source_roots.sort_unstable();
        self.source_roots.dedup();
        self.targets.sort_unstable();
        self.targets.dedup();
        self.outputs.sort_unstable();
        self.outputs.dedup();
        self.platform_services.sort_unstable();
        self.platform_services.dedup();

        for dependency in &mut self.dependencies {
            if let Some(predicate) = &mut dependency.when {
                predicate.normalize();
            }
        }

        self.dependencies.sort_unstable();
    }
}

impl TargetPredicateManifest {
    fn normalize(&mut self) {
        match self {
            Self::All(manifest) => normalize_predicates(&mut manifest.all),
            Self::Any(manifest) => normalize_predicates(&mut manifest.any),
            Self::Not(manifest) => manifest.not.normalize(),
            Self::In(manifest) => {
                manifest.values.sort_unstable();
                manifest.values.dedup();
            }
            Self::Equals(_) | Self::NotEquals(_) => {}
        }
    }
}

fn normalize_predicates(predicates: &mut Vec<TargetPredicateManifest>) {
    for predicate in predicates.iter_mut() {
        predicate.normalize();
    }

    predicates.sort_unstable();
    predicates.dedup();
}
