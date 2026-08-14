use std::collections::BTreeSet;

use bray_codegen::CodegenTarget;
use bray_emitter::ProductLinkFacts;
use bray_linker::{
    DeadStripPolicy, DebugLinkPolicy, LinkInputKind, LinkInputMode, LinkInputProvenance,
    LinkInputSource, LinkInputSpec, LinkModel, LinkPolicy, LinkTarget, LinkedProductKind,
    SectionGarbageCollectionPolicy,
};
use bray_runtime_interface::{ExecutableHostContract, RuntimeArtifactSelection};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement, ProductKind};

use super::super::super::Compilation;
use super::error::NativeProductFactError;

impl Compilation {
    pub(super) fn product_link_facts(
        &self,
        kind: ProductKind,
        host: Option<&ExecutableHostContract>,
        runtime: Option<RuntimeArtifactSelection>,
        mappings: &[bray_codegen::CodegenMappings],
        target: &CodegenTarget,
        configuration: crate::BuildConfiguration,
    ) -> Result<ProductLinkFacts, NativeProductFactError> {
        let product = match kind {
            ProductKind::Library => LinkedProductKind::StaticLibrary,
            ProductKind::Executable | ProductKind::Test => LinkedProductKind::Executable,
        };

        let link_model = product_link_model(target.machine().object_format());

        // Link facts own the Arc-backed target identity independently of codegen facts.
        let link_target = LinkTarget::try_new(
            target.identity().clone(),
            target.triple(),
            target.machine().architecture(),
            target.machine().object_format(),
            target.relocation_model(),
            target.code_model(),
            link_model,
        )
        .map_err(NativeProductFactError::InvalidLinkTarget)?;

        let linked_debug = match configuration {
            crate::BuildConfiguration::Development
                if configuration
                    .requires_linked_debug_companion(target.machine().object_format()) =>
            {
                DebugLinkPolicy::Companion
            }
            crate::BuildConfiguration::Development => DebugLinkPolicy::Embedded,
            crate::BuildConfiguration::Release
            | crate::BuildConfiguration::ObservedRelease
            | crate::BuildConfiguration::TimedRelease { .. } => {
                DebugLinkPolicy::None
            }
        };

        let preserve_unused = configuration.preserves_unused_link_content();

        let policy = match product {
            LinkedProductKind::StaticLibrary => LinkPolicy::new(
                DeadStripPolicy::Preserve,
                SectionGarbageCollectionPolicy::Preserve,
                match configuration {
                    crate::BuildConfiguration::Development => DebugLinkPolicy::Embedded,
                    crate::BuildConfiguration::Release
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
            .flat_map(bray_codegen::CodegenMappings::symbols)
            .filter(|symbol| symbol.linkage() == bray_codegen::CodegenLinkage::Import)
            .map(|symbol| symbol.name().as_str())
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

        let standard_library_inputs = self.standard_library_link_inputs(
            kind,
            &imported_symbols,
            &platform_overrides,
        )?;

        let native_inputs = configured_inputs
            .chain(runtime_inputs)
            .chain(standard_library_inputs)
            .collect::<Result<Vec<_>, _>>()?;

        let mut facts =
            ProductLinkFacts::new(link_target, policy).with_native_inputs(native_inputs);

        if let Some(runtime) = runtime {
            facts = facts.with_runtime(runtime);
        }

        if let Some(host) = host {
            // Link facts own the Arc-backed entry symbol after host construction returns.
            facts = facts.with_retained_symbols([host.native_entry().clone()]);
        }

        Ok(facts)
    }

    pub(super) fn standard_library_link_inputs(
        &self,
        product_kind: ProductKind,
        imported_symbols: &BTreeSet<&str>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
    ) -> Result<Vec<Result<LinkInputSpec, NativeProductFactError>>, NativeProductFactError> {
        if product_kind == ProductKind::Library {
            return Ok(Vec::new());
        }

        let Some(resolver) = self.standard_library() else {
            return Ok(Vec::new());
        };

        let selected = self.requested_target();

        let available_services = resolver
            .target_platform_services(selected.profile().identity(), selected.runtime_abi())
            .map_err(NativeProductFactError::StandardLibrary)?;

        let platform_services = platform_services_for_imported_symbols(
            &available_services,
            imported_symbols.iter().copied(),
        );

        let provider_services = platform_services
            .difference(platform_overrides)
            .copied()
            .collect::<Vec<_>>();

        let artifacts = resolver
            .target_artifacts_for_platform_services(
                selected.profile().identity(),
                selected.runtime_abi(),
                &provider_services,
            )
            .map_err(NativeProductFactError::StandardLibrary)?;

        let package = bray_symbols::PackageIdentity::try_new(
            bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        )
        .unwrap_or_else(|| panic!("standard library package identity must be valid"));

        // Every standard-library input retains the Arc-backed package provenance.
        let selected_artifacts = artifacts.iter();

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
                bray_standard_library::StandardLibraryArtifactKind::PackageInterface
                | bray_standard_library::StandardLibraryArtifactKind::PackageImplementation
                | bray_standard_library::StandardLibraryArtifactKind::DependencyMetadata
                | bray_standard_library::StandardLibraryArtifactKind::RuntimeArtifact => {
                    return None;
                }
                bray_standard_library::StandardLibraryArtifactKind::SharedLibrary => {
                    return Some(Err(NativeProductFactError::InvalidNativeLinkInput));
                }
            };

            Some(
                LinkInputSpec::try_new(
                    kind,
                    LinkInputSource::file(artifact.path()),
                    match artifact.metadata().kind() {
                        bray_standard_library::StandardLibraryArtifactKind::PlatformServiceLibrary => {
                            LinkInputProvenance::PlatformProvider(package.clone())
                        }
                        _ => LinkInputProvenance::Package(package.clone()),
                    },
                    LinkInputMode::Ordinary,
                )
                .map_err(|_| NativeProductFactError::InvalidNativeLinkInput),
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

        Ok(inputs)
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
) -> Result<LinkInputSpec, NativeProductFactError> {
    let input = match requirement.kind() {
        NativeLinkKind::Dynamic | NativeLinkKind::Static | NativeLinkKind::System => {
            LinkInputSpec::try_native_library(requirement.name(), provenance)
        }
        NativeLinkKind::Framework => LinkInputSpec::try_framework(requirement.name(), provenance),
    };

    input.ok_or(NativeProductFactError::InvalidNativeLinkInput)
}
