use std::hash::Hash;
use std::sync::Arc;

use bray_base::StableDigestHasher;

use bray_codegen::{
    BackendIdentity, CodegenInstanceKey, CodegenOptions, CodegenPartitionPolicy,
    CodegenSpecialization,
};
use bray_ir::{MirExecutableTemplateId, MirUnitKey};
use bray_native_artifact::{NativeContentDigest, NativeIndexError, NativeUnitKind, NativeUnitSummary};
use bray_package_interface::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity,
    PackageImplementationConfiguration, PackageImplementationSpecializationKey,
    PackageNativeArtifactError,
};
use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{PackageIdentity, SymbolKeyData};
use bray_target::NativeTarget;

use super::super::super::Compilation;
use super::error::NativeProductPlanningError;
use crate::fact::CancellationToken;

/// One authenticated, indivisible package unit selected in place of imported MIR.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::compilation) struct SelectedNativeUnit {
    pub(in crate::compilation) package: PackageIdentity,
    pub(in crate::compilation) digest: NativeContentDigest,
    pub(in crate::compilation) kind: NativeUnitKind,
    pub(in crate::compilation) symbol: BinarySymbolName,
    pub(in crate::compilation) bytes: Arc<[u8]>,
}

/// Identifies the exact backend, partition policy, and configuration of published units.
pub(in crate::compilation) fn native_producer_identity(
    backend: &BackendIdentity,
    options: &CodegenOptions,
    configuration: &PackageImplementationConfiguration,
    target: NativeTarget,
) -> NativeContentDigest {
    let mut hasher = StableDigestHasher::new();

    "bray native publication v1".hash(&mut hasher);
    backend.hash(&mut hasher);
    options.hash(&mut hasher);
    CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION.hash(&mut hasher);
    configuration.hash(&mut hasher);
    target.hash(&mut hasher);

    NativeContentDigest::new(hasher.finalize())
}

impl Compilation {
    pub(super) fn selected_imported_native_unit(
        &self,
        key: &CodegenInstanceKey,
        options: CodegenOptions,
        cancellation: &CancellationToken,
    ) -> Result<Option<SelectedNativeUnit>, NativeProductPlanningError> {
        let MirUnitKey::ImportedExecutable(imported) = key.template() else {
            return Ok(None);
        };

        if imported.template() != MirExecutableTemplateId::ROOT
            || imported.platform_service().is_some()
            || key.specialization() != &CodegenSpecialization::NonGeneric
            || !key.witnesses().is_empty()
            || key.contextual_self_witness().is_some()
        {
            return Ok(None);
        }

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(address) = skeleton
            .value()
            .as_ref()
            .and_then(|skeleton| skeleton.imported_semantic_address(imported.owner()))
        else {
            return Ok(None);
        };

        let loaded = self
            .loaded_dependency_implementation_with_cancellation(address.interface(), cancellation)?
            .expect("imported executable must have an implementation input");

        let Some(implementation) = loaded.value() else {
            return Ok(None);
        };

        // The cached implementation error is borrowed; planning retains its exact cause.
        let implementation = implementation.as_ref().map_err(|error| {
            NativeProductPlanningError::from(crate::fact::FactQueryError::from(error.clone()))
        })?;

        let input = self
            .dependency_interface_input(address.interface())
            .expect("imported executable must have a dependency input");

        let validation = |error| {
            NativeProductPlanningError::Codegen(
                crate::compilation::CodegenPreparationError::Diagnostics(
                    crate::compilation::imported::implementation_validation_diagnostics(
                        error, input,
                    ),
                ),
            )
        };

        if implementation
            .native_boundary(address.symbol())
            .map_err(validation)?
            .is_some()
        {
            return Ok(None);
        }

        let binding_context = self.binding_context(cancellation)?;
        let portable = self.portable_codegen_symbol_key(&binding_context, imported.owner())?;

        let SymbolKeyData::External(external) = portable.data() else {
            panic!(
                "imported executable {:?} must have an external declaration key",
                key
            );
        };

        let specialization = PackageImplementationSpecializationKey::new(
            ImplementationExternalSymbolIdentity::new(external),
            [],
            [],
            // The specialization key owns the immutable package configuration.
            implementation.identity().configuration().clone(),
            CURRENT_TEMPLATE_SCHEMA_REVISION,
            implementation.identity().dependencies().iter().cloned(),
        );

        let Some(binding) = implementation
            .native_binding(address.symbol(), &specialization, options)
            .map_err(validation)?
        else {
            return Ok(None);
        };

        let native_failure = |error| {
            NativeProductPlanningError::Codegen(
                crate::compilation::CodegenPreparationError::Diagnostics(
                    crate::compilation::imported::native_artifact_diagnostics(error, input),
                ),
            )
        };

        let index = implementation
            .native_artifact()
            .map_err(native_failure)?
            .expect("selected native binding must have an index");

        if let Some(codegen) = &self.state.codegen {
            let expected = native_producer_identity(
                codegen.selected(),
                &options,
                implementation.identity().configuration(),
                index.target(),
            );

            if index.producer() != expected {
                return Err(native_failure(PackageNativeArtifactError::Index(
                    NativeIndexError::WrongProducer {
                        expected,
                        actual: index.producer(),
                    },
                )));
            }
        }

        let digest = NativeContentDigest::new(binding.unit());

        let unit = index
            .units()
            .iter()
            .find(|unit| unit.digest() == digest)
            .expect("authenticated binding must name an indexed unit");

        let NativeUnitSummary::Exact {
            definitions,
            references,
            roots,
        } = unit.summary()
        else {
            panic!("authenticated binding must name an exact native unit");
        };

        // Dependency closure and native policy selection are handled by BRA-584. Until then,
        // only an isolated definition can replace the consumer's generated instance.
        if definitions.len() != 1
            || !references.is_empty()
            || !roots.is_empty()
            || !unit.native_links().is_empty()
            || !unit.link_options().is_empty()
            || index
                .co_retention_groups()
                .iter()
                .any(|group| group.members().contains(&digest))
            || unit.kind() == NativeUnitKind::OpaqueArchive
        {
            return Ok(None);
        }

        let bytes = implementation
            .native_unit_bytes(binding.unit())
            .map_err(validation)?
            .expect("authenticated native index must retain its selected unit");

        let symbol = BinarySymbolName::try_new(binding.symbol())
            .expect("authenticated native binding must have a valid binary symbol");

        Ok(Some(SelectedNativeUnit {
            // The native plan owns package provenance after dependency lookup ends.
            package: input.package().clone(),
            digest,
            kind: unit.kind(),
            symbol,
            bytes,
        }))
    }
}
