use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceManifest {
    pub format: u32,
    #[serde(default)]
    pub package: Option<WorkspacePackageMetadataManifest>,
    pub output_root: String,
    pub targets: Vec<TargetManifest>,
    pub packages: Vec<WorkspacePackageManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspacePackageMetadataManifest {
    pub version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TargetManifest {
    pub name: String,
    pub identity: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspacePackageManifest {
    pub path: String,
    pub role: PackageRoleManifest,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PackageRoleManifest {
    Root,
    Vendored,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageManifest {
    pub format: u32,
    pub identity: String,
    pub version: PackageVersionManifest,
    #[serde(default)]
    pub features: Vec<String>,
    pub source_roots: Vec<SourceRootManifest>,
    #[serde(default)]
    pub dependencies: Vec<DependencyManifest>,
    pub products: Vec<ProductManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PackageVersionManifest {
    Explicit(String),
    Inherited(WorkspacePackageVersionManifest),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspacePackageVersionManifest {
    pub workspace: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceRootManifest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DependencyManifest {
    pub package: String,
    pub product: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProductManifest {
    pub name: String,
    pub kind: ProductKindManifest,
    pub source_roots: Vec<String>,
    pub targets: Vec<String>,
    pub outputs: Vec<OutputKindManifest>,
    #[serde(default)]
    pub platform_services: Vec<PlatformServiceManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlatformServiceManifest {
    pub role: String,
    pub declaration: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProductKindManifest {
    Executable,
    Library,
    Test,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OutputKindManifest {
    Assembly,
    BackendIr,
    BackendBitcode,
    RelocatableObject,
    ExecutableModule,
    DebugCompanion,
    PackageInterface,
    DependencyMetadata,
    Executable,
    StaticLibrary,
    SharedLibrary,
    LinkedCompanion,
}
