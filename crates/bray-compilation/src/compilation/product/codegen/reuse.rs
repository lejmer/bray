use std::hash::Hash;
use std::path::PathBuf;
use std::sync::Arc;

use bray_base::StableDigestHasher;

use bray_codegen::{
    BackendIdentity, CodegenInstanceKey, CodegenOptions, CodegenPartitionPolicy,
    CodegenSpecialization,
};
use bray_ir::{MirExecutableTemplateId, MirUnitKey};
use bray_linker::{LinkInputProvenance, LinkInputSpec};
use bray_native_artifact::{NativeContentDigest, NativeIndexError, NativeUnitKind};
use bray_package_interface::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity,
    PackageImplementationConfiguration, PackageImplementationSpecializationKey,
    PackageNativeArtifactError,
};
use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{NativeSymbolContract, PackageIdentity, ProductKind, SymbolKeyData};
use bray_target::NativeTarget;

use super::super::super::Compilation;
use super::error::NativeProductPlanningError;
use super::link::native_link_input;
use crate::fact::CancellationToken;

/// One authenticated, indivisible package unit selected in place of imported MIR.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::compilation) struct SelectedNativeUnit {
    pub(in crate::compilation) symbol: BinarySymbolName,
    pub(in crate::compilation) units: Arc<[SelectedNativePayload]>,
}

/// An authenticated payload and its native dependencies in the resolved package closure.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::compilation) struct SelectedNativePayload {
    pub(in crate::compilation) package: PackageIdentity,
    pub(in crate::compilation) digest: NativeContentDigest,
    pub(in crate::compilation) kind: NativeUnitKind,
    pub(in crate::compilation) source: SelectedNativePayloadSource,
    pub(in crate::compilation) native_links: Arc<[LinkInputSpec]>,
}

/// Physical source of an already selected native unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::compilation) enum SelectedNativePayloadSource {
    Bytes(Arc<[u8]>),
    File(PathBuf),
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

        let resolver = implementation
            .native_resolver()
            .map_err(native_failure)?
            .expect("selected native binding must have an index");

        let index = resolver.index();

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

        if index.units().iter().any(|unit| {
            matches!(unit.summary(), bray_native_artifact::NativeUnitSummary::Opaque)
                && unit.kind() != NativeUnitKind::OpaqueArchive
        }) {
            // Opaque code can hide runtime and platform requirements not represented
            // in the concrete program plan. Its source template preserves those demands.
            return Ok(None);
        }

        let root = NativeSymbolContract::required_name(
            bray_base::NonEmptySharedStr::try_new(binding.symbol())
                .expect("authenticated native binding must have a valid symbol"),
        );

        // A producer may reference support supplied outside its package index. In that case,
        // compile the imported template until the complete provider set is available.
        let Ok(selection) = resolver.select([root]) else {
            return Ok(None);
        };

        if !selection.units().contains(&NativeContentDigest::new(binding.unit())) {
            return Ok(None);
        }

        let static_library = self.product_kind() == ProductKind::Library;
        let mut units = Vec::with_capacity(selection.units().len());

        for &digest in selection.units() {
            let unit = &index.units()[index.units()
                .binary_search_by_key(&digest, |unit| unit.digest())
                .expect("resolved unit must be in its validated index")];

            if static_library && unit.kind() == NativeUnitKind::OpaqueArchive {
                // Static libraries archive object inputs and cannot retain an input archive.
                return Ok(None);
            }

            if !unit.link_options().is_empty() {
                return Ok(None);
            }

            let bytes = implementation
                .native_unit_bytes(digest.bytes())
                .map_err(native_failure)?
                .expect("authenticated native index must retain its selected unit");

            let provenance = LinkInputProvenance::Package(input.package().clone());

            let native_links = unit.native_links().iter()
                .map(|requirement| native_link_input(requirement, provenance.clone()))
                .collect::<Result<Vec<_>, _>>()?;

            units.push(SelectedNativePayload {
                package: input.package().clone(),
                digest,
                kind: unit.kind(),
                source: SelectedNativePayloadSource::Bytes(bytes),
                native_links: native_links.into(),
            });
        }

        let symbol = BinarySymbolName::try_new(binding.symbol())
            .expect("authenticated native binding must have a valid binary symbol");

        Ok(Some(SelectedNativeUnit {
            symbol,
            units: units.into(),
        }))
    }
}
