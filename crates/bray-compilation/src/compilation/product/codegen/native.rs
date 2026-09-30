use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use bray_codegen::{
    CodegenLinkage, CodegenOptions, CodegenTarget, demanded_runtime_references_for_mir,
    mapped_symbol_runtime_roles,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_native_artifact::{
    NativeArtifactIndex, NativeUnitKind, NativeUnitLocation, NativeUnitResolver,
};
use bray_package_interface::{
    PackageArtifactInput, PackageImplementationArtifact, PackageNativeArtifactError,
};
use bray_runtime_interface::{BinarySymbolName, RuntimeAbiRole};
use bray_symbols::{PackageIdentity, ProductIdentity};

use super::super::super::Compilation;
use super::super::specialization::ConcreteCodegenReachability;
use super::NativeDemand;
use super::error::NativeProductPlanningError;
use super::reuse::SelectedNativePayload;
use crate::fact::CancellationToken;

/// Selected compatible representations of the actual native dependency set.
#[derive(Debug)]
pub(in crate::compilation) struct NativeLibraries {
    pub(super) resolver: NativeUnitResolver,
    artifacts: Vec<(PackageArtifactInput, PackageImplementationArtifact)>,
}

impl Compilation {
    pub(super) fn mapped_product_runtime_demands(
        &self,
        product: &ProductIdentity,
        reachability: &ConcreteCodegenReachability,
        options: bray_codegen::CodegenOptions,
        runtime: Option<&bray_runtime_interface::RuntimeArtifactPlan>,
        allow_bitcode: bool,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            BTreeSet<NativeDemand>,
            bray_native_artifact::NativeUnitSelection,
        ),
        NativeProductPlanningError,
    > {
        let graph = reachability.graph();

        let mut demands = options
            .runtime_observations()
            .runtime_roles()
            .iter()
            .copied()
            .map(NativeDemand::native_runtime_role)
            .collect::<BTreeSet<_>>();

        for instance in graph.instances() {
            let key = instance.key();

            demands.extend(
                demanded_runtime_references_for_mir(instance.mir())
                    .into_iter()
                    .map(|reference| NativeDemand::runtime_role(key.clone(), reference.role())),
            );
        }

        let roots = graph.roots().iter().cloned().collect::<BTreeSet<_>>();
        let platform_overrides = super::link::runtime_platform_services(runtime).collect();

        let native_target = bray_target::NativeTarget::for_identity(target.identity())
            .expect("native demand planning requires a supported native target");

        let mut native_roots = Vec::new();
        let mut definitions = BTreeMap::new();

        for key in graph
            .instances()
            .iter()
            .map(bray_codegen::CodegenInstance::key)
            .chain(graph.external_instances())
        {
            let realization = reachability
                .instance(key)
                .expect("reachable callable must have a concrete realization");

            let (name, linkage, native_entry) = match graph.instance(key) {
                Some(instance) => self.codegen_emitted_symbol_boundary(
                    product,
                    instance,
                    &platform_overrides,
                    target,
                    &roots,
                    reachability,
                    cancellation,
                )?,
                None => match reachability.selected_native(key) {
                    Some(selected) => (selected.symbol.clone(), CodegenLinkage::Import, None),
                    None => self.codegen_callable_symbol_boundary(
                        product,
                        target,
                        realization,
                        self.codegen_native_boundary(key, &platform_overrides, cancellation)?,
                        CodegenLinkage::Import,
                        cancellation,
                    )?,
                },
            };

            let panic_report_context = self
                .codegen_instance_signature(realization, cancellation)?
                .has_panic_report_context();

            demands.extend(
                mapped_symbol_runtime_roles(native_entry.is_some(), panic_report_context)
                    .into_iter()
                    .map(|role| NativeDemand::runtime_role(key.clone(), role)),
            );

            if linkage == CodegenLinkage::Import {
                native_roots.push(bray_symbols::NativeSymbolContract::required_name(
                    bray_base::NonEmptySharedStr::try_new(
                        native_target.object_symbol_name(name.as_str()).as_ref(),
                    )
                    .expect("mapped native import must be nonempty"),
                ));
            } else if let Some(binding) = super::link::product_symbol_binding(linkage) {
                super::link::insert_product_definition(&mut definitions, name, binding);
            }

            if let Some(entry) = native_entry {
                super::link::insert_product_definition(
                    &mut definitions,
                    entry.name().clone(),
                    super::link::product_symbol_binding(entry.linkage())
                        .expect("emitted callback must have definition linkage"),
                );
            }

            if let Some(instance) = graph.instance(key)
                && let Some((symbol, binding)) = self.codegen_static_definition(
                    realization,
                    instance.mir(),
                    target,
                    cancellation,
                )?
            {
                let host = BinarySymbolName::try_new(
                    bray_codegen::CodegenStaticStorageMapping::host_name_for(&symbol),
                )
                .expect("generated static host name must be valid");

                super::link::insert_product_definition(&mut definitions, symbol, binding);

                super::link::insert_product_definition(
                    &mut definitions,
                    host,
                    bray_symbols::NativeSymbolBinding::Strong,
                );
            }
        }

        for reference in graph
            .instances()
            .iter()
            .flat_map(|instance| instance.mir().storages())
            .filter_map(|storage| match storage.kind() {
                bray_ir::MirStorageKind::NativeStatic(reference) => Some(reference),
                _ => None,
            })
        {
            let contract = self.native_static_contract(reference, cancellation)?;

            let name = BinarySymbolName::try_new(
                contract
                    .symbol
                    .identity()
                    .name()
                    .expect("checked native static must have a named symbol"),
            )
            .expect("checked native static must have a valid symbol name");

            match contract.direction {
                bray_symbols::ForeignCallableDirection::Import => {
                    if contract.symbol.presence() == bray_symbols::NativeSymbolPresence::Required {
                        native_roots.push(bray_symbols::NativeSymbolContract::required_name(
                            bray_base::NonEmptySharedStr::try_new(
                                native_target.object_symbol_name(name.as_str()).as_ref(),
                            )
                            .expect("mapped native import must be nonempty"),
                        ));
                    }
                }
                bray_symbols::ForeignCallableDirection::Export => {
                    super::link::insert_product_definition(
                        &mut definitions,
                        name,
                        contract.symbol.binding(),
                    );
                }
            }
        }

        let libraries = self.native_libraries(options, allow_bitcode)?;

        let provided = RuntimeAbiRole::ALL
            .into_iter()
            .filter(|_| runtime.is_none())
            .filter_map(|role| role.native_symbol())
            .chain([bray_codegen::LINKED_PRODUCT_HOST_SYMBOL])
            .map(|symbol| {
                (
                    bray_symbols::NativeSymbolIdentity::Name(
                        bray_base::NonEmptySharedStr::try_new(
                            native_target.object_symbol_name(symbol).as_ref(),
                        )
                        .expect("runtime symbol must be nonempty"),
                    ),
                    bray_symbols::NativeSymbolBinding::Strong,
                )
            })
            .chain(definitions.iter().map(|(name, &binding)| {
                (
                    bray_symbols::NativeSymbolIdentity::Name(
                        bray_base::NonEmptySharedStr::try_new(
                            native_target.object_symbol_name(name.as_str()).as_ref(),
                        )
                        .expect("mapped native definition must be nonempty"),
                    ),
                    binding,
                )
            }));

        let selected = libraries.select(
            native_roots,
            provided,
            [],
            runtime,
            self.native_link_inputs(),
        )?;

        let package_count = libraries.resolver.artifacts().len();

        let package_units = selected
            .units()
            .iter()
            .copied()
            .filter(|location| location.artifact < package_count)
            .collect::<Vec<_>>();

        demands.extend(
            libraries
                .runtime_roles(&package_units)
                .into_iter()
                .map(NativeDemand::native_runtime_role),
        );

        Ok((demands, selected))
    }

    pub(super) fn native_libraries(
        &self,
        options: CodegenOptions,
        bitcode: bool,
    ) -> Result<Arc<NativeLibraries>, NativeProductPlanningError> {
        let dependencies = self.dependency_interfaces();

        for ordinal in 0..dependencies.len() {
            self.record_dependency_implementation(
                bray_symbols::ImportedInterfaceId::try_from_index(ordinal)
                    .expect("loaded dependency must have an interface identity"),
            );
        }

        let inputs = dependencies
            .iter()
            .enumerate()
            .flat_map(|(ordinal, dependency)| {
                let interface = bray_symbols::ImportedInterfaceId::try_from_index(ordinal)
                    .expect("loaded dependency must have an interface identity");

                dependency
                    .implementation_input()
                    .into_iter()
                    .chain(dependency.native_implementations())
                    .map(move |input| (input, Some(interface)))
            })
            .chain(
                self.native_implementation_inputs()
                    .iter()
                    .map(|input| (input, None)),
            )
            .collect::<Vec<_>>();

        self.record_codegen_configuration();

        let key = (options, bitcode);

        if let Some(cached) = self
            .state
            .native_libraries
            .lock()
            .expect("native library cache mutex poisoned")
            .get(&key)
        {
            return Ok(Arc::clone(cached));
        }

        let configuration = self
            .package_implementation_configuration(None)
            .map_err(NativeProductPlanningError::InvalidCodegenTarget)?;

        let expected = self
            .state
            .codegen
            .as_ref()
            .map(|backend| backend.selected().toolchain_revision())
            .filter(|_| bitcode);

        let mut packages: BTreeMap<
            _,
            (
                NativeUnitKind,
                PackageArtifactInput,
                PackageImplementationArtifact,
                NativeArtifactIndex,
            ),
        > = BTreeMap::new();

        let mut seen = BTreeSet::new();

        for (input, interface) in inputs {
            if !seen.insert((input.path(), interface)) {
                continue;
            }

            let artifact = if let Some(interface) = interface.filter(|interface| {
                self.dependency_interface_input(*interface)
                    .expect("enumerated dependency must exist")
                    .implementation_input()
                    == Some(input)
            }) {
                let loaded = self
                    .loaded_dependency_implementation_with_cancellation(
                        interface,
                        &self.state.cancellation,
                    )?
                    .expect("enumerated dependency must have an implementation query");

                let Some(artifact) = loaded.value() else {
                    return Err(NativeProductPlanningError::Codegen(
                        crate::compilation::CodegenPreparationError::Diagnostics(
                            loaded.diagnostics().clone(),
                        ),
                    ));
                };

                // Package artifacts share their immutable bytes and decoded payload caches.
                artifact
                    .as_ref()
                    .map_err(|error| {
                        package_diagnostic(
                            input.path(),
                            error.clone().into_diagnostic(DiagnosticId::new(0)),
                        )
                    })?
                    .as_ref()
                    .clone()
            } else {
                let artifact = input.load_implementation().map_err(|error| {
                    package_diagnostic(input.path(), error.into_diagnostic(DiagnosticId::new(0)))
                })?;

                artifact
                    .validate_configuration(&configuration)
                    .map_err(|error| {
                        package_diagnostic(
                            input.path(),
                            error.into_diagnostic(DiagnosticId::new(0)),
                        )
                    })?;

                // Unattached native inputs can include variants of any loaded semantic package.
                // Foreign-only packages have no semantic interface to bind against.
                let interface = interface.or_else(|| {
                    dependencies
                        .iter()
                        .position(|dependency| {
                            dependency.package() == artifact.identity().interface().package()
                        })
                        .map(|ordinal| {
                            bray_symbols::ImportedInterfaceId::try_from_index(ordinal)
                                .expect("loaded dependency must have an interface identity")
                        })
                });

                if let Some(interface) = interface {
                    let loaded = self
                        .loaded_dependency_interface_with_cancellation(
                            interface,
                            &self.state.cancellation,
                        )?
                        .expect("native dependency must have a loaded semantic interface");

                    let (Some(validated), Some(surface)) = (loaded.validated(), loaded.surface())
                    else {
                        return Err(NativeProductPlanningError::Codegen(
                            crate::compilation::CodegenPreparationError::Diagnostics(
                                loaded.result().diagnostics().clone(),
                            ),
                        ));
                    };

                    artifact
                        .validate_interface(validated, surface)
                        .map_err(|error| {
                            package_diagnostic(
                                input.path(),
                                error.into_diagnostic(DiagnosticId::new(0)),
                            )
                        })?;
                }

                artifact
            };

            let Some((rank, index)) = artifact
                .native_representation(expected)
                .map_err(|error| native_package_error(input.path(), error))?
            else {
                continue;
            };

            let package = artifact.identity().interface().clone();

            if let Some((previous_rank, previous_input, previous_artifact, previous_index)) =
                packages.get(&package)
            {
                if artifact.identity() != previous_artifact.identity()
                    || index.producer() != previous_index.producer()
                    || rank == *previous_rank && index != *previous_index
                {
                    return Err(NativeProductPlanningError::ConflictingNativeArtifacts {
                        first: previous_input.path().to_owned(),
                        second: input.path().to_owned(),
                    });
                }
            }

            if packages
                .get(&package)
                .is_none_or(|(previous, ..)| rank > *previous)
            {
                // The library cache owns the immutable input handle after request enumeration.
                packages.insert(package, (rank, input.clone(), artifact, index));
            }
        }

        let mut artifacts = Vec::new();
        let mut indexes = Vec::new();

        for (_, input, artifact, index) in packages.into_values() {
            artifacts.push((input, artifact));
            indexes.push(index);
        }

        let libraries = Arc::new(NativeLibraries {
            resolver: NativeUnitResolver::new(indexes),
            artifacts,
        });

        let mut cache = self
            .state
            .native_libraries
            .lock()
            .expect("native library cache mutex poisoned");

        Ok(Arc::clone(cache.entry(key).or_insert(libraries)))
    }
}

impl NativeLibraries {
    pub(super) fn contains(
        &self,
        package: &bray_package_interface::PackageInterfaceIdentity,
    ) -> bool {
        self.artifacts
            .binary_search_by(|(_, artifact)| artifact.identity().interface().cmp(package))
            .is_ok()
    }

    pub(super) fn select(
        &self,
        demands: impl IntoIterator<Item = bray_symbols::NativeSymbolContract>,
        provided: impl IntoIterator<
            Item = (
                bray_symbols::NativeSymbolIdentity,
                bray_symbols::NativeSymbolBinding,
            ),
        >,
        static_demands: impl IntoIterator<Item = [u8; 32]>,
        runtime: Option<&bray_runtime_interface::RuntimeArtifactPlan>,
        native_links: &[bray_symbols::NativeLinkRequirement],
    ) -> Result<bray_native_artifact::NativeUnitSelection, NativeProductPlanningError> {
        let resolver = runtime.map(|runtime| {
            // Shared summaries retain the identity of every independent producer.
            NativeUnitResolver::new(
                self.resolver
                    .artifacts()
                    .iter()
                    .cloned()
                    .chain([runtime.native_index().clone()]),
            )
        });

        let resolver = resolver.as_ref().unwrap_or(&self.resolver);

        let demands = demands.into_iter().chain(
            runtime
                .into_iter()
                .flat_map(|runtime| runtime.native_demands().iter().cloned()),
        );

        let provided = provided.into_iter().chain(runtime.into_iter().flat_map(|runtime| {
            super::link::runtime_platform_services(Some(runtime)).filter_map(|role| {
                let symbol = runtime.native_index().target().object_symbol_name(role.native_symbol());

                if runtime.native_index().units().iter().any(|unit| {
                    matches!(unit.summary(), bray_native_artifact::NativeUnitSummary::Exact { definitions, .. }
                        if definitions.iter().any(|definition| definition.symbol().identity().name() == Some(symbol.as_ref())))
                }) {
                    return None;
                }

                // The selected foreign archive supplies this declared ABI override. Its opaque
                // summary cannot index it, so prevent weak fallbacks from satisfying the reference.
                Some((
                    bray_symbols::NativeSymbolIdentity::Name(
                        bray_base::NonEmptySharedStr::try_new(symbol.as_ref())
                            .expect("runtime platform catalog must publish nonempty native names"),
                    ),
                    bray_symbols::NativeSymbolBinding::Strong,
                ))
            })
        }));

        resolver
            .select_with_provided(demands, provided, static_demands, native_links)
            .map_err(NativeProductPlanningError::NativeResolution)
    }

    pub(super) fn path(&self, location: NativeUnitLocation) -> &Path {
        self.artifacts[location.artifact].0.path()
    }

    pub(super) fn package(&self, location: NativeUnitLocation) -> &PackageIdentity {
        self.artifacts[location.artifact]
            .1
            .identity()
            .interface()
            .package()
    }

    pub(super) fn unit(&self, location: NativeUnitLocation) -> &bray_native_artifact::NativeUnit {
        let index = &self.resolver.artifacts()[location.artifact];

        &index.units()[index
            .units()
            .binary_search_by_key(&location.digest, |unit| unit.digest())
            .expect("selected unit must belong to its artifact")]
    }

    pub(super) fn runtime_roles(
        &self,
        units: &[NativeUnitLocation],
    ) -> BTreeSet<bray_runtime_interface::RuntimeAbiRole> {
        let mut roles = BTreeSet::new();

        for &location in units {
            for reference in self.unit(location).summary().references() {
                roles.extend(
                    bray_runtime_interface::RuntimeAbiRole::ALL
                        .into_iter()
                        .filter(|role| {
                            role.native_symbol().is_some_and(|name| {
                                reference.identity().name().is_some_and(|actual| {
                                    actual
                                        == self.resolver.artifacts()[location.artifact]
                                            .target()
                                            .object_symbol_name(name)
                                })
                            })
                        }),
                );
            }

            if matches!(
                self.unit(location).summary(),
                bray_native_artifact::NativeUnitSummary::Opaque { .. }
            ) {
                roles.extend(
                    self.artifacts[location.artifact]
                        .1
                        .identity()
                        .runtime_requirements()
                        .iter()
                        .flat_map(|requirements| requirements.roles())
                        .copied(),
                );
            }
        }

        roles
    }

    pub(super) fn payload(
        &self,
        location: NativeUnitLocation,
    ) -> Result<SelectedNativePayload, NativeProductPlanningError> {
        let (input, artifact) = &self.artifacts[location.artifact];

        let unit = self.unit(location);

        let bytes = artifact
            .native_unit_bytes(location.digest.bytes())
            .map_err(|error| native_package_error(input.path(), error))?
            .expect("validated native index must name an embedded payload");

        let package = self.package(location).clone();

        let native_links = unit
            .native_links()
            .iter()
            .map(|link| {
                super::link::native_link_input(
                    link,
                    bray_linker::LinkInputProvenance::Package(package.clone()),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SelectedNativePayload {
            package,
            digest: location.digest,
            kind: unit.kind(),
            bytes,
            native_links: native_links.into(),
        })
    }
}

pub(super) fn native_package_error(
    path: &Path,
    error: PackageNativeArtifactError,
) -> NativeProductPlanningError {
    package_diagnostic(
        path,
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::InterfaceValidationFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::interface_validation_failure(
            error.into_diagnostic_failure(),
        )),
    )
}

fn package_diagnostic(path: &Path, diagnostic: Diagnostic) -> NativeProductPlanningError {
    NativeProductPlanningError::Codegen(crate::compilation::CodegenPreparationError::Diagnostics(
        DiagnosticBag::single(
            diagnostic
                .with_arg(DiagnosticArg::file_path(path))
                .with_arg(DiagnosticArg::artifact_path(path)),
        ),
    ))
}
