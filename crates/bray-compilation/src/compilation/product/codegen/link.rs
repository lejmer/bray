use std::collections::BTreeSet;

use bray_codegen::CodegenTarget;
use bray_emitter::ProductLinkInputs;
use bray_linker::{
    DeadStripPolicy, DebugLinkPolicy, LinkInputKind, LinkInputMode, LinkInputProvenance,
    LinkInputSource, LinkInputSpec, LinkModel, LinkPolicy, LinkTarget, LinkedProductKind,
    SectionGarbageCollectionPolicy,
};
use bray_runtime_interface::{ExecutableHostContract, RuntimeArtifactSelection};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement, ProductKind};

use super::super::super::Compilation;
use super::error::NativeProductPlanningError;

struct StandardLibraryLinkSelection {
    inputs: Vec<Result<LinkInputSpec, NativeProductPlanningError>>,
    optimization_modules: u64,
    optimization_bytes: u64,
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
    ) -> Result<ProductLinkInputs, NativeProductPlanningError> {
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

        let platform_overrides = runtime
            .iter()
            .flat_map(|runtime| {
                runtime
                    .components()
                    .iter()
                    .flat_map(|component| component.metadata().platform_services())
            })
            .copied()
            .collect();

        let standard_library = self.standard_library_link_selection(
            kind,
            &imported_symbols,
            &platform_overrides,
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

        let mut preservation_roots =
            super::plan::product_preservation_roots(mappings, product_host)
                .cloned()
                .collect::<BTreeSet<_>>();

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
            .flat_map(bray_codegen::CodegenMappings::native_storages)
            .filter(|mapping| {
                mapping.direction() == bray_symbols::ForeignCallableDirection::Export
            })
            .map(bray_codegen::CodegenNativeStaticMapping::symbol)
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

        Ok(inputs)
    }

    fn standard_library_link_selection(
        &self,
        product_kind: ProductKind,
        imported_symbols: &BTreeSet<&str>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        target: &CodegenTarget,
        configuration: crate::BuildConfiguration,
    ) -> Result<StandardLibraryLinkSelection, NativeProductPlanningError> {
        if product_kind == ProductKind::Library {
            return Ok(StandardLibraryLinkSelection {
                inputs: Vec::new(),
                optimization_modules: 0,
                optimization_bytes: 0,
            });
        }

        let Some(resolver) = self.standard_library_provider_resolver() else {
            return Ok(StandardLibraryLinkSelection {
                inputs: Vec::new(),
                optimization_modules: 0,
                optimization_bytes: 0,
            });
        };

        let selected = self.requested_target();

        let available_services = resolver
            .target_platform_services(selected.profile().identity(), selected.runtime_abi())
            .map_err(NativeProductPlanningError::StandardLibrary)?;

        let platform_services = platform_services_for_imported_symbols(
            &available_services,
            imported_symbols.iter().copied(),
        );

        let provider_services = platform_services
            .difference(platform_overrides)
            .copied()
            .collect::<Vec<_>>();

        let artifacts = if configuration.uses_thin_lto() {
            let artifacts = resolver
                .target_artifacts(selected.profile().identity(), selected.runtime_abi())
                .map_err(NativeProductPlanningError::StandardLibrary)?;

            let codegen = self
                .state
                .codegen
                .as_ref()
                .ok_or(NativeProductPlanningError::CodegenUnavailable)?;

            let compatibility = codegen
                .selected_bitcode_target_contract(target)
                .map_err(NativeProductPlanningError::BitcodeTargetContract)?
                .ok_or_else(|| {
                    NativeProductPlanningError::StandardLibrary(
                        bray_standard_library::StandardLibraryLoadError::OptimizationUnavailable {
                            target: target.identity().clone(),
                        },
                    )
                })?;

            select_optimization_artifacts(
                &artifacts,
                &compatibility,
                selected.runtime_abi(),
                codegen.selected(),
            )?
        } else {
            resolver
                .link_artifacts_for_platform_services(
                    selected.profile().identity(),
                    selected.runtime_abi(),
                    &provider_services,
                )
                .map_err(NativeProductPlanningError::StandardLibrary)?
                .iter()
                .cloned()
                .collect()
        };

        let package = bray_symbols::PackageIdentity::try_new(
            bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        )
        .unwrap_or_else(|| panic!("standard library package identity must be valid"));

        // Every standard-library input retains the Arc-backed package provenance.
        let selected_artifacts = artifacts.iter();

        let optimization_modules = selected_artifacts
            .clone()
            .filter_map(|artifact| artifact.metadata().optimization())
            .map(|metadata| u64::from(metadata.module_count().get()))
            .sum();

        let optimization_bytes = selected_artifacts
            .clone()
            .filter(|artifact| artifact.metadata().optimization().is_some())
            .map(|artifact| artifact.metadata().byte_len())
            .sum();

        let artifact_inputs = selected_artifacts.clone().filter_map(|artifact| {
            let kind = match artifact.metadata().kind() {
                bray_standard_library::StandardLibraryArtifactKind::RelocatableObject => {
                    LinkInputKind::RelocatableObject
                }
                bray_standard_library::StandardLibraryArtifactKind::StaticLibrary => {
                    LinkInputKind::Archive
                }
                bray_standard_library::StandardLibraryArtifactKind::PlatformServiceLibrary => {
                    LinkInputKind::Archive
                }
                bray_standard_library::StandardLibraryArtifactKind::OptimizationArchive => {
                    LinkInputKind::Archive
                }
                bray_standard_library::StandardLibraryArtifactKind::PackageInterface
                | bray_standard_library::StandardLibraryArtifactKind::PackageImplementation
                | bray_standard_library::StandardLibraryArtifactKind::DependencyMetadata
                | bray_standard_library::StandardLibraryArtifactKind::RuntimeArtifact => {
                    return None;
                }
                bray_standard_library::StandardLibraryArtifactKind::SharedLibrary => {
                    return Some(Err(NativeProductPlanningError::InvalidNativeLinkInput));
                }
            };

            Some(
                LinkInputSpec::try_new(
                    kind,
                    LinkInputSource::file(artifact.path()),
                    standard_library_artifact_provenance(artifact.metadata(), &package),
                    LinkInputMode::Ordinary,
                )
                .map_err(|_| NativeProductPlanningError::InvalidNativeLinkInput),
            )
        });

        let native_links: BTreeSet<_> = selected_artifacts
            .flat_map(|artifact| artifact.metadata().native_links())
            .collect();

        let native_inputs = native_links.into_iter().map(|requirement| {
            native_link_input(
                requirement,
                LinkInputProvenance::PlatformProvider(package.clone()),
            )
        });

        let inputs = artifact_inputs.chain(native_inputs).collect();

        Ok(StandardLibraryLinkSelection {
            inputs,
            optimization_modules,
            optimization_bytes,
        })
    }

    #[cfg(test)]
    pub(super) fn standard_library_link_inputs(
        &self,
        product_kind: ProductKind,
        imported_symbols: &BTreeSet<&str>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
    ) -> Result<Vec<Result<LinkInputSpec, NativeProductPlanningError>>, NativeProductPlanningError>
    {
        let target = self
            .requested_target()
            .codegen_target()
            .map_err(NativeProductPlanningError::InvalidCodegenTarget)?;

        self.standard_library_link_selection(
            product_kind,
            imported_symbols,
            platform_overrides,
            &target,
            crate::BuildConfiguration::Development,
        )
        .map(|selection| selection.inputs)
    }
}

fn standard_library_artifact_provenance(
    artifact: &bray_standard_library::StandardLibraryArtifact,
    package: &bray_symbols::PackageIdentity,
) -> LinkInputProvenance {
    let is_native_provider = artifact.kind()
        == bray_standard_library::StandardLibraryArtifactKind::PlatformServiceLibrary
        || artifact.optimization().is_some_and(|optimization| {
            optimization.producer().kind()
                == bray_standard_library::StandardLibraryOptimizationProducerKind::PinnedNative
        });

    if is_native_provider {
        LinkInputProvenance::PlatformProvider(package.clone())
    } else {
        LinkInputProvenance::Package(package.clone())
    }
}

fn select_optimization_artifacts(
    artifacts: &[bray_standard_library::ResolvedStandardLibraryArtifact],
    compatibility: &bray_codegen::BackendBitcodeTargetContract,
    runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    backend: &bray_codegen::BackendIdentity,
) -> Result<Vec<bray_standard_library::ResolvedStandardLibraryArtifact>, NativeProductPlanningError>
{
    let metadata = artifacts
        .iter()
        .map(bray_standard_library::ResolvedStandardLibraryArtifact::metadata)
        .collect::<Vec<_>>();

    select_optimization_artifact_indices(&metadata, compatibility, runtime_abi, backend)
        .map(|indices| {
            indices
                .into_iter()
                .map(|index| artifacts[index].clone())
                .collect()
        })
}

fn select_optimization_artifact_indices(
    artifacts: &[&bray_standard_library::StandardLibraryArtifact],
    compatibility: &bray_codegen::BackendBitcodeTargetContract,
    runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    backend: &bray_codegen::BackendIdentity,
) -> Result<Vec<usize>, NativeProductPlanningError> {
    let compatible_optimization = artifacts
        .iter()
        .enumerate()
        .filter_map(|(index, artifact)| {
            optimization_artifact_is_compatible(
                artifact,
                compatibility,
                runtime_abi,
                backend,
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();

    let fully_optimized_fallbacks = compatible_optimization
        .iter()
        .filter_map(|index| artifacts[*index].optimization())
        .filter(|metadata| optimization_fully_replaces_fallback(metadata, artifacts))
        .map(|metadata| (metadata.fallback().path(), metadata.fallback().digest()))
        .collect::<BTreeSet<_>>();

    let selected = compatible_optimization
        .iter()
        .copied()
        .chain(artifacts.iter().enumerate().filter_map(|(index, artifact)| {
            (artifact.kind()
                == bray_standard_library::StandardLibraryArtifactKind::PlatformServiceLibrary
                && !fully_optimized_fallbacks
                    .contains(&(artifact.path(), artifact.digest())))
            .then_some(index)
        }))
        .collect::<Vec<_>>();

    if !selected.iter().any(|index| {
        artifacts[*index]
            .optimization()
            .is_some_and(|metadata| metadata.partition() == "std")
    }) {
        return Err(NativeProductPlanningError::StandardLibrary(
            bray_standard_library::StandardLibraryLoadError::OptimizationUnavailable {
                target: compatibility.target().clone(),
            },
        ));
    }

    Ok(selected)
}

fn optimization_fully_replaces_fallback(
    optimization: &bray_standard_library::StandardLibraryOptimizationMetadata,
    artifacts: &[&bray_standard_library::StandardLibraryArtifact],
) -> bool {
    artifacts.iter().any(|artifact| {
        artifact.path() == optimization.fallback().path()
            && artifact.digest() == optimization.fallback().digest()
            && optimization_covers_fallback(optimization, artifact)
    })
}

fn optimization_covers_fallback(
    optimization: &bray_standard_library::StandardLibraryOptimizationMetadata,
    fallback: &bray_standard_library::StandardLibraryArtifact,
) -> bool {
    fallback.platform_services().iter().all(|service| {
        optimization
            .platform_services()
            .binary_search(service)
            .is_ok()
    })
}

fn optimization_artifact_is_compatible(
    artifact: &bray_standard_library::StandardLibraryArtifact,
    compatibility: &bray_codegen::BackendBitcodeTargetContract,
    runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    backend: &bray_codegen::BackendIdentity,
) -> bool {
    let Some(metadata) = artifact.optimization() else {
        return false;
    };

    let artifact_compatibility = metadata.compatibility();
    let producer = metadata.producer();

    if metadata.semantics() != bray_standard_library::StandardLibraryOptimizationSemantics::ThinLto
        || artifact_compatibility.triple() != compatibility.triple()
        || artifact_compatibility.data_layout() != compatibility.data_layout()
        || artifact_compatibility.relocation_model() != compatibility.relocation_model()
        || artifact_compatibility.code_model() != compatibility.code_model()
        || artifact_compatibility.runtime_abi() != runtime_abi
        || producer.toolchain() != "llvm"
        || producer.toolchain_revision() != backend.toolchain_revision()
    {
        return false;
    }

    match producer.kind() {
        bray_standard_library::StandardLibraryOptimizationProducerKind::Bray => {
            metadata.partition() == "std"
                && producer.implementation() == backend.name()
                && producer.implementation_revision() == backend.revision()
        }
        bray_standard_library::StandardLibraryOptimizationProducerKind::PinnedNative => true,
    }
}

pub(super) fn platform_services_for_imported_symbols<'symbol>(
    available_services: &[bray_runtime_interface::PlatformServiceRole],
    imported_symbols: impl IntoIterator<Item = &'symbol str>,
) -> BTreeSet<bray_runtime_interface::PlatformServiceRole> {
    let imported_symbols: BTreeSet<_> = imported_symbols.into_iter().collect();

    available_services
        .iter()
        .copied()
        .filter(|role| {
            imported_symbols.contains(bray_runtime_interface::native_platform_service_role_symbol(
                *role,
            ))
        })
        .collect()
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

fn native_link_input(
    requirement: &NativeLinkRequirement,
    provenance: LinkInputProvenance,
) -> Result<LinkInputSpec, NativeProductPlanningError> {
    let input = match requirement.kind() {
        NativeLinkKind::Dynamic | NativeLinkKind::Static | NativeLinkKind::System => {
            LinkInputSpec::try_native_library(requirement.name(), provenance)
        }
        NativeLinkKind::Framework => LinkInputSpec::try_framework(requirement.name(), provenance),
    };

    input.ok_or(NativeProductPlanningError::InvalidNativeLinkInput)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_standard_library::{
        StandardLibraryArtifact, StandardLibraryArtifactKind,
        StandardLibraryOptimizationCompatibility, StandardLibraryOptimizationFallback,
        StandardLibraryOptimizationMetadata, StandardLibraryOptimizationProducer,
        StandardLibraryOptimizationProducerKind,
    };
    use bray_target::NativeTarget;

    use super::{
        optimization_artifact_is_compatible, optimization_covers_fallback,
        select_optimization_artifact_indices, standard_library_artifact_provenance,
    };

    #[test]
    fn optimization_selection_requires_the_complete_backend_target_contract() {
        let target = bray_codegen::CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);

        let contract =
            bray_codegen::BackendBitcodeTargetContract::try_new(&target, "test-data-layout")
                .unwrap_or_else(|| panic!("test bitcode contract must be valid"));

        let backend = bray_codegen::BackendIdentity::try_new("llvm", "1", "22.1.8")
            .unwrap_or_else(|| panic!("test backend identity must be valid"));

        let runtime_abi = RuntimeAbiVersion::new(1, 0);
        let artifact = optimization_artifact(&contract, runtime_abi, &backend);

        assert!(optimization_artifact_is_compatible(
            &artifact,
            &contract,
            runtime_abi,
            &backend,
        ));

        let different_layout =
            bray_codegen::BackendBitcodeTargetContract::try_new(&target, "different-data-layout")
                .unwrap_or_else(|| panic!("different test bitcode contract must be valid"));

        assert!(!optimization_artifact_is_compatible(
            &artifact,
            &different_layout,
            runtime_abi,
            &backend,
        ));

        assert!(!optimization_artifact_is_compatible(
            &artifact,
            &contract,
            RuntimeAbiVersion::new(1, 1),
            &backend,
        ));
    }

    #[test]
    fn partial_provider_optimization_retains_its_object_fallback() {
        use bray_runtime_interface::PlatformServiceRole;

        let fallback = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            "targets/test/libprovider.a",
            b"fallback",
        )
        .map(|artifact| {
            artifact.with_platform_services([
                PlatformServiceRole::ContextNativeTextWidth,
                PlatformServiceRole::TimeDateValidate,
            ])
        })
        .unwrap_or_else(|error| panic!("test fallback must be valid: {error:?}"));

        let target = bray_codegen::CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);

        let contract = bray_codegen::BackendBitcodeTargetContract::try_new(&target, "layout")
            .unwrap_or_else(|| panic!("test target contract must be valid"));

        let backend = bray_codegen::BackendIdentity::try_new("llvm", "1", "22.1.8")
            .unwrap_or_else(|| panic!("test backend identity must be valid"));

        let optimization =
            optimization_metadata(&fallback, &contract, RuntimeAbiVersion::new(1, 0), &backend)
                .with_platform_services([PlatformServiceRole::TimeDateValidate]);

        assert!(!optimization_covers_fallback(&optimization, &fallback));

        let complete = optimization.clone().with_platform_services([
            PlatformServiceRole::ContextNativeTextWidth,
            PlatformServiceRole::TimeDateValidate,
        ]);

        assert!(optimization_covers_fallback(&complete, &fallback));

        let standard_library = optimization_artifact(
            &contract,
            RuntimeAbiVersion::new(1, 0),
            &backend,
        );

        let provider = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::OptimizationArchive,
            "targets/test/libprovider_optimization.a",
            b"optimization",
        )
        .map(|artifact| artifact.with_optimization(optimization))
        .unwrap_or_else(|error| panic!("test optimization artifact must be valid: {error:?}"));

        let artifacts = [&fallback, &standard_library, &provider];

        let selected = select_optimization_artifact_indices(
            &artifacts,
            &contract,
            RuntimeAbiVersion::new(1, 0),
            &backend,
        )
        .unwrap_or_else(|error| panic!("test optimization selection must be valid: {error:?}"));

        let selected_paths = selected
            .into_iter()
            .map(|index| artifacts[index].path())
            .collect::<Vec<_>>();

        assert_eq!(
            selected_paths,
            [
                standard_library.path(),
                provider.path(),
                fallback.path(),
            ]
        );
    }

    #[test]
    fn pinned_native_optimization_keeps_platform_provider_precedence() {
        use bray_runtime_interface::PlatformServiceRole;

        let fallback = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            "targets/test/libprovider.a",
            b"fallback",
        )
        .map(|artifact| {
            artifact.with_platform_services([PlatformServiceRole::StandardOutputWrite])
        })
        .unwrap_or_else(|error| panic!("test fallback must be valid: {error:?}"));

        let target = bray_codegen::CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);

        let contract = bray_codegen::BackendBitcodeTargetContract::try_new(&target, "layout")
            .unwrap_or_else(|| panic!("test target contract must be valid"));

        let backend = bray_codegen::BackendIdentity::try_new("llvm", "1", "22.1.8")
            .unwrap_or_else(|| panic!("test backend identity must be valid"));

        let optimization = optimization_metadata_for(
            &fallback,
            &contract,
            RuntimeAbiVersion::new(1, 0),
            &backend,
            StandardLibraryOptimizationProducerKind::PinnedNative,
            "provider",
        )
        .with_platform_services([PlatformServiceRole::StandardOutputWrite]);

        let artifact = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::OptimizationArchive,
            "targets/test/libprovider_optimization.a",
            b"optimization",
        )
        .map(|artifact| artifact.with_optimization(optimization))
        .unwrap_or_else(|error| panic!("test optimization artifact must be valid: {error:?}"));

        let package = bray_symbols::PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        assert!(matches!(
            standard_library_artifact_provenance(&artifact, &package),
            bray_linker::LinkInputProvenance::PlatformProvider(_)
        ));
    }

    fn optimization_artifact(
        contract: &bray_codegen::BackendBitcodeTargetContract,
        runtime_abi: RuntimeAbiVersion,
        backend: &bray_codegen::BackendIdentity,
    ) -> StandardLibraryArtifact {
        let fallback = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::StaticLibrary,
            "targets/test/libstd.a",
            b"fallback",
        )
        .unwrap_or_else(|error| panic!("test fallback must be valid: {error:?}"));

        let metadata = optimization_metadata(&fallback, contract, runtime_abi, backend);

        StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::OptimizationArchive,
            "targets/test/libstd_optimization.a",
            b"optimization",
        )
        .map(|artifact| artifact.with_optimization(metadata))
        .unwrap_or_else(|error| panic!("test optimization artifact must be valid: {error:?}"))
    }

    fn optimization_metadata(
        fallback: &StandardLibraryArtifact,
        contract: &bray_codegen::BackendBitcodeTargetContract,
        runtime_abi: RuntimeAbiVersion,
        backend: &bray_codegen::BackendIdentity,
    ) -> StandardLibraryOptimizationMetadata {
        optimization_metadata_for(
            fallback,
            contract,
            runtime_abi,
            backend,
            StandardLibraryOptimizationProducerKind::Bray,
            "std",
        )
    }

    fn optimization_metadata_for(
        fallback: &StandardLibraryArtifact,
        contract: &bray_codegen::BackendBitcodeTargetContract,
        runtime_abi: RuntimeAbiVersion,
        backend: &bray_codegen::BackendIdentity,
        producer_kind: StandardLibraryOptimizationProducerKind,
        partition: &str,
    ) -> StandardLibraryOptimizationMetadata {
        let fallback =
            StandardLibraryOptimizationFallback::try_new(fallback.path(), fallback.digest())
                .unwrap_or_else(|error| panic!("test fallback contract must be valid: {error:?}"));

        let producer = StandardLibraryOptimizationProducer::try_new(
            producer_kind,
            backend.name(),
            backend.revision(),
            "llvm",
            backend.toolchain_revision(),
        )
        .unwrap_or_else(|error| panic!("test producer must be valid: {error:?}"));

        let compatibility = StandardLibraryOptimizationCompatibility::try_new(
            contract.triple(),
            contract.data_layout(),
            contract.relocation_model(),
            contract.code_model(),
            runtime_abi,
        )
        .unwrap_or_else(|error| panic!("test compatibility must be valid: {error:?}"));

        StandardLibraryOptimizationMetadata::try_new(
            partition,
            producer,
            compatibility,
            fallback,
            NonZeroU32::MIN,
        )
        .unwrap_or_else(|error| panic!("test optimization metadata must be valid: {error:?}"))
    }
}
