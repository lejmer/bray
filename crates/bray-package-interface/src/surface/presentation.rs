use bray_diagnostics::{
    DiagnosticCallablePosition, DiagnosticInterfaceIdentitySurfaceProblem,
    DiagnosticInterfaceRelationship, DiagnosticInterfaceSymbolGraphProblem,
};
use bray_symbols::{
    CallablePosition, ImportedIdentitySurfaceError, diagnostic_symbol_kind,
    diagnostic_symbol_relationship_kind,
};

use super::{PackageInterfaceSurfaceBuildError, SymbolRelationship};

/// Projects a surface-build failure into its exact locale-neutral diagnostic payload.
pub fn diagnostic_surface_problem(
    error: &PackageInterfaceSurfaceBuildError,
) -> DiagnosticInterfaceSymbolGraphProblem {
    match error {
        PackageInterfaceSurfaceBuildError::NonLibraryProduct => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceNonLibraryProduct
        }
        PackageInterfaceSurfaceBuildError::DependencyCountOverflow => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceDependencyCountOverflow
        }
        PackageInterfaceSurfaceBuildError::DuplicateDependencyPackage(package) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceDuplicateDependencyPackage(
                package.as_str().to_owned(),
            )
        }
        PackageInterfaceSurfaceBuildError::NonCanonicalSymbolOrder { previous, current } => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceNonCanonicalSymbolOrder {
                previous: previous.raw(),
                current: current.raw(),
            }
        }
        PackageInterfaceSurfaceBuildError::Identity(error) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceIdentity(diagnostic_identity_problem(
                *error,
            ))
        }
        PackageInterfaceSurfaceBuildError::RelationshipSymbolOutOfBounds(relationship) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceRelationshipSymbolOutOfBounds(
                diagnostic_relationship(*relationship),
            )
        }
        PackageInterfaceSurfaceBuildError::InvalidRelationship(relationship) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceInvalidRelationship(
                diagnostic_relationship(*relationship),
            )
        }
        PackageInterfaceSurfaceBuildError::DuplicateRelationshipPosition(relationship) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceDuplicateRelationshipPosition(
                diagnostic_relationship(*relationship),
            )
        }
        PackageInterfaceSurfaceBuildError::ExportOwnerOutOfBounds(owner) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceExportOwnerOutOfBounds(owner.raw())
        }
        PackageInterfaceSurfaceBuildError::InvalidExportOwner(owner) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceInvalidExportOwner(owner.raw())
        }
        PackageInterfaceSurfaceBuildError::ExportTargetOutOfBounds(target) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceExportTargetOutOfBounds(target.raw())
        }
        PackageInterfaceSurfaceBuildError::DependencyOutOfBounds(dependency) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceDependencyOutOfBounds(dependency.raw())
        }
        PackageInterfaceSurfaceBuildError::DependencyKeyPackageMismatch(dependency) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceDependencyKeyPackageMismatch(
                dependency.raw(),
            )
        }
        PackageInterfaceSurfaceBuildError::InvalidDirectExportTarget(target) => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceInvalidDirectExportTarget(target.raw())
        }
        PackageInterfaceSurfaceBuildError::DuplicateExportName { owner, name } => {
            DiagnosticInterfaceSymbolGraphProblem::SurfaceDuplicateExportName {
                owner: owner.raw(),
                name: name.as_str().to_owned(),
            }
        }
    }
}

const fn diagnostic_identity_problem(
    error: ImportedIdentitySurfaceError,
) -> DiagnosticInterfaceIdentitySurfaceProblem {
    match error {
        ImportedIdentitySurfaceError::Empty => DiagnosticInterfaceIdentitySurfaceProblem::Empty,
        ImportedIdentitySurfaceError::SymbolCountOverflow => {
            DiagnosticInterfaceIdentitySurfaceProblem::SymbolCountOverflow
        }
        ImportedIdentitySurfaceError::NonCanonicalSymbolId { expected, actual } => {
            DiagnosticInterfaceIdentitySurfaceProblem::NonCanonicalSymbolId {
                expected: expected.raw(),
                actual: actual.raw(),
            }
        }
        ImportedIdentitySurfaceError::MissingPackageRoot { actual } => {
            DiagnosticInterfaceIdentitySurfaceProblem::MissingPackageRoot {
                actual: diagnostic_symbol_kind(actual),
            }
        }
        ImportedIdentitySurfaceError::PackageRootHasContainer { container } => {
            DiagnosticInterfaceIdentitySurfaceProblem::PackageRootHasContainer {
                container: container.raw(),
            }
        }
        ImportedIdentitySurfaceError::PackageIdentityMismatch { symbol } => {
            DiagnosticInterfaceIdentitySurfaceProblem::PackageIdentityMismatch {
                symbol: symbol.raw(),
            }
        }
        ImportedIdentitySurfaceError::SymbolKindMismatch {
            symbol,
            declared,
            keyed,
        } => DiagnosticInterfaceIdentitySurfaceProblem::SymbolKindMismatch {
            symbol: symbol.raw(),
            declared: diagnostic_symbol_kind(declared),
            keyed: diagnostic_symbol_kind(keyed),
        },
        ImportedIdentitySurfaceError::DuplicateExternalKey { first, duplicate } => {
            DiagnosticInterfaceIdentitySurfaceProblem::DuplicateExternalKey {
                first: first.raw(),
                duplicate: duplicate.raw(),
            }
        }
        ImportedIdentitySurfaceError::MissingContainer { symbol } => {
            DiagnosticInterfaceIdentitySurfaceProblem::MissingContainer {
                symbol: symbol.raw(),
            }
        }
        ImportedIdentitySurfaceError::InvalidContainer { symbol, container } => {
            DiagnosticInterfaceIdentitySurfaceProblem::InvalidContainer {
                symbol: symbol.raw(),
                container: container.raw(),
            }
        }
        ImportedIdentitySurfaceError::ContainerKeyMismatch { symbol, container } => {
            DiagnosticInterfaceIdentitySurfaceProblem::ContainerKeyMismatch {
                symbol: symbol.raw(),
                container: container.raw(),
            }
        }
        ImportedIdentitySurfaceError::UnexpectedRoot { symbol, kind } => {
            DiagnosticInterfaceIdentitySurfaceProblem::UnexpectedRoot {
                symbol: symbol.raw(),
                kind: diagnostic_symbol_kind(kind),
            }
        }
    }
}

const fn diagnostic_relationship(
    relationship: SymbolRelationship,
) -> DiagnosticInterfaceRelationship {
    DiagnosticInterfaceRelationship::new(
        diagnostic_symbol_relationship_kind(relationship.kind()),
        relationship.owner().raw(),
        relationship.member().raw(),
        relationship.ordinal(),
        diagnostic_callable_position(relationship.position()),
        relationship.allows_mutation(),
    )
}

const fn diagnostic_callable_position(position: CallablePosition) -> DiagnosticCallablePosition {
    match position {
        CallablePosition::NamedOnly => DiagnosticCallablePosition::NamedOnly,
        CallablePosition::PositionalOrNamed => DiagnosticCallablePosition::PositionalOrNamed,
    }
}
