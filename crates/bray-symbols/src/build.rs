use std::collections::BTreeMap;
use std::sync::Arc;

use bray_declarations::{
    ContainerId, ContainerKind, DeclarationId, DeclarationKind, DeclarationRecord,
    DeclarationSurface, DeclarationTable,
};

use crate::allocator::SymbolIdAllocator;
use crate::graph::{DefaultProviderRecord, SymbolGraphBuilder, SymbolGraphRoots};
use crate::record::{
    CallableParameterDefaultProviderSymbol, DeclarationSymbolIdentity, ModuleSymbol,
    ModuleSymbolInput, PackageSymbol, ReceiverParameterSymbol, StructFieldDefaultProviderSymbol,
    UnionPayloadDefaultProviderSymbol, for_each_declaration_symbol,
};
use crate::surface_kind::declaration_symbol_kind;
use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbolId, CallableSymbolId,
    CompilerKnownSymbolProvider, MemberEntry, MemberValidity, MemberVisibility, ModuleOwnerId,
    ModulePathKey, ModuleSymbolId, PackageIdentity, PackageSymbolId, ReceiverParameterSymbolId,
    StructFieldDefaultProviderSymbolId, SymbolGraph, SymbolGraphBuildError, SymbolId, SymbolKey,
    SymbolKind, SymbolName, SymbolOrigin, SymbolRootKey, SynthesizedSymbolKey,
    UnionPayloadDefaultProviderSymbolId,
};

pub(crate) fn build_source_symbol_graph(
    package_identity: PackageIdentity,
    declarations: &DeclarationTable,
    compiler_known: Arc<CompilerKnownSymbolProvider>,
) -> Result<SymbolGraph, SymbolGraphBuildError> {
    let mut allocator = SymbolIdAllocator::starting_at(compiler_known.next_symbol_index());

    let roots = build_roots_and_modules(
        package_identity,
        declarations,
        &compiler_known,
        &mut allocator,
    )?;

    let container_declarations = containing_declarations(declarations);

    let mut graph = SymbolGraphBuilder::new(
        roots.roots,
        compiler_known,
        vec![roots.package],
        roots.modules,
    );

    push_source_symbols(
        &mut graph,
        declarations,
        &roots.module_owners,
        &container_declarations,
        &mut allocator,
    )?;

    match graph.finish() {
        Ok(graph) => Ok(graph),
        Err((declaration_id, symbol_kind)) => {
            let declaration_kind = declarations
                .declaration(declaration_id)
                .map_or(DeclarationKind::Module, DeclarationRecord::kind);

            Err(SymbolGraphBuildError::InvalidSourceSymbolKind {
                declaration: declaration_id,
                declaration_kind,
                symbol_kind,
            })
        }
    }
}

struct RootSkeleton {
    roots: SymbolGraphRoots,
    package: PackageSymbol,
    modules: Vec<ModuleSymbol>,
    module_owners: BTreeMap<ContainerId, OwnerIdentity>,
}

fn build_roots_and_modules(
    package_identity: PackageIdentity,
    declarations: &DeclarationTable,
    compiler_known: &CompilerKnownSymbolProvider,
    allocator: &mut SymbolIdAllocator,
) -> Result<RootSkeleton, SymbolGraphBuildError> {
    let compiler_known_id = compiler_known.environment().id();
    let package_id = PackageSymbolId::from_symbol_id(allocator.next()?);

    // Package identities and root keys share immutable text storage across graph records.
    let package_root_key = SymbolRootKey::Package(package_identity.clone());
    let package_key = SymbolKey::package(package_identity.clone());

    let mut module_ids = Vec::new();

    // Compiler-known module records are immutable and small; graph storage clones them so its
    // existing ordered all-module view remains contiguous.
    let mut modules = compiler_known.modules().to_vec();
    let mut module_owners = BTreeMap::new();

    for container in declarations.module_containers() {
        let id = ModuleSymbolId::from_symbol_id(allocator.next()?);
        let path = module_path_key(declarations, container.id())?;

        // Structured keys share immutable owner and path storage with their records and indexes.
        let key = SymbolKey::module(package_root_key.clone(), path.clone());
        let owner = ModuleOwnerId::from(package_id);

        let declaration_ids = module_declaration_ids(declarations, container.id())?;
        let visibility = module_visibility(declarations, container.id())?;
        let is_recovered = module_is_recovered(declarations, container.id())?;

        module_ids.push(id);

        // Immediate-owner lookup retains the same cheaply shared structured key.
        module_owners.insert(
            container.id(),
            OwnerIdentity {
                id: id.into(),
                key: key.clone(),
            },
        );

        modules.push(ModuleSymbol::new(ModuleSymbolInput {
            id,
            key,
            owner,
            path,
            origin: SymbolOrigin::Source,
            visibility,
            declarations: declaration_ids,
            module_parts: container.module_parts().into(),
            is_recovered,
        }));
    }

    let package = PackageSymbol::new(
        package_id,
        package_key,
        package_identity,
        SymbolOrigin::Source,
        module_ids.into_boxed_slice(),
    );

    Ok(RootSkeleton {
        roots: SymbolGraphRoots::new(compiler_known_id, vec![package_id].into_boxed_slice()),
        package,
        modules,
        module_owners,
    })
}

fn push_source_symbols(
    graph: &mut SymbolGraphBuilder,
    declarations: &DeclarationTable,
    module_owners: &BTreeMap<ContainerId, OwnerIdentity>,
    container_declarations: &BTreeMap<ContainerId, DeclarationId>,
    allocator: &mut SymbolIdAllocator,
) -> Result<(), SymbolGraphBuildError> {
    let mut source_identities = BTreeMap::new();

    for declaration in declarations.declarations() {
        if !declaration_creates_symbol(declaration.kind()) {
            continue;
        }

        let owner = declaration_owner(
            declaration,
            declarations,
            module_owners,
            container_declarations,
            &source_identities,
        )?;

        let Ok(surface_kind) = declaration.kind().try_into() else {
            continue;
        };

        let symbol_kind = declaration_symbol_kind(surface_kind, owner.id.kind());

        let raw_id = allocator.next()?;

        let id = match declaration_symbol_id(raw_id, symbol_kind) {
            Some(id) => id,
            None => {
                return Err(SymbolGraphBuildError::InvalidSourceSymbolKind {
                    declaration: declaration.id(),
                    declaration_kind: declaration.kind(),
                    symbol_kind,
                });
            }
        };

        // Source keys form an immutable owner chain and therefore share their owner key storage.
        let key =
            match SymbolKey::source_declaration(owner.key.clone(), symbol_kind, declaration.id()) {
                Some(key) => key,
                None => {
                    return Err(SymbolGraphBuildError::InvalidSourceSymbolKind {
                        declaration: declaration.id(),
                        declaration_kind: declaration.kind(),
                        symbol_kind,
                    });
                }
            };

        // Child identities need the same structured key after the record takes ownership.
        source_identities.insert(
            declaration.id(),
            OwnerIdentity {
                id,
                key: key.clone(),
            },
        );

        graph.map_declaration(declaration.id(), id);

        // Synthesized children retain the same Arc-backed subject key after identity publication.
        let synthesized_subject_key = key.clone();

        let identity = DeclarationSymbolIdentity::new(
            key,
            owner.id,
            declaration.id(),
            declaration.syntax_anchor(),
        );

        if let Some(member) = source_member_entry(declaration, id) {
            graph.add_member(owner.id, member);
        }

        if let Err(symbol_kind) = graph.push_source(id, identity) {
            return Err(SymbolGraphBuildError::InvalidSourceSymbolKind {
                declaration: declaration.id(),
                declaration_kind: declaration.kind(),
                symbol_kind,
            });
        }

        if let Some(owner) = receiver_owner(id, declaration.surface().has_static_modifier()) {
            let receiver_id = ReceiverParameterSymbolId::from_symbol_id(allocator.next()?);
            let receiver_key = SymbolKey::synthesized(SynthesizedSymbolKey::receiver_parameter(
                synthesized_subject_key.clone(),
            ));

            graph.push_receiver(ReceiverParameterSymbol::new(
                receiver_id,
                receiver_key,
                owner,
            ));
        }

        if let Some(default) = declaration.surface().runtime_default() {
            graph.add_runtime_default(id, default);

            if let Some(provider) =
                default_provider_record(id, synthesized_subject_key, owner.id, allocator)?
            {
                graph.push_default_provider(provider);
            }
        }

        if !declaration.surface().overload_arms().is_empty() {
            graph.add_overload_arms(id, declaration.surface().overload_arms().into());
        }
    }

    Ok(())
}

fn source_member_entry(
    declaration: &DeclarationRecord,
    symbol: AnySymbolId,
) -> Option<MemberEntry<AnySymbolId>> {
    let name = SymbolName::try_new(declaration.name()?.as_identifier()?)?;

    let visibility = surface_visibility(declaration.surface());

    let validity = if declaration.is_recovered() {
        MemberValidity::Malformed
    } else {
        MemberValidity::Valid
    };

    Some(MemberEntry::new(symbol, name, visibility, validity))
}

fn receiver_owner(symbol: AnySymbolId, is_static: bool) -> Option<CallableSymbolId> {
    match symbol {
        AnySymbolId::TypeCallableMember(id) if !is_static => Some(id.into()),
        AnySymbolId::TraitCallableMember(id) if !is_static => Some(id.into()),
        AnySymbolId::TraitCallableFulfillment(id) if !is_static => Some(id.into()),
        AnySymbolId::Finalizer(id) => Some(id.into()),
        AnySymbolId::Destructor(id) => Some(id.into()),
        AnySymbolId::ScopeEnter(id) => Some(id.into()),
        AnySymbolId::ScopeExit(id) => Some(id.into()),
        AnySymbolId::TraitFinalizerRequirement(id) => Some(id.into()),
        AnySymbolId::TraitDestructorRequirement(id) => Some(id.into()),
        AnySymbolId::TraitScopeEnterRequirement(id) => Some(id.into()),
        AnySymbolId::TraitScopeExitRequirement(id) => Some(id.into()),
        AnySymbolId::TraitScopeEnterFulfillment(id) => Some(id.into()),
        AnySymbolId::TraitScopeExitFulfillment(id) => Some(id.into()),
        _ => None,
    }
}

fn default_provider_record(
    subject: AnySymbolId,
    subject_key: SymbolKey,
    containing_symbol: AnySymbolId,
    allocator: &mut SymbolIdAllocator,
) -> Result<Option<DefaultProviderRecord>, SymbolGraphBuildError> {
    let raw_id = allocator.next()?;

    let provider = match subject {
        AnySymbolId::CallableParameter(subject) => {
            let id = CallableParameterDefaultProviderSymbolId::from_symbol_id(raw_id);
            let key = SymbolKey::synthesized(
                SynthesizedSymbolKey::callable_parameter_default_provider(subject_key),
            );

            DefaultProviderRecord::CallableParameter(CallableParameterDefaultProviderSymbol::new(
                id,
                key,
                containing_symbol,
                subject,
            ))
        }
        AnySymbolId::StructField(subject) => {
            let id = StructFieldDefaultProviderSymbolId::from_symbol_id(raw_id);
            let key = SymbolKey::synthesized(SynthesizedSymbolKey::struct_field_default_provider(
                subject_key,
            ));

            DefaultProviderRecord::StructField(StructFieldDefaultProviderSymbol::new(
                id,
                key,
                containing_symbol,
                subject,
            ))
        }
        AnySymbolId::UnionPayloadField(subject) => {
            let id = UnionPayloadDefaultProviderSymbolId::from_symbol_id(raw_id);
            let key = SymbolKey::synthesized(SynthesizedSymbolKey::union_payload_default_provider(
                subject_key,
            ));

            DefaultProviderRecord::UnionPayload(UnionPayloadDefaultProviderSymbol::new(
                id,
                key,
                containing_symbol,
                subject,
            ))
        }
        _ => return Ok(None),
    };

    Ok(Some(provider))
}

#[derive(Clone)]
struct OwnerIdentity {
    id: AnySymbolId,
    key: SymbolKey,
}

macro_rules! define_source_symbol_allocator {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        pub(crate) fn declaration_symbol_id(id: SymbolId, kind: SymbolKind) -> Option<AnySymbolId> {
            match kind {
                $(
                    SymbolKind::$variant => Some(crate::$id::from_symbol_id(id).into()),
                )+
                _ => None,
            }
        }
    };
}

for_each_declaration_symbol!(define_source_symbol_allocator);

fn declaration_creates_symbol(kind: DeclarationKind) -> bool {
    !matches!(
        kind,
        DeclarationKind::Module | DeclarationKind::Using | DeclarationKind::Export
    )
}

fn containing_declarations(table: &DeclarationTable) -> BTreeMap<ContainerId, DeclarationId> {
    table
        .declarations()
        .iter()
        .filter_map(|declaration| {
            declaration
                .child_container()
                .map(|container| (container, declaration.id()))
        })
        .collect()
}

fn declaration_owner(
    declaration: &DeclarationRecord,
    table: &DeclarationTable,
    modules: &BTreeMap<ContainerId, OwnerIdentity>,
    containing_declarations: &BTreeMap<ContainerId, DeclarationId>,
    source_identities: &BTreeMap<DeclarationId, OwnerIdentity>,
) -> Result<OwnerIdentity, SymbolGraphBuildError> {
    let container_id = declaration.owning_container();

    let Some(container) = table.container(container_id) else {
        return Err(SymbolGraphBuildError::MissingContainer {
            declaration: declaration.id(),
            container: container_id,
        });
    };

    if container.kind() == ContainerKind::Module {
        let Some(owner) = modules.get(&container_id) else {
            return Err(SymbolGraphBuildError::MissingModuleOwner {
                declaration: declaration.id(),
                container: container_id,
            });
        };

        // Owner identities contain Arc-backed structured keys and are cheap to share.
        return Ok(owner.clone());
    }

    let Some(containing_declaration) = containing_declarations.get(&container_id).copied() else {
        return Err(SymbolGraphBuildError::MissingContainingDeclaration {
            declaration: declaration.id(),
            container: container_id,
        });
    };

    let Some(owner) = source_identities.get(&containing_declaration) else {
        return Err(SymbolGraphBuildError::MissingContainingSymbol {
            declaration: declaration.id(),
            containing_declaration,
        });
    };

    // Owner identities contain Arc-backed structured keys and are cheap to share.
    Ok(owner.clone())
}

fn module_path_key(
    table: &DeclarationTable,
    container_id: ContainerId,
) -> Result<ModulePathKey, SymbolGraphBuildError> {
    let Some(container) = table.container(container_id) else {
        return Err(SymbolGraphBuildError::MissingModulePath {
            container: container_id,
        });
    };

    let Some(path) = container.module_path() else {
        return Err(SymbolGraphBuildError::MissingModulePath {
            container: container_id,
        });
    };

    if let Some(path) = ModulePathKey::try_new(path.segments().iter().map(String::as_str)) {
        return Ok(path);
    }

    let Some(module_part_id) = container.module_parts().first().copied() else {
        return Err(SymbolGraphBuildError::MissingRecoveredModuleAnchor {
            container: container_id,
        });
    };

    let Some(module_part) = table.module_part(module_part_id) else {
        return Err(SymbolGraphBuildError::MissingModulePart {
            container: container_id,
            module_part: module_part_id,
        });
    };

    Ok(ModulePathKey::recovered(module_part.declaration()))
}

fn module_declaration_ids(
    table: &DeclarationTable,
    container_id: ContainerId,
) -> Result<Box<[DeclarationId]>, SymbolGraphBuildError> {
    let Some(container) = table.container(container_id) else {
        return Err(SymbolGraphBuildError::MissingModulePath {
            container: container_id,
        });
    };

    container
        .module_parts()
        .iter()
        .map(|module_part_id| {
            table
                .module_part(*module_part_id)
                .map(bray_declarations::ModulePartRecord::declaration)
                .ok_or(SymbolGraphBuildError::MissingModulePart {
                    container: container_id,
                    module_part: *module_part_id,
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Vec::into_boxed_slice)
}

fn module_is_recovered(
    table: &DeclarationTable,
    container_id: ContainerId,
) -> Result<bool, SymbolGraphBuildError> {
    let Some(container) = table.container(container_id) else {
        return Err(SymbolGraphBuildError::MissingModulePath {
            container: container_id,
        });
    };

    for module_part_id in container.module_parts() {
        let Some(module_part) = table.module_part(*module_part_id) else {
            return Err(SymbolGraphBuildError::MissingModulePart {
                container: container_id,
                module_part: *module_part_id,
            });
        };

        if module_part.is_recovered() {
            return Ok(true);
        }
    }

    Ok(false)
}

fn module_visibility(
    table: &DeclarationTable,
    container_id: ContainerId,
) -> Result<MemberVisibility, SymbolGraphBuildError> {
    let Some(container) = table.container(container_id) else {
        return Err(SymbolGraphBuildError::MissingModulePath {
            container: container_id,
        });
    };

    let Some(module_part_id) = container.module_parts().first().copied() else {
        return Err(SymbolGraphBuildError::MissingRecoveredModuleAnchor {
            container: container_id,
        });
    };

    let Some(module_part) = table.module_part(module_part_id) else {
        return Err(SymbolGraphBuildError::MissingModulePart {
            container: container_id,
            module_part: module_part_id,
        });
    };

    Ok(surface_visibility(module_part.surface()))
}

fn surface_visibility(surface: &DeclarationSurface) -> MemberVisibility {
    if surface.is_internal() {
        MemberVisibility::Internal
    } else {
        MemberVisibility::Public
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_declarations::{DeclarationKind, DeclarationTable, merge_declaration_chunks};
    use bray_testing::test_source_store;

    use crate::surface_kind::{DeclarationSurfaceKind, declaration_symbol_kind};
    use crate::test_support::{declaration_chunk, declaration_table};
    use crate::{
        AnySymbolId, CallableSymbolId, CompilerKnownEnvironmentSymbolId, FunctionSymbolId,
        MemberLookupResult, ModuleOwnerId, ModulePathKey, PackageIdentity, RuntimeDefaultPresence,
        SymbolGraph, SymbolId, SymbolKind, SymbolOrigin, SymbolProvider, SymbolRecordId,
    };

    #[test]
    fn roots_and_partial_modules_form_one_deterministic_identity_tree() {
        let table = declaration_table(&[
            "module core.io; func read() {}",
            "module core.io { const Size: Int = 1; }",
        ]);

        let graph = build_graph(&table);

        assert_eq!(graph.roots().compiler_known().symbol_id().raw(), 0);
        assert_eq!(graph.roots().packages().len(), 1);
        assert_eq!(graph.roots().packages()[0].symbol_id().raw(), 15);
        assert_eq!(graph.modules().len(), 3);

        let package = &graph.packages()[0];
        let module = source_module(&graph);

        assert_eq!(module.id().symbol_id().raw(), 16);
        assert_eq!(package.modules(), [module.id()]);
        assert_eq!(module.owner(), ModuleOwnerId::from(package.id()));
        assert_eq!(module.origin(), SymbolOrigin::Source);
        assert_eq!(module.declarations().len(), 2);
        assert_eq!(module.module_parts().len(), 2);

        let path = valid_module_path(["core", "io"]);

        assert_eq!(
            graph.module_by_path(ModuleOwnerId::from(package.id()), &path),
            Some(module)
        );

        assert_eq!(graph.functions().len(), 2);
        assert_eq!(graph.constants().len(), 1);
    }

    #[test]
    fn exact_access_is_checked_against_the_assigned_kind_collection() {
        let table = declaration_table(&["module app; func main() {}"]);
        let graph = build_graph(&table);
        let function = source_function(&graph);

        assert_eq!(graph.function(function.id()), Some(function));

        let forged = FunctionSymbolId::from_symbol_id(SymbolId::new(100));

        assert_eq!(graph.function(forged), None);
    }

    #[test]
    fn graph_member_lookup_preserves_names_visibility_categories_and_recovery() {
        let table = declaration_table(&[concat!(
            "module app; ",
            "func run() {} ",
            "internal const hidden: bool = true; ",
            "struct Point { x: bool; } ",
            "func recovered("
        )]);

        let graph = build_graph(&table);
        let module = source_module(&graph);

        assert_eq!(
            graph.lookup_member(module.id().into(), "run"),
            MemberLookupResult::Found(source_function(&graph).id().into())
        );

        assert!(matches!(
            graph.lookup_member_with_access(module.id().into(), "hidden", |_, visibility| {
                visibility.is_public()
            }),
            MemberLookupResult::Inaccessible(_)
        ));

        let structure = graph.lookup_member(module.id().into(), "Point");

        let MemberLookupResult::Found(AnySymbolId::Struct(structure)) = structure else {
            panic!("structure lookup must retain its exact symbol kind");
        };

        assert!(matches!(
            graph.lookup_member(structure.into(), "x"),
            MemberLookupResult::Found(AnySymbolId::StructField(_))
        ));

        assert!(matches!(
            graph.lookup_member(module.id().into(), "recovered"),
            MemberLookupResult::Malformed(_)
        ));
    }

    #[test]
    fn compiler_known_members_use_the_same_origin_neutral_lookup_contract() {
        let graph = build_graph(&declaration_table(&["module app;"]));
        let environment = graph.compiler_known_environment();

        assert!(matches!(
            graph.lookup_member(environment.id().into(), "bool"),
            MemberLookupResult::Found(AnySymbolId::Struct(_))
        ));
    }

    #[test]
    fn source_symbols_are_available_through_the_origin_neutral_provider_contract() {
        let table = declaration_table(&["module app; func main() {}"]);
        let graph = build_graph(&table);
        let function = source_function(&graph);

        assert_eq!(provided_symbol(&graph, function.id()), Some(function));

        let forged_function = FunctionSymbolId::from_symbol_id(SymbolId::new(100));

        assert_eq!(provided_symbol(&graph, forged_function), None);

        let environment = graph.compiler_known_environment();

        assert_eq!(provided_symbol(&graph, environment.id()), Some(environment));

        let forged_environment =
            CompilerKnownEnvironmentSymbolId::from_symbol_id(SymbolId::new(100));

        assert_eq!(provided_symbol(&graph, forged_environment), None);
    }

    #[test]
    fn nested_declarations_retain_immediate_semantic_containment() {
        let table = declaration_table(&[concat!(
            "module app; ",
            "struct Point<T> { x: T; } ",
            "union Maybe { Some(value: Int); } ",
            "func make(value: Int) {}",
        )]);

        let graph = build_graph(&table);

        let module = source_module(&graph);

        let Some(structure) = graph.structure(module.structures()[0]) else {
            panic!("source structure relationship must resolve");
        };

        let Some(field) = graph.struct_field(structure.fields()[0]) else {
            panic!("source field relationship must resolve");
        };

        let generic = &graph.generic_type_parameters()[0];

        assert_eq!(field.containing_symbol(), structure.id().into());
        assert_eq!(generic.containing_symbol(), structure.id().into());

        let Some(union) = graph.union(module.unions()[0]) else {
            panic!("source union relationship must resolve");
        };

        let variant = &graph.union_variants()[0];
        let payload = &graph.union_payload_fields()[0];

        assert_eq!(variant.containing_symbol(), union.id().into());
        assert_eq!(payload.containing_symbol(), variant.id().into());

        let function = source_function(&graph);
        let parameter = &graph.callable_parameters()[0];

        assert_eq!(parameter.containing_symbol(), function.id().into());
    }

    #[test]
    fn kind_specific_records_publish_typed_declaration_relationships() {
        let table = declaration_table(&[concat!(
            "module app; ",
            "func make<T, const N: Int>(first: T = 1, second: T) {} ",
            "struct Config<U> { value: U = 1; func read() {} static func create() {} } ",
            "union Maybe { Some(value: Int = 1); None; } ",
            "overload create = {make}",
        )]);

        let graph = build_graph(&table);
        let module = source_module(&graph);
        let function = source_function(&graph);

        assert_eq!(module.functions(), [function.id()]);
        assert_eq!(module.structures().len(), 1);
        assert_eq!(module.unions().len(), 1);
        assert_eq!(
            module.callable_overloads(),
            [graph.callable_overloads()[0].id()]
        );

        assert_eq!(function.generic_type_parameters().len(), 1);
        assert_eq!(function.generic_const_parameters().len(), 1);
        assert_eq!(function.parameters().len(), 2);

        let first_parameter = graph.callable_parameter(function.parameters()[0]);

        let Some(first_parameter) = first_parameter else {
            panic!("function parameter relationship must resolve to its typed record");
        };

        assert_eq!(
            first_parameter.owner(),
            CallableSymbolId::from(function.id())
        );
        assert_eq!(first_parameter.ordinal(), 0);
        assert_eq!(
            first_parameter.default_presence(),
            RuntimeDefaultPresence::Present
        );

        let Some(parameter_provider_id) = first_parameter.default_provider() else {
            panic!("written callable default must have a provider identity");
        };

        let parameter_provider = graph.callable_parameter_default_provider(parameter_provider_id);

        let Some(parameter_provider) = parameter_provider else {
            panic!("provider identity must resolve to its kind-specific record");
        };

        assert_eq!(parameter_provider.subject(), first_parameter.id());
        assert_eq!(parameter_provider.containing_symbol(), function.id().into());
        assert_eq!(parameter_provider.origin(), SymbolOrigin::Synthesized);

        let Some(structure) = graph.structure(module.structures()[0]) else {
            panic!("source structure relationship must resolve");
        };

        let field = graph.struct_field(structure.fields()[0]);

        let Some(field) = field else {
            panic!("struct field relationship must resolve to its typed record");
        };

        assert_eq!(field.ordinal(), 0);
        assert_eq!(field.default_presence(), RuntimeDefaultPresence::Present);
        assert!(field.default_provider().is_some());
        assert_eq!(field.structure(), structure.id());

        let [instance_member, static_member] = structure.callable_members() else {
            panic!("expected one instance and one static callable member");
        };

        let instance_member = graph.type_callable_member(*instance_member);
        let static_member = graph.type_callable_member(*static_member);

        let (Some(instance_member), Some(static_member)) = (instance_member, static_member) else {
            panic!("callable member relationships must resolve to typed records");
        };

        let Some(receiver_id) = instance_member.receiver() else {
            panic!("instance callable member must synthesize a receiver");
        };

        let receiver = graph.receiver_parameter(receiver_id);

        let Some(receiver) = receiver else {
            panic!("receiver identity must resolve to its typed record");
        };

        assert_eq!(
            receiver.owner(),
            CallableSymbolId::from(instance_member.id())
        );
        assert_eq!(receiver.origin(), SymbolOrigin::Synthesized);
        assert_eq!(static_member.receiver(), None);

        let Some(union) = graph.union(module.unions()[0]) else {
            panic!("source union relationship must resolve");
        };

        let variant = graph.union_variant(union.variants()[0]);

        let Some(variant) = variant else {
            panic!("union variant relationship must resolve to its typed record");
        };

        assert_eq!(variant.union(), union.id());

        let payload = graph.union_payload_field(variant.payload_fields()[0]);

        let Some(payload) = payload else {
            panic!("payload relationship must resolve to its typed record");
        };

        assert_eq!(payload.ordinal(), 0);
        assert_eq!(payload.variant(), variant.id());
        assert_eq!(payload.default_presence(), RuntimeDefaultPresence::Present);
        assert!(payload.default_provider().is_some());
        assert_eq!(graph.callable_overloads()[0].arm_syntax().len(), 1);
    }

    #[test]
    fn recovered_runtime_defaults_keep_deterministic_provider_identities() {
        let table = declaration_table(&["module app; func make(value: Int = ) {}"]);
        let graph = build_graph(&table);
        let parameter = &graph.callable_parameters()[0];

        assert_eq!(
            parameter.default_presence(),
            RuntimeDefaultPresence::Recovered
        );
        assert!(parameter.default_provider().is_some());
        assert_eq!(graph.callable_parameter_default_providers().len(), 1);
    }

    #[test]
    fn conflicting_source_declarations_keep_distinct_identities() {
        let table = declaration_table(&["module app; func same() {} func same() {}"]);
        let graph = build_graph(&table);

        let source_functions = graph
            .functions()
            .iter()
            .filter(|function| function.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [first, second] = source_functions.as_slice() else {
            panic!("expected both conflicting functions to remain in the graph");
        };

        assert_ne!(first.id(), second.id());
        assert_ne!(first.key(), second.key());

        let Some(first_declaration) = first.declaration() else {
            panic!("source function must retain its declaration");
        };

        let Some(second_declaration) = second.declaration() else {
            panic!("source function must retain its declaration");
        };

        assert_eq!(
            graph.symbol_for_declaration(first_declaration),
            Some(AnySymbolId::from(first.id()))
        );
        assert_eq!(
            graph.symbol_for_declaration(second_declaration),
            Some(AnySymbolId::from(second.id()))
        );
    }

    #[test]
    fn recovered_declarations_keep_source_identity_and_containment() {
        let table = declaration_table(&["module app; struct Point\nusing core;"]);
        let graph = build_graph(&table);

        let source_structures = graph
            .structures()
            .iter()
            .filter(|structure| structure.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [structure] = source_structures.as_slice() else {
            panic!("expected the recovered struct declaration to produce a symbol");
        };

        let module = source_module(&graph);

        assert!(structure.is_recovered());
        assert_eq!(structure.containing_symbol(), module.id().into());
        assert_eq!(
            graph.containing_symbol(structure.id().into()),
            Some(module.id().into())
        );
        assert_eq!(graph.containing_module(structure.id().into()), Some(module));

        let Some(structure_declaration) = structure.declaration() else {
            panic!("source structure must retain its declaration");
        };

        let declaration = match table.declaration(structure_declaration) {
            Some(declaration) => declaration,
            None => panic!("symbol declaration should remain in the declaration table"),
        };

        let Some(syntax) = structure.syntax_anchor() else {
            panic!("source structure must retain its syntax anchor");
        };

        assert_eq!(syntax.full_range(), declaration.full_range());
        assert!(
            graph
                .symbol_for_declaration(structure_declaration)
                .is_some()
        );
    }

    #[test]
    fn recovered_empty_module_paths_use_structured_declaration_anchors() {
        let table = declaration_table(&["module ; func main() {}"]);
        let graph = build_graph(&table);

        let module = source_module(&graph);

        assert!(module.is_recovered());
        assert!(module.path().is_recovered());
        assert_eq!(module.path().segments().len(), 0);
        assert_eq!(
            module.path().recovery_anchor(),
            module.declarations().first().copied()
        );
        assert_eq!(
            source_function(&graph).containing_symbol(),
            module.id().into()
        );
    }

    #[test]
    fn graph_identity_is_independent_of_chunk_and_worker_order() {
        let sources = test_source_store([
            "module core; struct Point { x: Int; }",
            "module core; union Maybe { Some(value: Int); }",
        ]);

        let first = declaration_chunk(&sources, 0);
        let second = declaration_chunk(&sources, 1);

        let forward = merge_declaration_chunks([&first, &second]);
        let reverse = merge_declaration_chunks([&second, &first]);

        let forward_graph = build_graph(forward.table());
        let reverse_graph = build_graph(reverse.table());

        assert_eq!(forward_graph, reverse_graph);

        let table = Arc::new(forward.table().clone());
        let expected = Arc::new(forward_graph);

        let workers = (0..4)
            .map(|_| {
                let table = Arc::clone(&table);
                let expected = Arc::clone(&expected);

                std::thread::spawn(move || {
                    let actual = build_graph(&table);

                    assert_eq!(actual, *expected);
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            if let Err(error) = worker.join() {
                panic!("parallel graph construction failed: {error:?}");
            }
        }
    }

    #[test]
    fn implementation_context_selects_fulfillment_categories() {
        assert_eq!(
            declaration_symbol_kind(
                surface_kind(DeclarationKind::TypeCallableMember),
                SymbolKind::NamedTraitImplementation,
            ),
            SymbolKind::TraitCallableFulfillment
        );

        assert_eq!(
            declaration_symbol_kind(
                surface_kind(DeclarationKind::ImplementationTypeMemberBinding),
                SymbolKind::UnnamedTraitImplementation,
            ),
            SymbolKind::TraitTypeFulfillment
        );

        assert_eq!(
            declaration_symbol_kind(
                surface_kind(DeclarationKind::TypeCallableMember),
                SymbolKind::InherentImplementation,
            ),
            SymbolKind::TypeCallableMember
        );
    }

    fn package_identity() -> PackageIdentity {
        match PackageIdentity::try_new("test.package") {
            Some(identity) => identity,
            None => panic!("test package identity is valid"),
        }
    }

    fn surface_kind(kind: DeclarationKind) -> DeclarationSurfaceKind {
        match kind.try_into() {
            Ok(kind) => kind,
            Err(()) => panic!("test declaration kind must produce a symbol"),
        }
    }

    fn valid_module_path<const N: usize>(segments: [&str; N]) -> ModulePathKey {
        match ModulePathKey::try_new(segments) {
            Some(path) => path,
            None => panic!("test module path is valid"),
        }
    }

    fn build_graph(table: &DeclarationTable) -> SymbolGraph {
        match SymbolGraph::build_source(package_identity(), table) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph should build: {error:?}"),
        }
    }

    fn source_module(graph: &SymbolGraph) -> &crate::ModuleSymbol {
        let Some(package) = graph.packages().first() else {
            panic!("source graph must retain its package root");
        };

        let Some(module_id) = package.modules().first().copied() else {
            panic!("test package must retain its source module");
        };

        let Some(module) = graph.module(module_id) else {
            panic!("source module ID must resolve in the graph");
        };

        module
    }

    fn source_function(graph: &SymbolGraph) -> &crate::FunctionSymbol {
        let module = source_module(graph);

        let Some(function_id) = module.functions().first().copied() else {
            panic!("test source module must retain its function");
        };

        let Some(function) = graph.function(function_id) else {
            panic!("source function ID must resolve in the graph");
        };

        function
    }

    fn provided_symbol<I>(provider: &impl SymbolProvider<I>, id: I) -> Option<&I::Record>
    where
        I: SymbolRecordId,
    {
        provider.symbol(id)
    }
}
