use std::io::ErrorKind;
use std::path::PathBuf;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticDocumentParseKind, DiagnosticId, DiagnosticIoErrorKind,
    DiagnosticKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticProjectManifestField,
    SeverityKind,
};
use bray_symbols::{PackageIdentity, ProductIdentity};
use bray_target::{TargetFactKind, TargetIdentity};

use crate::TargetPredicateValueKind;

/// Exact validation failure in a syntactically valid project manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectManifestProblem {
    violation: ProjectManifestViolation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProjectManifestViolation {
    UnsupportedFormat { field: DiagnosticProjectManifestField, actual: u64, supported: u64 },
    InvalidPath { field: DiagnosticProjectManifestField, path: PathBuf },
    InvalidName { field: DiagnosticProjectManifestField, value: String },
    InvalidPackageVersion { field: DiagnosticProjectManifestField, value: String },
    MissingWorkspacePackageVersion { field: DiagnosticProjectManifestField },
    ReservedPackageIdentity { field: DiagnosticProjectManifestField, identity: String },
    StandardLibraryPackageIdentityRequired { field: DiagnosticProjectManifestField, identity: String },
    StandardLibraryRootPackageRequired { field: DiagnosticProjectManifestField, identity: String },
    MissingSelection { field: DiagnosticProjectManifestField },
    DuplicateSelection { field: DiagnosticProjectManifestField, value: String },
    MissingRootPackage { field: DiagnosticProjectManifestField },
    UndeclaredFeature { field: DiagnosticProjectManifestField, feature: String },
    UnknownSourceRoot { field: DiagnosticProjectManifestField, root: String },
    UnknownTarget { field: DiagnosticProjectManifestField, target: String },
    UnknownTargetPredicateProperty {
        field: DiagnosticProjectManifestField,
        property: String,
    },
    TargetPredicateValueKindMismatch {
        field: DiagnosticProjectManifestField,
        property: TargetFactKind,
        expected: TargetPredicateValueKind,
        actual: TargetPredicateValueKind,
    },
    UnknownDependencyPackage { field: DiagnosticProjectManifestField, package: PackageIdentity },
    UnknownDependencyProduct { field: DiagnosticProjectManifestField, product: ProductIdentity },
    DependencyProductNotLibrary { field: DiagnosticProjectManifestField, product: ProductIdentity },
    DependencyTargetUnavailable { field: DiagnosticProjectManifestField, product: ProductIdentity, target: TargetIdentity },
    TestedLibraryOnNonTestProduct { field: DiagnosticProjectManifestField, product: String },
    InvalidSourceRoot { field: DiagnosticProjectManifestField, path: PathBuf },
    SourceSymlink { field: DiagnosticProjectManifestField, path: PathBuf },
    NonUtf8SourcePath { field: DiagnosticProjectManifestField, path: PathBuf },
    DependencyCyclePackage { field: DiagnosticProjectManifestField, package: PackageIdentity },
    DependencyCycleProduct { field: DiagnosticProjectManifestField, product: ProductIdentity },
}

/// Stable category of an exact project-manifest validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectManifestProblemKind {
    /// The manifest revision is not supported.
    UnsupportedFormat,
    /// A manifest path violates the portable path contract.
    InvalidPath,
    /// A manifest name violates its field's grammar.
    InvalidName,
    /// A package version is not a valid semantic version.
    InvalidPackageVersion,
    /// A package inherits a version absent from its workspace.
    MissingWorkspacePackageVersion,
    /// An ordinary package claims the standard-library namespace.
    ReservedPackageIdentity,
    /// A standard-library package is outside its required namespace.
    StandardLibraryPackageIdentityRequired,
    /// A standard-library workspace selects the wrong root package.
    StandardLibraryRootPackageRequired,
    /// A required manifest selection is absent.
    MissingSelection,
    /// A canonical manifest selection occurs more than once.
    DuplicateSelection,
    /// A workspace root package is absent.
    MissingRootPackage,
    /// A product selects a feature its package does not declare.
    UndeclaredFeature,
    /// A product selects a source root its package does not declare.
    UnknownSourceRoot,
    /// A product selects a target its workspace does not declare.
    UnknownTarget,
    /// A target predicate names a property outside the language-defined target profile.
    UnknownTargetPredicateProperty,
    /// A target predicate supplies a literal category its property does not accept.
    TargetPredicateValueKindMismatch,
    /// A dependency selects a package absent from the workspace inventory.
    UnknownDependencyPackage,
    /// A dependency selects a product absent from its package.
    UnknownDependencyProduct,
    /// A dependency selects a non-library product.
    DependencyProductNotLibrary,
    /// A dependency product does not support one selected target.
    DependencyTargetUnavailable,
    /// A non-test product selects a sibling library under test.
    TestedLibraryOnNonTestProduct,
    /// A declared source root cannot be read as a project source tree.
    InvalidSourceRoot,
    /// A project source tree contains a symbolic link.
    SourceSymlink,
    /// A project source tree contains a non-UTF-8 path.
    NonUtf8SourcePath,
    /// The project dependency graph contains a cycle.
    DependencyCycle,
}

impl ProjectManifestProblem {
    /// Returns the exact stable category of this validation failure.
    pub const fn kind(&self) -> ProjectManifestProblemKind {
        match self.violation {
            ProjectManifestViolation::UnsupportedFormat { .. } => {
                ProjectManifestProblemKind::UnsupportedFormat
            }
            ProjectManifestViolation::InvalidPath { .. } => ProjectManifestProblemKind::InvalidPath,
            ProjectManifestViolation::InvalidName { .. } => ProjectManifestProblemKind::InvalidName,
            ProjectManifestViolation::InvalidPackageVersion { .. } => {
                ProjectManifestProblemKind::InvalidPackageVersion
            }
            ProjectManifestViolation::MissingWorkspacePackageVersion { .. } => {
                ProjectManifestProblemKind::MissingWorkspacePackageVersion
            }
            ProjectManifestViolation::ReservedPackageIdentity { .. } => {
                ProjectManifestProblemKind::ReservedPackageIdentity
            }
            ProjectManifestViolation::StandardLibraryPackageIdentityRequired { .. } => {
                ProjectManifestProblemKind::StandardLibraryPackageIdentityRequired
            }
            ProjectManifestViolation::StandardLibraryRootPackageRequired { .. } => {
                ProjectManifestProblemKind::StandardLibraryRootPackageRequired
            }
            ProjectManifestViolation::MissingSelection { .. } => {
                ProjectManifestProblemKind::MissingSelection
            }
            ProjectManifestViolation::DuplicateSelection { .. } => {
                ProjectManifestProblemKind::DuplicateSelection
            }
            ProjectManifestViolation::MissingRootPackage { .. } => {
                ProjectManifestProblemKind::MissingRootPackage
            }
            ProjectManifestViolation::UndeclaredFeature { .. } => {
                ProjectManifestProblemKind::UndeclaredFeature
            }
            ProjectManifestViolation::UnknownSourceRoot { .. } => {
                ProjectManifestProblemKind::UnknownSourceRoot
            }
            ProjectManifestViolation::UnknownTarget { .. } => {
                ProjectManifestProblemKind::UnknownTarget
            }
            ProjectManifestViolation::UnknownTargetPredicateProperty { .. } => {
                ProjectManifestProblemKind::UnknownTargetPredicateProperty
            }
            ProjectManifestViolation::TargetPredicateValueKindMismatch { .. } => {
                ProjectManifestProblemKind::TargetPredicateValueKindMismatch
            }
            ProjectManifestViolation::UnknownDependencyPackage { .. } => {
                ProjectManifestProblemKind::UnknownDependencyPackage
            }
            ProjectManifestViolation::UnknownDependencyProduct { .. } => {
                ProjectManifestProblemKind::UnknownDependencyProduct
            }
            ProjectManifestViolation::DependencyProductNotLibrary { .. } => {
                ProjectManifestProblemKind::DependencyProductNotLibrary
            }
            ProjectManifestViolation::DependencyTargetUnavailable { .. } => {
                ProjectManifestProblemKind::DependencyTargetUnavailable
            }
            ProjectManifestViolation::TestedLibraryOnNonTestProduct { .. } => {
                ProjectManifestProblemKind::TestedLibraryOnNonTestProduct
            }
            ProjectManifestViolation::InvalidSourceRoot { .. } => {
                ProjectManifestProblemKind::InvalidSourceRoot
            }
            ProjectManifestViolation::SourceSymlink { .. } => {
                ProjectManifestProblemKind::SourceSymlink
            }
            ProjectManifestViolation::NonUtf8SourcePath { .. } => {
                ProjectManifestProblemKind::NonUtf8SourcePath
            }
            ProjectManifestViolation::DependencyCyclePackage { .. }
            | ProjectManifestViolation::DependencyCycleProduct { .. } => {
                ProjectManifestProblemKind::DependencyCycle
            }
        }
    }
}

/// A structured project graph loading failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectLoadError {
    /// A required workspace or package manifest could not be read.
    ReadManifest {
        /// Manifest path.
        path: PathBuf,
        /// Structured host I/O category.
        kind: ErrorKind,
    },
    /// A required manifest is not valid UTF-8 JSON matching the Bray schema.
    ParseManifest {
        /// Manifest path.
        path: PathBuf,
        /// Stable parser failure category.
        kind: DiagnosticDocumentParseKind,
        /// One-based line reported by the parser, when applicable.
        line: Option<u64>,
        /// One-based column reported by the parser, when applicable.
        column: Option<u64>,
    },
    /// A manifest value violates a Bray project invariant.
    InvalidManifest {
        /// Manifest path.
        path: PathBuf,
        /// Exact locale-neutral validation category and payload.
        problem: ProjectManifestProblem,
    },
}

impl ProjectLoadError {
    fn manifest_problem(path: PathBuf, violation: ProjectManifestViolation) -> Self {
        Self::InvalidManifest {
            path,
            problem: ProjectManifestProblem { violation },
        }
    }

    pub(crate) fn unsupported_format(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        actual: u64,
        supported: u64,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::UnsupportedFormat {
                field,
                actual,
                supported,
            },
        )
    }

    pub(crate) fn invalid_path(
        manifest: PathBuf,
        field: DiagnosticProjectManifestField,
        path: PathBuf,
    ) -> Self {
        Self::manifest_problem(manifest, ProjectManifestViolation::InvalidPath { field, path })
    }

    pub(crate) fn invalid_name(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        value: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::InvalidName {
                field,
                value: value.into(),
            },
        )
    }

    pub(crate) fn invalid_package_version(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        value: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::InvalidPackageVersion {
                field,
                value: value.into(),
            },
        )
    }

    pub(crate) fn missing_workspace_package_version(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::MissingWorkspacePackageVersion { field },
        )
    }

    pub(crate) fn reserved_package_identity(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        identity: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::ReservedPackageIdentity {
                field,
                identity: identity.into(),
            },
        )
    }

    pub(crate) fn standard_library_package_identity_required(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        identity: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::StandardLibraryPackageIdentityRequired {
                field,
                identity: identity.into(),
            },
        )
    }

    pub(crate) fn standard_library_root_package_required(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        identity: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::StandardLibraryRootPackageRequired {
                field,
                identity: identity.into(),
            },
        )
    }

    pub(crate) fn missing_selection(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
    ) -> Self {
        Self::manifest_problem(path, ProjectManifestViolation::MissingSelection { field })
    }

    pub(crate) fn duplicate_selection(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        value: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::DuplicateSelection {
                field,
                value: value.into(),
            },
        )
    }

    pub(crate) fn missing_root_package(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
    ) -> Self {
        Self::manifest_problem(path, ProjectManifestViolation::MissingRootPackage { field })
    }

    pub(crate) fn undeclared_feature(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        feature: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::UndeclaredFeature {
                field,
                feature: feature.into(),
            },
        )
    }

    pub(crate) fn unknown_source_root(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        root: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::UnknownSourceRoot {
                field,
                root: root.into(),
            },
        )
    }

    pub(crate) fn unknown_target(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        target: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::UnknownTarget {
                field,
                target: target.into(),
            },
        )
    }

    pub(crate) fn unknown_target_predicate_property(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        property: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::UnknownTargetPredicateProperty {
                field,
                property: property.into(),
            },
        )
    }

    pub(crate) fn target_predicate_value_kind_mismatch(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        property: TargetFactKind,
        expected: TargetPredicateValueKind,
        actual: TargetPredicateValueKind,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::TargetPredicateValueKindMismatch {
                field,
                property,
                expected,
                actual,
            },
        )
    }

    pub(crate) fn unknown_dependency_package(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        package: PackageIdentity,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::UnknownDependencyPackage { field, package },
        )
    }

    pub(crate) fn unknown_dependency_product(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        product: ProductIdentity,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::UnknownDependencyProduct { field, product },
        )
    }

    pub(crate) fn dependency_product_not_library(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        product: ProductIdentity,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::DependencyProductNotLibrary { field, product },
        )
    }

    pub(crate) fn dependency_target_unavailable(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        product: ProductIdentity,
        target: TargetIdentity,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::DependencyTargetUnavailable {
                field,
                product,
                target,
            },
        )
    }

    pub(crate) fn tested_library_on_non_test_product(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        product: impl Into<String>,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::TestedLibraryOnNonTestProduct {
                field,
                product: product.into(),
            },
        )
    }

    pub(crate) fn invalid_source_root(
        manifest: PathBuf,
        field: DiagnosticProjectManifestField,
        path: PathBuf,
    ) -> Self {
        Self::manifest_problem(
            manifest,
            ProjectManifestViolation::InvalidSourceRoot { field, path },
        )
    }

    pub(crate) fn source_symlink(
        manifest: PathBuf,
        field: DiagnosticProjectManifestField,
        path: PathBuf,
    ) -> Self {
        Self::manifest_problem(
            manifest,
            ProjectManifestViolation::SourceSymlink { field, path },
        )
    }

    pub(crate) fn non_utf8_source_path(
        manifest: PathBuf,
        field: DiagnosticProjectManifestField,
        path: PathBuf,
    ) -> Self {
        Self::manifest_problem(
            manifest,
            ProjectManifestViolation::NonUtf8SourcePath { field, path },
        )
    }

    pub(crate) fn dependency_cycle_package(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        package: PackageIdentity,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::DependencyCyclePackage { field, package },
        )
    }

    pub(crate) fn dependency_cycle_product(
        path: PathBuf,
        field: DiagnosticProjectManifestField,
        product: ProductIdentity,
    ) -> Self {
        Self::manifest_problem(
            path,
            ProjectManifestViolation::DependencyCycleProduct { field, product },
        )
    }

    /// Converts this project-boundary failure into a locale-neutral diagnostic.
    pub fn into_diagnostic(self, id: DiagnosticId) -> Diagnostic {
        match self {
            Self::ReadManifest { path, kind } => Diagnostic::new(
                id,
                DiagnosticKind::ProjectManifestReadFailed,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::file_path(path))
            .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                kind,
            ))),
            Self::ParseManifest {
                path,
                kind,
                line,
                column,
            } => Diagnostic::new(
                id,
                DiagnosticKind::ProjectManifestParseFailed,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::file_path(path))
            .with_arg(DiagnosticArg::document_parse_kind(kind))
            .with_optional_arg(line.map(DiagnosticArg::document_line))
            .with_optional_arg(column.map(DiagnosticArg::document_column)),
            Self::InvalidManifest { path, problem } => {
                invalid_manifest_diagnostic(id, path, problem)
            }
        }
    }
}

fn invalid_manifest_diagnostic(
    id: DiagnosticId,
    path: PathBuf,
    problem: ProjectManifestProblem,
) -> Diagnostic {
    let base = |kind, field| {
        Diagnostic::new(id, kind, SeverityKind::Error)
            .with_arg(DiagnosticArg::file_path(path.clone()))
            .with_arg(DiagnosticArg::project_manifest_field(field))
    };

    match problem.violation {
        ProjectManifestViolation::UnsupportedFormat {
            field,
            actual,
            supported,
        } => base(DiagnosticKind::ProjectManifestUnsupportedFormat, field)
            .with_arg(DiagnosticArg::actual_revision(actual))
            .with_arg(DiagnosticArg::expected_revision(supported)),
        ProjectManifestViolation::InvalidPath { field, path } => {
            base(DiagnosticKind::ProjectManifestInvalidPath, field)
                .with_arg(DiagnosticArg::project_path(path))
        }
        ProjectManifestViolation::InvalidName { field, value } => {
            base(DiagnosticKind::ProjectManifestInvalidName, field)
                .with_arg(DiagnosticArg::referenced_name(value))
        }
        ProjectManifestViolation::InvalidPackageVersion { field, value } => {
            base(DiagnosticKind::ProjectPackageVersionInvalid, field)
                .with_arg(DiagnosticArg::referenced_name(value))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::PackageVersionMustBeValid,
                ))
        }
        ProjectManifestViolation::MissingWorkspacePackageVersion { field } => {
            base(
                DiagnosticKind::ProjectPackageVersionMissingWorkspace,
                field,
            )
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::PackageVersionMustBeValid,
            ))
        }
        ProjectManifestViolation::ReservedPackageIdentity { field, identity } => {
            base(DiagnosticKind::ProjectPackageIdentityReserved, field)
                .with_arg(DiagnosticArg::actual_package_identity(identity))
        }
        ProjectManifestViolation::StandardLibraryPackageIdentityRequired { field, identity } => {
            base(
                DiagnosticKind::ProjectStandardLibraryPackageIdentityRequired,
                field,
            )
            .with_arg(DiagnosticArg::actual_package_identity(identity))
        }
        ProjectManifestViolation::StandardLibraryRootPackageRequired { field, identity } => {
            base(
                DiagnosticKind::ProjectStandardLibraryRootPackageRequired,
                field,
            )
            .with_arg(DiagnosticArg::actual_package_identity(identity))
        }
        ProjectManifestViolation::MissingSelection { field } => {
            base(DiagnosticKind::ProjectManifestMissingSelection, field)
        }
        ProjectManifestViolation::DuplicateSelection { field, value } => {
            base(DiagnosticKind::ProjectManifestDuplicateSelection, field)
                .with_arg(DiagnosticArg::referenced_name(value))
        }
        ProjectManifestViolation::MissingRootPackage { field } => {
            base(DiagnosticKind::ProjectManifestMissingRootPackage, field)
        }
        ProjectManifestViolation::UndeclaredFeature { field, feature } => {
            base(DiagnosticKind::ProjectManifestUndeclaredFeature, field)
                .with_arg(DiagnosticArg::referenced_name(feature))
        }
        ProjectManifestViolation::UnknownSourceRoot { field, root } => {
            base(DiagnosticKind::ProjectManifestUnknownSourceRoot, field)
                .with_arg(DiagnosticArg::referenced_name(root))
        }
        ProjectManifestViolation::UnknownTarget { field, target } => {
            base(DiagnosticKind::ProjectManifestUnknownTarget, field)
                .with_arg(DiagnosticArg::target_triple(target))
        }
        ProjectManifestViolation::UnknownTargetPredicateProperty { field, property } => {
            base(
                DiagnosticKind::ProjectManifestUnknownTargetPredicateProperty,
                field,
            )
                .with_arg(DiagnosticArg::referenced_name(property))
        }
        ProjectManifestViolation::TargetPredicateValueKindMismatch {
            field,
            property,
            expected,
            actual,
        } => base(
            DiagnosticKind::ProjectManifestTargetPredicateValueKindMismatch,
            field,
        )
        .with_arg(DiagnosticArg::referenced_name(property.as_str()))
        .with_arg(DiagnosticArg::expected_target_predicate_value_kind(
            expected.into(),
        ))
        .with_arg(DiagnosticArg::actual_target_predicate_value_kind(
            actual.into(),
        )),
        ProjectManifestViolation::UnknownDependencyPackage { field, package } => {
            base(DiagnosticKind::ProjectDependencyPackageUnknown, field)
                .with_arg(DiagnosticArg::actual_package_identity(package.as_str()))
        }
        ProjectManifestViolation::UnknownDependencyProduct { field, product } => {
            product_diagnostic(base(DiagnosticKind::ProjectDependencyProductUnknown, field), product)
        }
        ProjectManifestViolation::DependencyProductNotLibrary { field, product } => product_diagnostic(
            base(DiagnosticKind::ProjectDependencyProductNotLibrary, field),
            product,
        ),
        ProjectManifestViolation::DependencyTargetUnavailable {
            field,
            product,
            target,
        } => product_diagnostic(
            base(
                DiagnosticKind::ProjectDependencyProductTargetUnavailable,
                field,
            ),
            product,
        )
        .with_arg(DiagnosticArg::target_triple(target.as_str())),
        ProjectManifestViolation::TestedLibraryOnNonTestProduct { field, product } => {
            base(DiagnosticKind::ProjectManifestUnexpectedTestedLibrary, field)
                .with_arg(DiagnosticArg::actual_product_identity(product))
        }
        ProjectManifestViolation::InvalidSourceRoot { field, path } => {
            base(DiagnosticKind::ProjectSourceRootInvalid, field)
                .with_arg(DiagnosticArg::project_path(path))
        }
        ProjectManifestViolation::SourceSymlink { field, path } => {
            base(DiagnosticKind::ProjectSourceRootContainsSymlink, field)
                .with_arg(DiagnosticArg::project_path(path))
        }
        ProjectManifestViolation::NonUtf8SourcePath { field, path } => {
            base(
                DiagnosticKind::ProjectSourceRootContainsNonUtf8Path,
                field,
            )
            .with_arg(DiagnosticArg::project_path(path))
        }
        ProjectManifestViolation::DependencyCyclePackage { field, package } => {
            base(DiagnosticKind::ProjectDependencyCycle, field).with_arg(
                DiagnosticArg::project_dependency_cycle_member(
                    bray_diagnostics::DiagnosticProjectDependencyCycleMember::Package {
                        identity: package.as_str().to_owned(),
                    },
                ),
            )
        }
        ProjectManifestViolation::DependencyCycleProduct { field, product } => {
            base(DiagnosticKind::ProjectDependencyCycle, field).with_arg(
                DiagnosticArg::project_dependency_cycle_member(
                    bray_diagnostics::DiagnosticProjectDependencyCycleMember::Product {
                        package: product.package().as_str().to_owned(),
                        product: product.name().to_owned(),
                    },
                ),
            )
        }
    }
}

fn product_diagnostic(diagnostic: Diagnostic, product: ProductIdentity) -> Diagnostic {
    diagnostic
        .with_arg(DiagnosticArg::actual_package_identity(
            product.package().as_str(),
        ))
        .with_arg(DiagnosticArg::actual_product_identity(product.name()))
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::path::PathBuf;

    use bray_diagnostics::{
        DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticId, DiagnosticKind,
    };

    use super::ProjectLoadError;

    #[test]
    fn project_load_errors_preserve_their_diagnostic_categories() {
        let manifest = PathBuf::from("bray-workspace.json");

        let read = diagnostic_bag(ProjectLoadError::ReadManifest {
            path: manifest.clone(),
            kind: ErrorKind::NotFound,
        });

        bray_testing::assert_goal_state_diagnostic_kind(
            &read,
            DiagnosticKind::ProjectManifestReadFailed,
        );

        let parse = diagnostic_bag(ProjectLoadError::ParseManifest {
            path: manifest.clone(),
            kind: DiagnosticDocumentParseKind::Syntax,
            line: Some(1),
            column: Some(2),
        });

        bray_testing::assert_goal_state_diagnostic_kind(
            &parse,
            DiagnosticKind::ProjectManifestParseFailed,
        );

    }

    fn diagnostic_bag(error: ProjectLoadError) -> DiagnosticBag {
        DiagnosticBag::single(error.into_diagnostic(DiagnosticId::new(0)))
    }
}
