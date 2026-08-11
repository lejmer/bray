use bray_diagnostics::DiagnosticProjectManifestField;

pub(crate) const fn format_english_project_manifest_field(
    field: DiagnosticProjectManifestField,
) -> &'static str {
    match field {
        DiagnosticProjectManifestField::ManifestPath => "manifest path",
        DiagnosticProjectManifestField::WorkspaceFormat => "workspace format",
        DiagnosticProjectManifestField::WorkspaceOutputRoot => "workspace output root",
        DiagnosticProjectManifestField::WorkspaceTargets => "workspace targets",
        DiagnosticProjectManifestField::WorkspacePackages => "workspace packages",
        DiagnosticProjectManifestField::WorkspaceRootPackage => "workspace root package",
        DiagnosticProjectManifestField::WorkspacePackageVersion => "workspace package version",
        DiagnosticProjectManifestField::WorkspacePackagePath => "workspace package path",
        DiagnosticProjectManifestField::PackageIdentity => "package identity",
        DiagnosticProjectManifestField::PackageVersion => "package version",
        DiagnosticProjectManifestField::PackageFeatures => "package features",
        DiagnosticProjectManifestField::PackageSourceRoots => "package source roots",
        DiagnosticProjectManifestField::PackageProducts => "package products",
        DiagnosticProjectManifestField::ProductIdentity => "product identity",
        DiagnosticProjectManifestField::ProductSourceRoots => "product source roots",
        DiagnosticProjectManifestField::ProductTargets => "product targets",
        DiagnosticProjectManifestField::ProductOutputs => "product outputs",
        DiagnosticProjectManifestField::ProductDependencies => "product dependencies",
        DiagnosticProjectManifestField::ProductTestedLibrary => "product tested library",
        DiagnosticProjectManifestField::PackagePlatformServices => "package platform services",
        DiagnosticProjectManifestField::PlatformServiceRole => "platform-service role",
        DiagnosticProjectManifestField::PlatformServiceDeclaration => {
            "platform-service declaration"
        }
        DiagnosticProjectManifestField::DependencyPackage => "dependency package",
        DiagnosticProjectManifestField::DependencyProduct => "dependency product",
        DiagnosticProjectManifestField::DependencyTargetPredicate => "dependency target predicate",
        DiagnosticProjectManifestField::SourceRootName => "source-root name",
        DiagnosticProjectManifestField::SourceRootPath => "source-root path",
        DiagnosticProjectManifestField::TargetName => "target name",
        DiagnosticProjectManifestField::TargetIdentity => "target identity",
        DiagnosticProjectManifestField::TargetPredicate => "target predicate",
    }
}
