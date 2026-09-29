use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::CodegenTarget;
use bray_emitter::ProductLinkInputs;
use bray_linker::{
    DeadStripPolicy, DebugLinkPolicy, LinkInputProvenance, LinkInputSpec, LinkModel, LinkPolicy, LinkTarget, LinkedProductKind,
    SectionGarbageCollectionPolicy,
};
use bray_runtime_interface::{ExecutableHostContract, RuntimeArtifactSelection};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding, ProductKind};

use super::super::super::Compilation;
use super::error::{NativeLinkInputPlanningError, NativeProductPlanningError};

struct StandardLibraryLinkSelection {
    inputs: Vec<Result<LinkInputSpec, NativeProductPlanningError>>,
    payloads: Vec<super::reuse::SelectedNativePayload>,
    optimization_modules: u64,
    optimization_bytes: u64,
}

pub(super) fn runtime_platform_services(
    runtime: Option<&RuntimeArtifactSelection>,
) -> BTreeSet<bray_runtime_interface::PlatformServiceRole> {
    runtime
        .into_iter()
        .flat_map(RuntimeArtifactSelection::components)
        .flat_map(bray_runtime_interface::RuntimeArtifactComponentMetadata::platform_services)
        .copied()
        .collect()
}

pub(super) fn runtime_platform_symbols(
    runtime: Option<&RuntimeArtifactSelection>,
) -> Result<BTreeSet<bray_runtime_interface::BinarySymbolName>, NativeProductPlanningError> {
    runtime_platform_services(runtime)
        .into_iter()
        .map(platform_symbol)
        .collect()
}

fn platform_symbol(
    role: bray_runtime_interface::PlatformServiceRole,
) -> Result<bray_runtime_interface::BinarySymbolName, NativeProductPlanningError> {
    bray_runtime_interface::BinarySymbolName::try_new(role.native_symbol())
        .ok_or(NativeProductPlanningError::InvalidSymbolName)
}

impl Compilation {
    pub(super) fn product_link_inputs(
        &self,
        kind: ProductKind,
        host: Option<&ExecutableHostContract>,
        runtime: Option<RuntimeArtifactSelection>,
        mappings: &[bray_codegen::CodegenMappings],
        product_host: Option<&bray_codegen::CodegenProductHostMapping>,
        target: &CodegenTarget,
        configuration: crate::BuildConfiguration,
    ) -> Result<(ProductLinkInputs, Vec<super::reuse::SelectedNativePayload>), NativeProductPlanningError> {
        let product = match kind {
            ProductKind::Library => LinkedProductKind::StaticLibrary,
            ProductKind::Executable | ProductKind::Test => LinkedProductKind::Executable,
        };

        let link_model = product_link_model(target.machine().object_format());

        // Link inputs own the Arc-backed target identity independently of codegen inputs.
        let link_target = LinkTarget::try_new(
            target.identity().clone(),
            target.triple(),
            target.machine().architecture(),
            target.machine().object_format(),
            target.relocation_model(),
            target.code_model(),
            link_model,
        )
        .map_err(NativeProductPlanningError::InvalidLinkTarget)?;

        let linked_debug = match configuration {
            crate::BuildConfiguration::Development
                if configuration
                    .requires_linked_debug_companion(target.machine().object_format()) =>
            {
                DebugLinkPolicy::Companion
            }
            crate::BuildConfiguration::Development => DebugLinkPolicy::Embedded,
            crate::BuildConfiguration::Release
            | crate::BuildConfiguration::ObjectRelease
            | crate::BuildConfiguration::ObservedRelease
            | crate::BuildConfiguration::TimedRelease { .. } => DebugLinkPolicy::None,
        };

        let preserve_unused = configuration.preserves_unused_link_content();

        let policy = match product {
            LinkedProductKind::StaticLibrary => LinkPolicy::new(
                DeadStripPolicy::Preserve,
                SectionGarbageCollectionPolicy::Preserve,
                match configuration {
                    crate::BuildConfiguration::Development => DebugLinkPolicy::Embedded,
                    crate::BuildConfiguration::Release
                    | crate::BuildConfiguration::ObjectRelease
                    | crate::BuildConfiguration::ObservedRelease
                    | crate::BuildConfiguration::TimedRelease { .. } => DebugLinkPolicy::None,
                },
                None,
            ),
            LinkedProductKind::Executable => LinkPolicy::new(
                if preserve_unused {
                    DeadStripPolicy::Preserve
                } else {
                    DeadStripPolicy::RemoveUnreachable
                },
                if preserve_unused {
                    SectionGarbageCollectionPolicy::Preserve
                } else {
                    SectionGarbageCollectionPolicy::RemoveUnreferenced
                },
                linked_debug,
                Some(bray_linker::LinkSubsystem::Console),
            ),
            LinkedProductKind::SharedLibrary => LinkPolicy::new(
                if preserve_unused {
                    DeadStripPolicy::Preserve
                } else {
                    DeadStripPolicy::RemoveUnreachable
                },
                if preserve_unused {
                    SectionGarbageCollectionPolicy::Preserve
                } else {
                    SectionGarbageCollectionPolicy::RemoveUnreferenced
                },
                linked_debug,
                None,
            ),
        };

        let policy = if configuration.uses_thin_lto() && product != LinkedProductKind::StaticLibrary
        {
            policy.with_optimization(bray_linker::LinkTimeOptimizationPolicy::ThinLto {
                jobs: self.worker_budget().nonzero(),
            })
        } else {
            policy
        };

        let configured_inputs = self.native_link_inputs().iter().map(|requirement| {
            native_link_input(requirement, LinkInputProvenance::HostConfiguration)
        });

        let runtime_inputs = runtime.iter().flat_map(|runtime| {
            // Every input retains the Arc-backed runtime artifact provenance.
            let artifact = runtime.contract().artifact().clone();

            runtime.native_links().map(move |requirement| {
                native_link_input(
                    requirement,
                    LinkInputProvenance::RuntimeDependency(artifact.clone()),
                )
            })
        });

        let imported_symbols = mappings
            .iter()
            .flat_map(|mappings| {
                mappings
                    .symbols()
                    .iter()
                    .filter(|symbol| symbol.linkage() == bray_codegen::CodegenLinkage::Import)
                    .map(|symbol| symbol.name().as_str())
                    .chain(
                        mappings
                            .native_storages()
                            .iter()
                            .filter(|storage| {
                                storage.direction()
                                    == bray_symbols::ForeignCallableDirection::Import
                            })
                            .map(|storage| storage.symbol().as_str()),
                    )
            })
            .collect();

        let mut provided_symbols = product_native_definitions(mappings);

        let platform_override_symbols = runtime_platform_symbols(runtime.as_ref())?;

        for symbol in &platform_override_symbols {
            provided_symbols.insert(symbol.as_str(), NativeSymbolBinding::Strong);
        }

        let standard_library = self.standard_library_link_selection(
            kind,
            &imported_symbols,
            &provided_symbols,
            target,
            configuration,
        )?;

        let native_inputs = configured_inputs
            .chain(runtime_inputs)
            .chain(standard_library.inputs)
            .collect::<Result<Vec<_>, _>>()?;

        let mut inputs =
            ProductLinkInputs::new(link_target, policy).with_native_inputs(native_inputs);

        if let Some(runtime) = runtime {
            inputs = inputs.with_runtime(runtime);
        }

        if let Some(host) = host {
            inputs = inputs.with_entry_point(host.native_entry().clone());
        }

        let mut preservation_roots =
            super::plan::product_preservation_roots(mappings, product_host)
                .cloned()
                .collect::<BTreeSet<_>>();

        preservation_roots.extend(platform_override_symbols);

        if let Some(host) = host {
            preservation_roots.insert(host.native_entry().clone());
        }

        if let Some(product_host) = product_host {
            preservation_roots.extend([
                product_host.descriptor_symbol().clone(),
                product_host.control_symbol().clone(),
            ]);
        }

        if configuration.uses_thin_lto() {
            if let Some(profile) = self.state.fact_runtime.profile() {
                profile.record_metric(
                    crate::profile::ProfileMetricKind::OptimizationModules,
                    standard_library.optimization_modules,
                );

                profile.record_metric(
                    crate::profile::ProfileMetricKind::OptimizationInputBytes,
                    standard_library.optimization_bytes,
                );

                profile.record_metric(
                    crate::profile::ProfileMetricKind::OptimizationWorkers,
                    u64::try_from(self.worker_budget().get()).unwrap_or(u64::MAX),
                );

                profile.record_metric(
                    crate::profile::ProfileMetricKind::OptimizationPreservationRoots,
                    u64::try_from(preservation_roots.len()).unwrap_or(u64::MAX),
                );
            }
        }

        inputs = inputs.with_retained_symbols(preservation_roots);

        let native_exports = mappings
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .filter(|mapping| mapping.native_binding().is_some())
            .map(bray_codegen::CodegenStaticStorageMapping::symbol)
            .cloned()
            .collect::<BTreeSet<_>>();

        inputs = inputs.with_exported_symbols(native_exports);

        if let Some(product_host) = product_host {
            if kind == ProductKind::Library {
                inputs = inputs.with_exported_symbols([
                    product_host.descriptor_symbol().clone(),
                    product_host.control_symbol().clone(),
                ]);
            }
        }

        Ok((inputs, standard_library.payloads))
    }

    fn standard_library_link_selection(
        &self,
        product_kind: ProductKind,
        imported_symbols: &BTreeSet<&str>,
        provided_symbols: &BTreeMap<&str, NativeSymbolBinding>,
        target: &CodegenTarget,
        configuration: crate::BuildConfiguration,
    ) -> Result<StandardLibraryLinkSelection, NativeProductPlanningError> {
        if product_kind == ProductKind::Library {
            return Ok(StandardLibraryLinkSelection {
                inputs: Vec::new(),
                payloads: Vec::new(),
                optimization_modules: 0,
                optimization_bytes: 0,
            });
        }

        let Some(resolver) = self.standard_library_provider_resolver() else {
            return Ok(StandardLibraryLinkSelection {
                inputs: Vec::new(),
                payloads: Vec::new(),
                optimization_modules: 0,
                optimization_bytes: 0,
            });
        };

        let selected = self.requested_target();

        let package = bray_symbols::PackageIdentity::try_new(
            bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        )
        .unwrap_or_else(|| panic!("standard library package identity must be valid"));

        let selection = self.select_standard_library_native(
            resolver,
            selected.profile().identity(),
            selected.runtime_abi(),
            imported_symbols,
            provided_symbols,
            target,
            configuration,
            &package,
        )?;

        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.set_standard_library_artifacts(vec![
                bray_profile::CompilationProfileStandardLibraryArtifact {
                    path: selection.index_path.display().to_string(),
                    modules: u32::try_from(selection.modules).unwrap_or(u32::MAX),
                    bytes: selection.bytes,
                },
            ]);
        }

        Ok(StandardLibraryLinkSelection {
            inputs: selection.inputs.into_iter().map(Ok).collect(),
            payloads: selection.payloads,
            optimization_modules: selection.modules,
            optimization_bytes: selection.bytes,
        })
    }

    #[cfg(test)]
    pub(super) fn standard_library_link_inputs(
        &self,
        product_kind: ProductKind,
        imported_symbols: &BTreeSet<&str>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
    ) -> Result<(Vec<LinkInputSpec>, Vec<super::reuse::SelectedNativePayload>), NativeProductPlanningError>
    {
        let target = self
            .requested_target()
            .codegen_target()
            .map_err(NativeProductPlanningError::InvalidCodegenTarget)?;

        let provided = platform_overrides.iter().map(|role| {
            (role.native_symbol(), NativeSymbolBinding::Strong)
        }).collect::<BTreeMap<_, _>>();

        self.standard_library_link_selection(
            product_kind,
            imported_symbols,
            &provided,
            &target,
            crate::BuildConfiguration::Development,
        )
        .and_then(|selection| Ok((
            selection.inputs.into_iter().collect::<Result<_, _>>()?,
            selection.payloads,
        )))
    }
}

pub(super) fn product_native_definitions(
    mappings: &[bray_codegen::CodegenMappings],
) -> BTreeMap<&str, NativeSymbolBinding> {
    let mut definitions = BTreeMap::new();

    for mapping in mappings {
        for symbol in mapping.symbols().iter().filter(|symbol| symbol.defines_in(mapping.unit())) {
            let binding = match symbol.linkage() {
                bray_codegen::CodegenLinkage::External
                | bray_codegen::CodegenLinkage::Export
                | bray_codegen::CodegenLinkage::LinkOnce => NativeSymbolBinding::Strong,
                bray_codegen::CodegenLinkage::Weak
                | bray_codegen::CodegenLinkage::Fallback
                | bray_codegen::CodegenLinkage::Common => NativeSymbolBinding::Weak,
                bray_codegen::CodegenLinkage::Private
                | bray_codegen::CodegenLinkage::Internal
                | bray_codegen::CodegenLinkage::Import => continue,
            };

            // An emitted LinkOnce body satisfies its own references even though its native
            // linkage is weak; selecting the std body would define that same body twice.
            insert_product_definition(&mut definitions, symbol.name().as_str(), binding);
        }

        for storage in mapping.native_storages().iter().filter(|storage| {
            storage.direction() == bray_symbols::ForeignCallableDirection::Export
        }) {
            insert_product_definition(&mut definitions, storage.symbol().as_str(), storage.binding());
        }
    }

    definitions
}

fn insert_product_definition<'a>(
    definitions: &mut BTreeMap<&'a str, NativeSymbolBinding>,
    name: &'a str,
    binding: NativeSymbolBinding,
) {
    let existing = definitions.entry(name).or_insert(binding);

    if binding == NativeSymbolBinding::Strong {
        *existing = binding;
    }
}

pub(super) const fn product_link_model(object_format: bray_target::ObjectFormat) -> LinkModel {
    match object_format {
        bray_target::ObjectFormat::Coff
        | bray_target::ObjectFormat::Elf
        | bray_target::ObjectFormat::MachO => LinkModel::Dynamic,
        bray_target::ObjectFormat::WebAssembly | bray_target::ObjectFormat::Xcoff => {
            LinkModel::Static
        }
    }
}

pub(super) fn native_link_input(
    requirement: &NativeLinkRequirement,
    provenance: LinkInputProvenance,
) -> Result<LinkInputSpec, NativeProductPlanningError> {
    let diagnostic_provenance = provenance.clone();

    let input = match requirement.kind() {
        NativeLinkKind::Dynamic | NativeLinkKind::Static | NativeLinkKind::System => {
            LinkInputSpec::try_native_library(requirement.name(), provenance)
        }
        NativeLinkKind::Framework => LinkInputSpec::try_framework(requirement.name(), provenance),
    };

    input.ok_or_else(|| {
        NativeProductPlanningError::InvalidNativeLinkInput(
            NativeLinkInputPlanningError::InvalidRequirement {
                name: requirement.name().to_owned(),
                kind: requirement.kind(),
                provenance: diagnostic_provenance,
            },
        )
    })
}
