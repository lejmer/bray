use std::collections::BTreeSet;

use bray_bound_tree::CheckedTemplateKind;
use bray_ir::MirExecutableTemplateId;
use bray_symbols::InterfaceSymbolId;

use crate::implementation::artifact_encoding::encode_artifact;
use crate::implementation::{
    CURRENT_MIR_SCHEMA_REVISION, CURRENT_TEMPLATE_SCHEMA_REVISION, InterfaceConstantCallableBody,
    InterfaceExecutableTemplate, InterfaceNativeBoundary, InterfaceNativeBoundaryKind,
    InterfacePreSpecializedMir, PackageImplementationArtifactBuildError,
    PackageImplementationConfiguration, PackageImplementationIdentity,
    invalid_executable_template_family,
};
use crate::{
    InterfaceArtifact, InterfaceCheckedTemplate, InterfaceSemantics, InterfaceValidationLimits,
    InterfaceValidationPolicy, PackageInterfaceExportBundle, PackageInterfaceSurface,
    ValidatedPackageInterface,
};

use super::PackageImplementationArtifact;

impl PackageImplementationArtifact {
    /// Encodes the implementation payloads associated with one interface export.
    pub fn try_from_export_bundle(
        interface: &InterfaceArtifact,
        bundle: &PackageInterfaceExportBundle,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, PackageImplementationArtifactBuildError> {
        let validated = ValidatedPackageInterface::try_new(
            interface.shared_bytes(),
            InterfaceValidationPolicy::new(bundle.language_revision()).with_limits(limits),
        )
        .map_err(PackageImplementationArtifactBuildError::InvalidArtifact)?;

        Self::try_new(
            &validated,
            bundle.surface(),
            bundle.semantics(),
            bundle.implementation_configuration().clone(),
            bundle.constant_callable_bodies().iter().cloned(),
            bundle.executable_templates().iter().cloned(),
            bundle.native_boundaries().iter().cloned(),
            [],
            limits,
        )
    }

    /// Validates and encodes implementation payloads for one exact package interface.
    pub fn try_new(
        interface: &ValidatedPackageInterface,
        surface: &PackageInterfaceSurface,
        semantics: &InterfaceSemantics,
        configuration: PackageImplementationConfiguration,
        constant_callable_bodies: impl IntoIterator<Item = InterfaceConstantCallableBody>,
        executable_templates: impl IntoIterator<Item = InterfaceExecutableTemplate>,
        native_boundaries: impl IntoIterator<Item = InterfaceNativeBoundary>,
        pre_specialized_mir: impl IntoIterator<Item = InterfacePreSpecializedMir>,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, PackageImplementationArtifactBuildError> {
        let mut bodies = constant_callable_bodies.into_iter().collect::<Vec<_>>();

        bodies.sort_by_key(InterfaceConstantCallableBody::owner);

        for pair in bodies.windows(2) {
            if pair[0].owner() == pair[1].owner() {
                return Err(
                    PackageImplementationArtifactBuildError::DuplicateCallableBody(pair[0].owner()),
                );
            }
        }

        for body in &bodies {
            validate_body_owner(surface, body.owner(), body.template())?;

            semantics
                .validate_implementation_template(surface, body.template(), limits)
                .map_err(PackageImplementationArtifactBuildError::InvalidBody)?;
        }

        let mut templates = executable_templates.into_iter().collect::<Vec<_>>();

        templates.sort_by_key(|template| (template.owner(), template.identity()));

        for pair in templates.windows(2) {
            if (pair[0].owner(), pair[0].identity()) == (pair[1].owner(), pair[1].identity()) {
                return Err(
                    PackageImplementationArtifactBuildError::DuplicateExecutableTemplate(
                        pair[0].owner(),
                    ),
                );
            }
        }

        for template in &templates {
            validate_executable_owner(surface, template.owner())?;
        }

        validate_executable_template_families(&templates)?;

        let mut boundaries = native_boundaries.into_iter().collect::<Vec<_>>();

        boundaries.sort_by_key(InterfaceNativeBoundary::owner);

        for pair in boundaries.windows(2) {
            if pair[0].owner() == pair[1].owner() {
                return Err(
                    PackageImplementationArtifactBuildError::DuplicateNativeBoundary(
                        pair[0].owner(),
                    ),
                );
            }
        }

        for boundary in &boundaries {
            validate_native_boundary_owner(surface, boundary)?;
        }

        let mut pre_specialized_mir = pre_specialized_mir.into_iter().collect::<Vec<_>>();

        pre_specialized_mir.sort_by_key(|mir| mir.key().cache_identity());

        for pair in pre_specialized_mir.windows(2) {
            if pair[0].key() == pair[1].key() {
                return Err(PackageImplementationArtifactBuildError::DuplicateSpecialization);
            }
        }

        if pre_specialized_mir.iter().any(|mir| {
            mir.key().configuration() != &configuration
                || mir.key().template_schema_revision() != CURRENT_TEMPLATE_SCHEMA_REVISION
                || mir.key().dependencies() != surface.dependencies()
                || mir.mir_schema_revision() != CURRENT_MIR_SCHEMA_REVISION
        }) {
            return Err(PackageImplementationArtifactBuildError::SpecializationIdentityMismatch);
        }

        let identity = implementation_identity(interface, surface, semantics, configuration);

        let bytes = encode_artifact(
            &identity,
            &bodies,
            &templates,
            &boundaries,
            &pre_specialized_mir,
        )?;

        Self::try_from_bytes(bytes, limits)
            .map_err(PackageImplementationArtifactBuildError::InvalidArtifact)
    }
}

pub(super) fn implementation_identity(
    interface: &ValidatedPackageInterface,
    surface: &PackageInterfaceSurface,
    semantics: &InterfaceSemantics,
    configuration: PackageImplementationConfiguration,
) -> PackageImplementationIdentity {
    let runtime_requirements = semantics
        .runtime_requirements()
        .iter()
        .map(crate::InterfaceRuntimeRequirement::requirements)
        .cloned();

    PackageImplementationIdentity::new(
        surface.identity().clone(),
        interface.header().content_hash(),
        interface.header().language_revision(),
        surface.dependencies().iter().cloned(),
        configuration,
        runtime_requirements,
    )
}

fn validate_executable_template_families(
    templates: &[InterfaceExecutableTemplate],
) -> Result<(), PackageImplementationArtifactBuildError> {
    if let Some(owner) = invalid_executable_template_family(templates) {
        return Err(
            PackageImplementationArtifactBuildError::InvalidExecutableTemplateFamily(owner),
        );
    }

    let mut platform_services = BTreeSet::new();

    for template in templates {
        let Some(role) = template.platform_service() else {
            continue;
        };

        if template.identity() != MirExecutableTemplateId::ROOT {
            return Err(
                PackageImplementationArtifactBuildError::InvalidExecutableTemplateFamily(
                    template.owner(),
                ),
            );
        }

        if !platform_services.insert(role) {
            return Err(
                PackageImplementationArtifactBuildError::InvalidExecutableTemplateFamily(
                    template.owner(),
                ),
            );
        }
    }

    Ok(())
}

fn validate_executable_owner(
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
) -> Result<(), PackageImplementationArtifactBuildError> {
    let Some(owner_symbol) = surface.symbols().symbol(owner) else {
        return Err(PackageImplementationArtifactBuildError::InvalidExecutableOwner(owner));
    };

    if !owner_symbol.kind().is_callable()
        && !matches!(
            owner_symbol.kind(),
            bray_symbols::SymbolKind::Static
                | bray_symbols::SymbolKind::CallableParameterDefaultProvider
                | bray_symbols::SymbolKind::StructFieldDefaultProvider
                | bray_symbols::SymbolKind::UnionPayloadDefaultProvider
        )
    {
        return Err(PackageImplementationArtifactBuildError::InvalidExecutableOwner(owner));
    }

    Ok(())
}

fn validate_native_boundary_owner(
    surface: &PackageInterfaceSurface,
    boundary: &InterfaceNativeBoundary,
) -> Result<(), PackageImplementationArtifactBuildError> {
    let owner = boundary.owner();

    let Some(owner_symbol) = surface.symbols().symbol(owner) else {
        return Err(PackageImplementationArtifactBuildError::InvalidNativeBoundaryOwner(owner));
    };

    let expected = match boundary.kind() {
        InterfaceNativeBoundaryKind::Callable => bray_symbols::SymbolKind::Function,
        InterfaceNativeBoundaryKind::Static { .. } => bray_symbols::SymbolKind::Static,
    };

    if owner_symbol.kind() != expected {
        return Err(PackageImplementationArtifactBuildError::InvalidNativeBoundaryOwner(owner));
    }

    Ok(())
}

pub(super) fn validate_body_owner(
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    template: &InterfaceCheckedTemplate,
) -> Result<(), PackageImplementationArtifactBuildError> {
    let Some(owner_symbol) = surface.symbols().symbol(owner) else {
        return Err(PackageImplementationArtifactBuildError::InvalidCallableOwner(owner));
    };

    if !owner_symbol.kind().is_callable()
        || template.kind() != CheckedTemplateKind::ConstantCallableBody
    {
        return Err(PackageImplementationArtifactBuildError::InvalidCallableOwner(owner));
    }

    Ok(())
}
