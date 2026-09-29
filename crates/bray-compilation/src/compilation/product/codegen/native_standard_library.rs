use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use bray_base::NonEmptySharedStr;
use bray_codegen::CodegenTarget;
use bray_linker::{
    LinkInputKind, LinkInputMode, LinkInputProvenance, LinkInputSource, LinkInputSpec,
};
use bray_native_artifact::{NativeUnitKind, NativeUnitResolver};
use bray_runtime_interface::RuntimeAbiVersion;
use bray_standard_library::{StandardLibraryLoadError, StandardLibraryResolver};
use bray_symbols::{NativeSymbolBinding, NativeSymbolContract, NativeSymbolIdentity, PackageIdentity};
use bray_target::{NativeTarget, TargetIdentity};

use super::super::super::Compilation;
use super::error::{NativeLinkInputPlanningError, NativeProductPlanningError};
use super::link::native_link_input;
use super::reuse::native_producer_identity;

pub(super) struct NativeStandardLibrarySelection {
    pub(super) inputs: Vec<LinkInputSpec>,
    pub(super) index_path: PathBuf,
    pub(super) modules: u64,
    pub(super) bytes: u64,
}

impl Compilation {
    pub(super) fn select_standard_library_native(
        &self,
        resolver: &StandardLibraryResolver,
        target_identity: &TargetIdentity,
        runtime_abi: RuntimeAbiVersion,
        imported_symbols: &BTreeSet<&str>,
        provided_symbols: &BTreeMap<&str, NativeSymbolBinding>,
        target: &CodegenTarget,
        configuration: crate::BuildConfiguration,
        package: &PackageIdentity,
    ) -> Result<NativeStandardLibrarySelection, NativeProductPlanningError> {
        let resolved = if configuration.uses_thin_lto() {
            let codegen = self.state.codegen.as_ref()
                .ok_or(NativeProductPlanningError::CodegenUnavailable)?;

            let native_target = NativeTarget::for_identity(target.identity())
                .expect("selected codegen target must have a native artifact identity");

            let implementation = self.package_implementation_configuration(None)
                .map_err(NativeProductPlanningError::InvalidCodegenTarget)?;

            let producer = native_producer_identity(
                codegen.selected(),
                &configuration.codegen_options(),
                &implementation,
                native_target,
            );

            match resolver.native_artifact(target_identity, runtime_abi, producer) {
                Ok(Some(artifact)) => Ok(Some(artifact)),
                Ok(None) => resolver.native_object_artifact(target_identity, runtime_abi),
                Err(error) => Err(error),
            }
        } else {
            resolver.native_object_artifact(target_identity, runtime_abi)
        };

        let (index_path, artifact) = resolved.map_err(|cause| NativeProductPlanningError::StandardLibrary {
            artifact_path: resolver.root().path().join(bray_standard_library::STANDARD_LIBRARY_MANIFEST_FILE_NAME),
            cause,
        })?.ok_or_else(|| NativeProductPlanningError::StandardLibrary {
            artifact_path: resolver.root().path().join(bray_standard_library::STANDARD_LIBRARY_MANIFEST_FILE_NAME),
            cause: StandardLibraryLoadError::OptimizationUnavailable {
                target: target.identity().clone(),
            },
        })?;

        let demands = imported_symbols.iter().map(|name| {
            NativeSymbolContract::required_name(
                NonEmptySharedStr::try_new(*name)
                    .expect("mapped imported native symbol must be nonempty"),
            )
        });

        // The index clone shares its immutable unit arrays with the resolver.
        let unit_resolver = NativeUnitResolver::new(artifact.index().clone());

        let provided = provided_symbols.iter().map(|(&name, &binding)| {
            (NativeSymbolIdentity::Name(NonEmptySharedStr::try_new(name)
                .expect("mapped provided native symbol must be nonempty")), binding)
        });

        let selected = unit_resolver.select_with_provided(demands, provided)
            .map_err(|cause| NativeProductPlanningError::StandardLibrary {
                artifact_path: index_path.clone(),
                cause: StandardLibraryLoadError::NativeResolution {
                    path: index_path.clone(),
                    cause,
                },
            })?;

        let mut inputs = Vec::new();
        let mut native_links = Vec::new();
        let mut seen_native_links = BTreeSet::new();
        let mut modules = 0_u64;
        let mut bytes = 0_u64;

        for &digest in selected.units() {
            let index = artifact.index().units().binary_search_by_key(&digest, |unit| unit.digest())
                .expect("resolved native unit must appear in its validated index");

            let unit = &artifact.index().units()[index];

            let path = artifact.payload(digest)
                .expect("authenticated native unit must retain its payload path");

            let kind = match unit.kind() {
                NativeUnitKind::Bitcode => LinkInputKind::Bitcode,
                NativeUnitKind::Object => LinkInputKind::RelocatableObject,
                NativeUnitKind::OpaqueArchive => LinkInputKind::Archive,
            };

            if kind == LinkInputKind::Bitcode {
                modules += 1;

                bytes += std::fs::metadata(path).map_err(|error| {
                    NativeProductPlanningError::StandardLibrary {
                        artifact_path: path.to_path_buf(),
                        cause: StandardLibraryLoadError::Read {
                            path: path.to_path_buf(),
                            kind: error.kind(),
                        },
                    }
                })?.len();
            }

            inputs.push(LinkInputSpec::try_new(
                kind,
                LinkInputSource::file(path),
                LinkInputProvenance::Package(package.clone()),
                LinkInputMode::Ordinary,
            ).map_err(|cause| NativeProductPlanningError::InvalidNativeLinkInput(
                NativeLinkInputPlanningError::InvalidStandardLibraryArtifact {
                    path: path.to_path_buf(),
                    kind,
                    cause,
                },
            ))?);

            for link in unit.native_links() {
                if seen_native_links.insert(link) {
                    native_links.push(link);
                }
            }
        }

        for link in native_links {
            inputs.push(native_link_input(
                link,
                LinkInputProvenance::Package(package.clone()),
            )?);
        }

        Ok(NativeStandardLibrarySelection {
            inputs,
            index_path,
            modules,
            bytes,
        })
    }
}
