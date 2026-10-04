use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::CodegenTarget;
use bray_emitter::ProductLinkInputs;
use bray_linker::{
    DeadStripPolicy, DebugLinkPolicy, LinkInputProvenance, LinkInputSpec, LinkModel, LinkPolicy,
    LinkTarget, LinkedProductKind, SectionGarbageCollectionPolicy,
};
use bray_runtime_interface::{
    ExecutableHostContract, RuntimeArtifactPlan, RuntimeArtifactSelection,
};
#[cfg(test)]
use bray_symbols::ProductKind;
use bray_symbols::{NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding};

use super::super::super::Compilation;
use super::error::{NativeLinkInputPlanningError, NativeProductPlanningError};

struct NativeLibraryLinkSelection {
    payloads: Vec<super::reuse::SelectedNativePayload>,
    optimization_modules: u64,
    optimization_bytes: u64,
    runtime: Option<RuntimeArtifactSelection>,
}

pub(super) fn runtime_platform_services(
    runtime: Option<&RuntimeArtifactPlan>,
) -> impl Iterator<Item = bray_runtime_interface::PlatformServiceRole> + '_ {
    runtime
        .into_iter()
        .flat_map(RuntimeArtifactPlan::components)
        .flat_map(bray_runtime_interface::RuntimeArtifactComponentMetadata::platform_services)
        .copied()
}

fn runtime_platform_symbols(
    runtime: Option<&RuntimeArtifactPlan>,
) -> BTreeSet<bray_runtime_interface::BinarySymbolName> {
    runtime_platform_services(runtime)
        .map(|role| {
            bray_runtime_interface::BinarySymbolName::try_new(role.native_symbol())
                .expect("platform service catalog must publish valid native symbol names")
        })
        .collect()
}

impl Compilation {
    pub(super) fn product_link_inputs(
        &self,
        product: LinkedProductKind,
        host: Option<&ExecutableHostContract>,
        runtime: Option<RuntimeArtifactPlan>,
        mappings: &[bray_codegen::CodegenMappings],
        product_host: Option<&bray_codegen::CodegenProductHostMapping>,
        target: &CodegenTarget,
        configuration: crate::BuildConfiguration,
    ) -> Result<
        (ProductLinkInputs, Vec<super::reuse::SelectedNativePayload>),
        NativeProductPlanningError,
    > {
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

        let mut imported_symbols: BTreeSet<&str> = mappings
            .iter()
            .flat_map(|mappings| {
                mappings
                    .symbols()
                    .iter()
                    .filter(|symbol| symbol.linkage() == bray_codegen::CodegenLinkage::Import)
                    .filter(|symbol| match symbol.key() {
                        bray_codegen::CodegenSymbolKey::Runtime(reference) => !host
                            .and_then(|host| host.role_binding(reference.role()))
                            .is_some_and(|binding| binding.implementation() == bray_runtime_interface::RuntimeRoleImplementation::CompilerLowering),
                        _ => true,
                    })
                    .map(|symbol| symbol.name().as_str())
                    .chain(
                        mappings
                            .native_storages()
                            .iter()
                            .filter(|storage| {
                                storage.direction() == bray_symbols::ForeignCallableDirection::Import
                                    && storage.presence() == bray_symbols::NativeSymbolPresence::Required
                            })
                            .map(|storage| storage.symbol().as_str()),
                    )
            })
            .collect();

        let mut provided_symbols = product_native_definitions(mappings);

        if let Some(product_host) = product_host {
            provided_symbols.insert(
                Cow::Borrowed(product_host.descriptor_symbol().as_str()),
                NativeSymbolBinding::Strong,
            );
        }

        let platform_override_symbols = runtime_platform_symbols(runtime.as_ref());

        if let Some(product_host) = product_host {
            imported_symbols.extend(
                product_host
                    .statics()
                    .iter()
                    .filter(|entry| {
                        !mappings
                            .iter()
                            .flat_map(bray_codegen::CodegenMappings::static_storages)
                            .any(|storage| storage.host_name() == entry.host_symbol().as_str())
                    })
                    .map(|entry| entry.host_symbol().as_str()),
            );
        }

        let libraries = self.native_library_link_selection(
            product,
            &imported_symbols,
            &provided_symbols,
            target,
            configuration,
            runtime,
            product_host,
        )?;

        self.profile_runtime_selection(libraries.runtime.as_ref());

        let runtime_inputs = libraries.runtime.iter().flat_map(|runtime| {
            // Every input retains the Arc-backed runtime artifact provenance.
            let artifact = runtime.contract().artifact().clone();

            runtime.native_links().map(move |requirement| {
                native_link_input(
                    requirement,
                    LinkInputProvenance::RuntimeDependency(artifact.clone()),
                )
            })
        });

        let native_inputs = configured_inputs
            .chain(runtime_inputs)
            .collect::<Result<Vec<_>, _>>()?;

        let mut inputs =
            ProductLinkInputs::new(link_target, policy).with_native_inputs(native_inputs);

        if let Some(runtime) = libraries.runtime {
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

        if configuration.uses_thin_lto() {
            if let Some(profile) = self.state.fact_runtime.profile() {
                use crate::profile::ProfileMetricKind as Metric;

                for (metric, value) in [
                    (Metric::OptimizationModules, libraries.optimization_modules),
                    (Metric::OptimizationInputBytes, libraries.optimization_bytes),
                    (
                        Metric::OptimizationWorkers,
                        u64::try_from(self.worker_budget().get()).unwrap_or(u64::MAX),
                    ),
                    (
                        Metric::OptimizationPreservationRoots,
                        u64::try_from(preservation_roots.len()).unwrap_or(u64::MAX),
                    ),
                ] {
                    profile.record_metric(metric, value);
                }
            }
        }

        inputs = inputs.with_retained_symbols(preservation_roots);

        let mut native_exports = super::plan::product_native_exports(mappings)
            .filter(|_| product != LinkedProductKind::StaticLibrary)
            .cloned()
            .collect::<BTreeSet<_>>();

        if product == LinkedProductKind::SharedLibrary {
            if let Some(host) = product_host {
                native_exports.extend([
                    host.descriptor_symbol().clone(),
                    host.control_symbol().clone(),
                ]);
            }
        }

        inputs = inputs.with_exported_symbols(native_exports);

        Ok((inputs, libraries.payloads))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "final native closure combines existing host, runtime and link contracts"
    )]
    fn native_library_link_selection(
        &self,
        product_kind: LinkedProductKind,
        imported_symbols: &BTreeSet<&str>,
        provided_symbols: &BTreeMap<Cow<'_, str>, NativeSymbolBinding>,
        target: &CodegenTarget,
        configuration: crate::BuildConfiguration,
        runtime: Option<RuntimeArtifactPlan>,
        product_host: Option<&bray_codegen::CodegenProductHostMapping>,
    ) -> Result<NativeLibraryLinkSelection, NativeProductPlanningError> {
        if product_kind == LinkedProductKind::StaticLibrary {
            return Ok(NativeLibraryLinkSelection {
                payloads: Vec::new(),
                optimization_modules: 0,
                optimization_bytes: 0,
                runtime: None,
            });
        }

        let libraries = self.native_libraries(
            configuration.codegen_options(),
            configuration.uses_thin_lto(),
        )?;

        let target = bray_target::NativeTarget::for_identity(target.identity())
            .expect("native selection requires a supported native target");

        let demands = imported_symbols.iter().map(|name| {
            bray_symbols::NativeSymbolContract::required_name(
                bray_base::NonEmptySharedStr::try_new(
                    target.object_symbol_name(name.as_ref()).as_ref(),
                )
                .expect("mapped imported symbol must be nonempty"),
            )
        });

        let provided = provided_symbols.iter().map(|(name, &binding)| {
            (
                bray_symbols::NativeSymbolIdentity::Name(
                    bray_base::NonEmptySharedStr::try_new(
                        target.object_symbol_name(name.as_ref()).as_ref(),
                    )
                    .expect("mapped provided symbol must be nonempty"),
                ),
                binding,
            )
        });

        let static_demands = product_host
            .into_iter()
            .flat_map(|host| host.statics())
            .map(|entry| entry.identity().bytes())
            .filter(|identity| libraries.resolver.static_entry(identity).is_some());

        let runtime_ordinal = libraries.resolver.artifacts().len();

        let selection = libraries.select(
            demands,
            provided,
            static_demands,
            runtime.as_ref(),
            self.native_link_inputs(),
        )?;

        let package_units = selection
            .units()
            .iter()
            .copied()
            .filter(|location| location.artifact < runtime_ordinal)
            .collect::<Vec<_>>();

        let runtime = runtime.map(|runtime| {
            runtime.finish(
                selection
                    .units()
                    .iter()
                    .filter(|location| location.artifact == runtime_ordinal)
                    .map(|location| location.digest),
            )
        });

        let payloads = package_units
            .iter()
            .map(|&location| libraries.payload(location))
            .collect::<Result<Vec<_>, _>>()?;

        let mut modules = 0;
        let mut bytes = 0;
        let mut artifacts = BTreeMap::new();

        for (&location, payload) in package_units.iter().zip(&payloads) {
            let entry = artifacts
                .entry(libraries.path(location))
                .or_insert((0_u32, 0_u64));

            if payload.kind == bray_native_artifact::NativeUnitKind::Bitcode {
                entry.0 += 1;

                entry.1 += u64::try_from(payload.bytes.len())
                    .expect("payload length must fit profile counter");
            }
        }

        let artifacts = artifacts
            .into_iter()
            .map(|(path, (count, size))| {
                modules += u64::from(count);
                bytes += size;

                bray_profile::CompilationProfileLibraryArtifact {
                    path: path.display().to_string(),
                    modules: count,
                    bytes: size,
                }
            })
            .collect();

        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.set_library_artifacts(artifacts);
        }

        Ok(NativeLibraryLinkSelection {
            payloads,
            optimization_modules: modules,
            optimization_bytes: bytes,
            runtime,
        })
    }

    #[cfg(test)]
    pub(super) fn native_library_link_inputs(
        &self,
        product_kind: ProductKind,
        imported_symbols: &BTreeSet<&str>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
    ) -> Result<
        (Vec<LinkInputSpec>, Vec<super::reuse::SelectedNativePayload>),
        NativeProductPlanningError,
    > {
        let target = self
            .requested_target()
            .codegen_target()
            .map_err(NativeProductPlanningError::InvalidCodegenTarget)?;

        let provided = platform_overrides
            .iter()
            .map(|role| {
                (
                    Cow::Borrowed(role.native_symbol()),
                    NativeSymbolBinding::Strong,
                )
            })
            .collect::<BTreeMap<_, _>>();

        self.native_library_link_selection(
            match product_kind {
                ProductKind::Library => LinkedProductKind::StaticLibrary,
                ProductKind::Executable | ProductKind::Test => LinkedProductKind::Executable,
            },
            imported_symbols,
            &provided,
            &target,
            crate::BuildConfiguration::Development,
            None,
            None,
        )
        .map(|selection| {
            (
                selection
                    .payloads
                    .iter()
                    .flat_map(|unit| unit.native_links.iter().cloned())
                    .collect(),
                selection.payloads,
            )
        })
    }
}

pub(super) fn product_native_definitions(
    mappings: &[bray_codegen::CodegenMappings],
) -> BTreeMap<Cow<'_, str>, NativeSymbolBinding> {
    let mut definitions = BTreeMap::new();

    for mapping in mappings {
        for symbol in mapping
            .symbols()
            .iter()
            .filter(|symbol| symbol.defines_in(mapping.unit()))
        {
            if let Some(binding) = product_symbol_binding(symbol.linkage()) {
                // An emitted LinkOnce body satisfies its references without selecting another copy.
                insert_product_definition(
                    &mut definitions,
                    Cow::Borrowed(symbol.name().as_str()),
                    binding,
                );
            }

            if let Some(entry) = symbol.native_entry() {
                let binding = product_symbol_binding(entry.linkage())
                    .expect("emitted native callback must have definition linkage");

                insert_product_definition(
                    &mut definitions,
                    Cow::Borrowed(entry.name().as_str()),
                    binding,
                );
            }
        }

        for storage in mapping
            .static_storages()
            .iter()
            .filter(|storage| storage.defines_storage())
        {
            insert_product_definition(
                &mut definitions,
                Cow::Borrowed(storage.symbol().as_str()),
                storage
                    .native_binding()
                    .unwrap_or(NativeSymbolBinding::Strong),
            );

            insert_product_definition(
                &mut definitions,
                Cow::Owned(storage.host_name()),
                NativeSymbolBinding::Strong,
            );
        }

        for storage in mapping
            .native_storages()
            .iter()
            .filter(|storage| storage.direction() == bray_symbols::ForeignCallableDirection::Export)
        {
            insert_product_definition(
                &mut definitions,
                Cow::Borrowed(storage.symbol().as_str()),
                storage.binding(),
            );
        }
    }

    definitions
}

pub(super) fn product_symbol_binding(
    linkage: bray_codegen::CodegenLinkage,
) -> Option<NativeSymbolBinding> {
    match linkage {
        bray_codegen::CodegenLinkage::Internal
        | bray_codegen::CodegenLinkage::External
        | bray_codegen::CodegenLinkage::Export
        | bray_codegen::CodegenLinkage::LinkOnce => Some(NativeSymbolBinding::Strong),
        bray_codegen::CodegenLinkage::Weak
        | bray_codegen::CodegenLinkage::Fallback
        | bray_codegen::CodegenLinkage::Common => Some(NativeSymbolBinding::Weak),
        bray_codegen::CodegenLinkage::Private | bray_codegen::CodegenLinkage::Import => None,
    }
}

pub(super) fn insert_product_definition<Name: Ord>(
    definitions: &mut BTreeMap<Name, NativeSymbolBinding>,
    name: Name,
    binding: NativeSymbolBinding,
) {
    let existing = definitions.entry(name).or_insert(binding);

    *existing = existing.strongest(binding);
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
        NativeLinkKind::Static => {
            LinkInputSpec::try_static_native_library(requirement.name(), provenance)
        }
        NativeLinkKind::Dynamic | NativeLinkKind::System => {
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
