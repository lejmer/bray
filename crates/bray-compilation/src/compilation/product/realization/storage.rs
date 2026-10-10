use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_codegen::{
    CodegenInstanceKey, CodegenStaticInstanceKey, CodegenTarget,
    demanded_runtime_references_for_mir,
};
use bray_ir::{MirStorageKind, MirUnit};
use bray_runtime_interface::{
    BinarySymbolName, ExecutionLaneRequirement, RuntimeArtifact, RuntimeArtifactPlan,
};
use bray_symbols::{StaticReferenceSelection, TypeId};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use crate::fact::CancellationToken;

pub(in crate::compilation::product) type NativeCallableEffects =
    BTreeMap<CodegenInstanceKey, Arc<CallableEffects>>;

#[derive(Debug, Default, Hash)]
pub(in crate::compilation::product) struct CallableEffects {
    pub(in crate::compilation::product) accesses: BTreeSet<CodegenStaticInstanceKey>,
    pub(in crate::compilation::product) requires_main_thread: bool,
}

impl CallableEffects {
    fn include(&mut self, other: &Self) {
        self.accesses.extend(other.accesses.iter().cloned());
        self.requires_main_thread |= other.requires_main_thread;
    }

    fn include_native_accesses(
        &mut self,
        identities: &[[u8; 32]],
        statics: &BTreeMap<[u8; 32], &bray_native_artifact::NativeStatic>,
        target: &bray_ir::MirTargetContract,
    ) {
        for identity in identities {
            let entry = statics
                .get(identity)
                .expect("resolved native accesses must retain their static metadata");

            self.accesses.insert(CodegenStaticInstanceKey::new(
                Arc::from(entry.order_key()),
                target.clone(),
                entry.duration(),
            ));
        }
    }
}

impl Compilation {
    pub(in crate::compilation::product) fn codegen_static_definition(
        &self,
        owner: &ConcreteCodegenInstance,
        mir: &MirUnit,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<
        Option<(BinarySymbolName, bray_symbols::NativeSymbolBinding)>,
        CodegenPreparationError,
    > {
        let Some(declaration) = owner.static_declaration() else {
            return Ok(None);
        };

        let reference = mir
            .storages()
            .iter()
            .find_map(|storage| match storage.kind() {
                MirStorageKind::Static(reference) | MirStorageKind::NativeStatic(reference)
                    if reference.template().declaration() == declaration =>
                {
                    Some(reference)
                }
                _ => None,
            })
            .expect("emitted static initializer must contain its own storage");

        let (instance, _, _) =
            self.concrete_codegen_static_selection(owner, reference, cancellation)?;

        let (_, symbol) =
            self.codegen_static_identity(owner, reference, &instance, target, cancellation)?;

        let binding = self
            .optional_native_static_contract(reference, cancellation)?
            .map_or(bray_symbols::NativeSymbolBinding::Strong, |native| {
                native.symbol.binding()
            });

        Ok(Some((symbol, binding)))
    }

    pub(in crate::compilation::product) fn native_callable_effects(
        &self,
        reachability: &ConcreteCodegenReachability,
        native: &bray_native_artifact::NativeUnitSelection,
        runtime: Option<&RuntimeArtifact>,
        cancellation: &CancellationToken,
    ) -> Result<NativeCallableEffects, CodegenPreparationError> {
        let graph = reachability.graph();

        let keys = graph
            .instances()
            .iter()
            .map(|instance| instance.key())
            .chain(graph.external_instances())
            .collect::<Vec<_>>();

        let positions = keys
            .iter()
            .enumerate()
            .map(|(index, key)| (*key, index))
            .collect::<BTreeMap<_, _>>();

        let native_statics = native
            .statics()
            .iter()
            .map(|entry| (entry.identity(), entry))
            .collect::<BTreeMap<_, _>>();

        let mut direct = Vec::with_capacity(keys.len());
        let mut edges = Vec::with_capacity(keys.len());

        for key in &keys {
            cancellation.check()?;

            let mut effects = CallableEffects::default();
            let mut dependencies = Vec::new();

            if let Some(selected) = reachability.selected_native(key) {
                effects.requires_main_thread = selected.requires_main_thread;

                let target = bray_target::NativeTarget::for_identity(key.target().identity())
                    .expect("selected native callable must have a native target");

                let symbol = bray_symbols::NativeSymbolContract::required_name(
                    bray_base::NonEmptySharedStr::try_new(
                        target.object_symbol_name(selected.symbol.as_str()).as_ref(),
                    )
                    .expect("selected native callable must have a nonempty symbol"),
                );

                let accesses = native
                    .static_accesses(&symbol)
                    .expect("selected native callable must be a resolved native demand");

                effects.include_native_accesses(accesses, &native_statics, key.target());
            } else if let Some(instance) = graph.instance(key) {
                effects.requires_main_thread =
                    instance.mir().frame_descriptor().is_some_and(|frame| {
                        frame.states().iter().any(|state| {
                            state
                                .lane_requirements()
                                .contains(&ExecutionLaneRequirement::MainThread)
                        })
                    });

                let owner = reachability
                    .instance(key)
                    .expect("reachable MIR must have its concrete realization");

                if let Some(runtime) = runtime {
                    let target = bray_target::NativeTarget::for_identity(key.target().identity())
                        .expect("native runtime effects require a native target");

                    for reference in demanded_runtime_references_for_mir(instance.mir()) {
                        let Some(binding) = runtime.contract().role_binding(reference.role())
                        else {
                            continue;
                        };

                        let symbol = bray_symbols::NativeSymbolContract::required_name(
                            bray_base::NonEmptySharedStr::try_new(
                                target
                                    .object_symbol_name(binding.symbol_name().as_str())
                                    .as_ref(),
                            )
                            .expect("validated runtime binding must have a nonempty symbol"),
                        );

                        // Newly mapped roles enter the host demand graph on the next closure pass.
                        let Some(accesses) = native.static_accesses(&symbol) else {
                            continue;
                        };

                        effects.include_native_accesses(accesses, &native_statics, key.target());
                    }
                }

                for storage in instance.mir().storages() {
                    if let Some(reference) =
                        self.codegen_static_reference(storage.kind(), cancellation)?
                    {
                        let (instance, _, _) =
                            self.concrete_codegen_static_selection(owner, reference, cancellation)?;

                        let template =
                            self.static_instance_template(instance.template().declaration())?;

                        effects.accesses.insert(CodegenStaticInstanceKey::new(
                            self.static_cleanup_order_key(&instance, cancellation)?,
                            key.target().clone(),
                            template.value().duration(),
                        ));
                    }
                }

                dependencies.extend(
                    instance
                        .dependencies()
                        .iter()
                        .map(|dependency| positions[dependency.instance()]),
                );
            } else {
                assert!(
                    graph.is_external(key),
                    "reachable callable must have MIR or an external binding"
                );
            }

            direct.push(effects);
            edges.push(dependencies);
        }

        let components = bray_base::strongly_connected_components(0..keys.len(), |index| {
            edges[index].iter().copied()
        });

        let mut component_of = vec![0; keys.len()];

        for (component, members) in components.iter().enumerate() {
            for &member in members {
                component_of[member] = component;
            }
        }

        let mut summaries = BTreeMap::<usize, Arc<CallableEffects>>::new();
        let mut result = BTreeMap::new();

        // The SCC order places callers before callees. Recursive callables share one summary.
        for (component, members) in components.iter().enumerate().rev() {
            cancellation.check()?;

            let mut effects = CallableEffects::default();
            let mut dependencies = BTreeSet::new();

            for &member in members {
                let own = std::mem::take(&mut direct[member]);

                effects.accesses.extend(own.accesses);
                effects.requires_main_thread |= own.requires_main_thread;

                dependencies.extend(
                    edges[member]
                        .iter()
                        .map(|&callee| component_of[callee])
                        .filter(|&callee| callee != component),
                );
            }

            for dependency in dependencies {
                effects.include(summaries.get(&dependency).expect(
                    "callee component must precede its caller during reverse SCC traversal",
                ));
            }

            let effects = Arc::new(effects);

            for &member in members {
                result.insert(keys[member].clone(), Arc::clone(&effects));
            }

            summaries.insert(component, effects);
        }

        Ok(result)
    }

    pub(in crate::compilation::product) fn product_static_host_entries(
        &self,
        product_kind: bray_symbols::ProductKind,
        reachability: &ConcreteCodegenReachability,
        callable_effects: &NativeCallableEffects,
        runtime: Option<&RuntimeArtifactPlan>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ProductStaticHostEntry>, CodegenPreparationError> {
        let runtime_providers = runtime
            .into_iter()
            .flat_map(|runtime| runtime.native_index().units())
            .flat_map(bray_native_artifact::NativeUnit::statics)
            .map(bray_native_artifact::NativeStatic::order_key)
            .collect::<BTreeSet<_>>();

        // Host graph tables own their Arc-backed static keys independently of reachability.
        let mut realized = BTreeMap::new();

        for instance in reachability.graph().instances() {
            let owner = reachability
                .instance(instance.key())
                .expect("reachable static consumer must have a concrete realization");

            for storage in instance.mir().storages() {
                let Some(reference) =
                    self.codegen_static_reference(storage.kind(), cancellation)?
                else {
                    continue;
                };

                let static_instance =
                    self.concrete_codegen_static(owner, &reference, target, cancellation)?;

                realized
                    .entry(static_instance.key.clone())
                    .or_insert(static_instance);
            }
        }

        let mut entries = realized
            .iter()
            .map(|(consumer_key, consumer)| {
                let cleanup_roots = consumer
                    .finalization
                    .iter()
                    .map(|finalization| finalization.instance.key().clone())
                    .chain(consumer.destroy.iter().map(|destroy| destroy.key().clone()));

                let mut cleanup = CallableEffects::default();

                for key in cleanup_roots {
                    cleanup.include(&callable_effects[&key]);
                }

                let requires_main_thread = cleanup.requires_main_thread;
                let mut accesses = cleanup.accesses;

                accesses.extend(
                    callable_effects[consumer.initializer.key()]
                        .accesses
                        .iter()
                        .cloned(),
                );

                let declarations = consumer
                    .lifecycle_dependencies
                    .iter()
                    .map(|declaration| {
                        self.static_declaration_order_prefix(*declaration, cancellation)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                // Runtime bindings close in the final product, whereas reusable static records
                // retain their source-declared lifecycle dependencies.
                let runtime_dependencies = accesses
                    .iter()
                    .filter(|provider| {
                        *provider != consumer_key
                            && runtime_providers.contains(provider.order_key())
                    })
                    .cloned()
                    .collect();

                accesses.retain(|provider| {
                    declarations
                        .iter()
                        .any(|prefix| provider.order_key().starts_with(prefix))
                });

                Ok((
                    consumer_key.clone(),
                    ProductStaticHostEntry {
                        key: consumer_key.clone(),
                        reference: consumer.reference.clone(),
                        ty: consumer.ty,
                        dependencies: accesses.into_iter().collect(),
                        runtime_dependencies,
                        requires_host: consumer.requires_host(),
                        requires_main_thread_cleanup: requires_main_thread,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, CodegenPreparationError>>()?;

        let retained = bray_base::transitive_dependencies(
            entries
                .iter()
                .filter(|(_, instance)| {
                    product_kind == bray_symbols::ProductKind::Library || instance.requires_host()
                })
                .map(|(key, _)| key.clone()),
            |key| {
                entries[key]
                    .dependencies
                    .iter()
                    .chain(entries[key].runtime_dependencies.iter())
                    .filter(|provider| realized.contains_key(*provider))
                    .cloned()
            },
        );

        entries.retain(|key, _| retained.contains(key));

        let local_dependencies = entries
            .iter()
            .map(|(key, entry)| {
                (
                    key.clone(),
                    entry
                        .dependencies
                        .iter()
                        .filter(|provider| retained.contains(*provider))
                        .cloned()
                        .collect(),
                )
            })
            .collect();

        let keys = entries
            .keys()
            .map(|key| (key.clone(), key.order_key()))
            .collect();

        let ordered = super::super::structural_order::dependency_order(&local_dependencies, &keys)
            .expect("semantically checked static lifecycle dependencies must be acyclic");

        let entries = ordered
            .into_iter()
            .map(|key| {
                entries
                    .remove(&key)
                    .expect("ordered source static must have its host entry")
            })
            .collect();

        Ok(entries)
    }
}

#[derive(Eq, PartialEq)]
pub(in crate::compilation::product) struct ProductStaticHostEntry {
    key: CodegenStaticInstanceKey,
    reference: StaticReferenceSelection,
    ty: TypeId,
    dependencies: Vec<CodegenStaticInstanceKey>,
    runtime_dependencies: Vec<CodegenStaticInstanceKey>,
    requires_host: bool,
    requires_main_thread_cleanup: bool,
}

impl ProductStaticHostEntry {
    pub(in crate::compilation::product) const fn key(&self) -> &CodegenStaticInstanceKey {
        &self.key
    }

    pub(in crate::compilation::product) fn order_key(&self) -> &[u8] {
        self.key.order_key()
    }

    pub(in crate::compilation::product) fn dependencies(&self) -> &[CodegenStaticInstanceKey] {
        &self.dependencies
    }

    pub(in crate::compilation::product) fn runtime_dependencies(
        &self,
    ) -> &[CodegenStaticInstanceKey] {
        &self.runtime_dependencies
    }

    pub(in crate::compilation::product) const fn requires_host(&self) -> bool {
        self.requires_host
    }

    pub(in crate::compilation::product) const fn requires_main_thread_cleanup(&self) -> bool {
        self.requires_main_thread_cleanup
    }

    pub(in crate::compilation::product) fn lowering_entry(
        &self,
    ) -> bray_lowering::ExecutableHostStatic {
        // Lowered host MIR owns the Arc-backed static reference after planning returns.
        bray_lowering::ExecutableHostStatic::new(self.reference.clone(), self.ty)
    }
}
