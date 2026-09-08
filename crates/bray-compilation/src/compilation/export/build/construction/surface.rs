use std::collections::{BTreeMap, BTreeSet};

use bray_binder::{NameAccess, SymbolQueryProvider};
use bray_package_interface::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    ExportedLookupKind, SymbolRelationshipKind,
};
use bray_symbols::{
    AnySymbolId, ExternalSymbolKey, MemberLookupResult, ModulePathKey, ModuleSurfaceQuery,
    ModuleSymbolId, SymbolKeyData, SymbolKind, SymbolOrdinal, SymbolOrigin, SymbolQueryRequest,
    SynthesizedSymbolKey, SynthesizedSymbolRole,
};
use bray_syntax::PathSyntax;

use super::super::super::PackageInterfaceExportError;
use crate::compilation::Compilation;
pub(in crate::compilation::export::build) struct ExportIdentitySurface {
    pub(in crate::compilation::export::build) symbols: Vec<ExportSymbolInput>,
    pub(in crate::compilation::export::build) relationships: Vec<ExportRelationshipInput>,
    pub(in crate::compilation::export::build) exports: Vec<ExportLookupInput>,
    pub(in crate::compilation::export::build) selected: BTreeSet<AnySymbolId>,
    pub(in crate::compilation::export::build) keys: BTreeMap<AnySymbolId, ExternalSymbolKey>,
}

pub(in crate::compilation) fn external_symbol_key(
    graph: &bray_symbols::SymbolGraph,
    package_identity: &bray_symbols::PackageIdentity,
    symbol: AnySymbolId,
) -> Result<ExternalSymbolKey, PackageInterfaceExportError> {
    let package = package_symbol(graph, package_identity)?;

    // External keys retain the Arc-backed package identity beyond this request.
    let mut keys = BTreeMap::from([(
        package,
        ExternalSymbolKey::package(package_identity.clone()),
    )]);

    external_key(graph, symbol, &mut keys)
}

pub(in crate::compilation::export::build) fn build_identity_surface(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    public_symbols: &[AnySymbolId],
) -> Result<ExportIdentitySurface, PackageInterfaceExportError> {
    // The exported surface retains the Arc-backed package identity independently.
    let package_key = ExternalSymbolKey::package(compilation.package_identity().clone());
    let package_symbol = package_symbol(graph, compilation.package_identity())?;
    let mut selected = BTreeSet::from_iter(public_symbols.iter().copied());

    selected.extend(
        graph
            .symbols()
            .filter(|symbol| graph.symbol_origin(*symbol) == Some(SymbolOrigin::Source)),
    );

    for symbol in public_symbols.iter().copied() {
        select_required_children(graph, symbol, &mut selected);
        select_owner_chain(graph, symbol, package_symbol, &mut selected)?;
    }

    selected.insert(package_symbol);

    for module in graph
        .modules()
        .iter()
        .filter(|module| module.origin() == SymbolOrigin::Source && module.visibility().is_public())
    {
        if module.is_recovered() {
            return Err(PackageInterfaceExportError::RecoveredPublicSymbol(
                SymbolKind::Module,
            ));
        }

        selected.insert(module.id().into());
    }

    loop {
        let previous_count = selected.len();

        for owner in selected.iter().copied().collect::<Vec<_>>() {
            select_required_children(graph, owner, &mut selected);
        }

        if selected.len() == previous_count {
            break;
        }
    }

    let mut keys = BTreeMap::new();

    keys.insert(package_symbol, package_key);

    for symbol in selected.iter().copied() {
        external_key(graph, symbol, &mut keys)?;
    }

    let symbols = selected
        .iter()
        .copied()
        .map(|symbol| export_symbol(graph, symbol, &keys))
        .collect::<Result<Vec<_>, _>>()?;

    let mut relationships = selected
        .iter()
        .copied()
        .filter_map(|symbol| export_relationship(graph, symbol, &selected, &keys).transpose())
        .collect::<Result<Vec<_>, _>>()?;

    relationships.extend(source_overload_relationships(
        compilation,
        graph,
        &selected,
        &keys,
    )?);

    let mut exports = direct_exports(graph, &selected, &keys)?;
    exports.extend(compilation.public_module_re_exports(&module_keys(graph, &keys)?)?);

    Ok(ExportIdentitySurface {
        symbols,
        relationships,
        exports,
        selected,
        keys,
    })
}

fn source_overload_relationships(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    selected: &BTreeSet<AnySymbolId>,
    keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
) -> Result<Vec<ExportRelationshipInput>, PackageInterfaceExportError> {
    let binding_context = compilation
        .binding_context(&compilation.state.cancellation)
        .map_err(super::super::super::invalid_compilation_fact_error)?;

    let overloads = graph
        .callable_overloads()
        .iter()
        .filter(|overload| overload.origin() == SymbolOrigin::Source)
        .map(|overload| (AnySymbolId::from(overload.id()), overload.arm_syntax()))
        .chain(
            graph
                .implementation_overloads()
                .iter()
                .filter(|overload| overload.origin() == SymbolOrigin::Source)
                .map(|overload| (AnySymbolId::from(overload.id()), overload.arm_syntax())),
        );

    let mut relationships = Vec::new();

    for (overload, arms) in overloads {
        if !selected.contains(&overload) {
            continue;
        }

        let owner = graph.containing_symbol(overload).ok_or(
            PackageInterfaceExportError::IncompletePublicDeclarationSemantics(overload.kind()),
        )?;

        let owner_key = keys.get(&overload).cloned().ok_or(
            PackageInterfaceExportError::IncompletePublicDeclarationSemantics(overload.kind()),
        )?;

        for (ordinal, anchor) in arms.iter().enumerate() {
            let path = anchor
                .find_descendant::<PathSyntax>(compilation.syntax_tree())
                .ok_or(
                    PackageInterfaceExportError::IncompletePublicDeclarationSemantics(
                        overload.kind(),
                    ),
                )?;

            let result = match owner {
                AnySymbolId::Module(module) => {
                    binding_context.bind_surface_path(module, &path, NameAccess::Internal)
                }
                owner => {
                    binding_context.bind_owner_surface_path(owner, &path, NameAccess::Internal)
                }
            }
            .map_err(super::super::super::invalid_compilation_binding_error)?;

            if result.diagnostics().has_errors() {
                // rust-style: allow(context-erasing-failure-conversion, reason = "source and semantic diagnostics retain the exact causes")
                return Err(PackageInterfaceExportError::InvalidCompilation);
            }

            let MemberLookupResult::Found(member) = result.value() else {
                return Err(super::super::super::export_contract_error(
                    super::super::super::PackageInterfaceExportContract::MissingOverloadArm,
                ));
            };

            if !selected.contains(member) {
                return Err(
                    PackageInterfaceExportError::IncompletePublicDeclarationSemantics(
                        member.kind(),
                    ),
                );
            }

            let member_key = keys.get(member).cloned().ok_or(
                PackageInterfaceExportError::IncompletePublicDeclarationSemantics(member.kind()),
            )?;

            let ordinal = u32::try_from(ordinal)
                .map(SymbolOrdinal::new)
                .map_err(|_| {
                    PackageInterfaceExportError::InvalidCompilationCause(
                        super::super::super::PackageInterfaceInvalidCompilationCause::Capacity {
                            field: "overload_arm_ordinal",
                            actual: ordinal.to_string(),
                        },
                    )
                })?;

            // Each serialized overload-arm relationship owns its stable owner identity.
            relationships.push(ExportRelationshipInput::new(
                SymbolRelationshipKind::OverloadArm,
                owner_key.clone(),
                member_key,
                ordinal.raw(),
            ));
        }
    }

    Ok(relationships)
}

fn package_symbol(
    graph: &bray_symbols::SymbolGraph,
    package_identity: &bray_symbols::PackageIdentity,
) -> Result<AnySymbolId, PackageInterfaceExportError> {
    graph
        .packages()
        .iter()
        .find(|package| package.identity() == package_identity)
        .map(|package| AnySymbolId::from(package.id()))
        .ok_or_else(|| {
            super::super::super::export_contract_error(
                super::super::super::PackageInterfaceExportContract::MissingPackageSymbol,
            )
        })
}

fn select_required_children(
    graph: &bray_symbols::SymbolGraph,
    owner: AnySymbolId,
    selected: &mut BTreeSet<AnySymbolId>,
) {
    for child in graph.declaration_children(owner).iter().copied() {
        if matches!(
            child.kind(),
            SymbolKind::GenericTypeParameter
                | SymbolKind::GenericConstParameter
                | SymbolKind::ReceiverParameter
                | SymbolKind::CallableParameterDefaultProvider
                | SymbolKind::StructFieldDefaultProvider
                | SymbolKind::UnionPayloadDefaultProvider
        ) {
            selected.insert(child);
        }
    }

    selected.extend(
        graph
            .callable_parameter_default_providers()
            .iter()
            .filter(|provider| AnySymbolId::CallableParameter(provider.subject()) == owner)
            .map(|provider| AnySymbolId::CallableParameterDefaultProvider(provider.id())),
    );

    selected.extend(
        graph
            .struct_field_default_providers()
            .iter()
            .filter(|provider| AnySymbolId::StructField(provider.subject()) == owner)
            .map(|provider| AnySymbolId::StructFieldDefaultProvider(provider.id())),
    );

    selected.extend(
        graph
            .union_payload_default_providers()
            .iter()
            .filter(|provider| AnySymbolId::UnionPayloadField(provider.subject()) == owner)
            .map(|provider| AnySymbolId::UnionPayloadDefaultProvider(provider.id())),
    );
}

fn select_owner_chain(
    graph: &bray_symbols::SymbolGraph,
    mut symbol: AnySymbolId,
    package: AnySymbolId,
    selected: &mut BTreeSet<AnySymbolId>,
) -> Result<(), PackageInterfaceExportError> {
    while symbol != package {
        let owner = graph.containing_symbol(symbol).ok_or(
            PackageInterfaceExportError::IncompletePublicDeclarationSemantics(symbol.kind()),
        )?;

        selected.insert(owner);
        symbol = owner;
    }

    Ok(())
}

fn external_key(
    graph: &bray_symbols::SymbolGraph,
    symbol: AnySymbolId,
    keys: &mut BTreeMap<AnySymbolId, ExternalSymbolKey>,
) -> Result<ExternalSymbolKey, PackageInterfaceExportError> {
    if let Some(key) = keys.get(&symbol) {
        return Ok(key.clone());
    }

    if graph.symbol_is_recovered(symbol) != Some(false) {
        return Err(PackageInterfaceExportError::RecoveredPublicSymbol(
            symbol.kind(),
        ));
    }

    let owner = graph
        .runtime_default_subject(symbol)
        .or_else(|| graph.containing_symbol(symbol))
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationSemantics(symbol.kind()))?;

    let owner_key = external_key(graph, owner, keys)?;

    let key = match graph.symbol_key(symbol).map(|key| key.data()) {
        Some(SymbolKeyData::Module { path, .. }) => {
            ExternalSymbolKey::module(owner_key.clone(), external_module_path(&owner_key, path))
        }
        Some(SymbolKeyData::Synthesized(synthesized)) => ExternalSymbolKey::synthesized(
            owner_key,
            synthesized.role(),
            external_synthesized_ordinal(synthesized),
        ),
        Some(SymbolKeyData::SourceDeclaration { .. }) => {
            external_declaration_key(graph, owner, owner_key, symbol)
        }
        _ => None,
    }
    .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationSemantics(symbol.kind()))?;

    keys.insert(symbol, key.clone());

    Ok(key)
}

fn external_synthesized_ordinal(key: &SynthesizedSymbolKey) -> Option<SymbolOrdinal> {
    match key.role() {
        SynthesizedSymbolRole::ReceiverParameter => None,
        SynthesizedSymbolRole::CallableParameterDefaultProvider
        | SynthesizedSymbolRole::StructFieldDefaultProvider
        | SynthesizedSymbolRole::UnionPayloadDefaultProvider => None,
        _ => key.ordinal(),
    }
}

fn external_module_path(package: &ExternalSymbolKey, path: &ModulePathKey) -> ModulePathKey {
    let package_segments = package
        .package_identity()
        .as_str()
        .split('.')
        .collect::<Vec<_>>();

    let path_segments = path.segments().collect::<Vec<_>>();

    if path_segments.len() > package_segments.len() && path_segments.starts_with(&package_segments)
    {
        return ModulePathKey::try_new(path_segments[package_segments.len()..].iter().copied())
            .unwrap_or_else(|| path.clone());
    }

    path.clone()
}

fn external_declaration_key(
    graph: &bray_symbols::SymbolGraph,
    owner: AnySymbolId,
    owner_key: ExternalSymbolKey,
    symbol: AnySymbolId,
) -> Option<ExternalSymbolKey> {
    let use_ordinal = matches!(
        symbol.kind(),
        SymbolKind::InherentImplementation | SymbolKind::UnnamedTraitImplementation
    ) || matches!(
        symbol.kind(),
        SymbolKind::GenericTypeParameter
            | SymbolKind::GenericConstParameter
            | SymbolKind::CallableParameter
            | SymbolKind::PredicateParameter
            | SymbolKind::Finalizer
            | SymbolKind::Destructor
            | SymbolKind::ScopeEnter
            | SymbolKind::ScopeExit
            | SymbolKind::ImplementationOverload
    ) || matches!(
        owner.kind(),
        SymbolKind::CallableOverload | SymbolKind::ImplementationOverload
    );

    if !use_ordinal {
        if let Some(name) = graph.member_name(symbol) {
            return ExternalSymbolKey::named(owner_key, symbol.kind(), name.clone());
        }
    }

    relationship_ordinal(graph, owner, symbol)
        .and_then(|ordinal| ExternalSymbolKey::ordinal(owner_key, symbol.kind(), ordinal))
}

fn relationship_ordinal(
    graph: &bray_symbols::SymbolGraph,
    owner: AnySymbolId,
    member: AnySymbolId,
) -> Option<SymbolOrdinal> {
    let kind = SymbolRelationshipKind::between(owner.kind(), member.kind())?;

    graph
        .declaration_children(owner)
        .iter()
        .copied()
        .filter(|child| SymbolRelationshipKind::between(owner.kind(), child.kind()) == Some(kind))
        .position(|child| child == member)
        .and_then(|ordinal| u32::try_from(ordinal).ok())
        .map(SymbolOrdinal::new)
}

fn export_symbol(
    graph: &bray_symbols::SymbolGraph,
    symbol: AnySymbolId,
    keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
) -> Result<ExportSymbolInput, PackageInterfaceExportError> {
    let key = keys
        .get(&symbol)
        .cloned()
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationSemantics(symbol.kind()))?;

    let containing_symbol = match graph
        .runtime_default_subject(symbol)
        .or_else(|| graph.containing_symbol(symbol))
    {
        Some(owner) => Some(keys.get(&owner).cloned().ok_or(
            PackageInterfaceExportError::IncompletePublicDeclarationSemantics(owner.kind()),
        )?),
        None => None,
    };

    Ok(ExportSymbolInput::new(key, containing_symbol))
}

fn export_relationship(
    graph: &bray_symbols::SymbolGraph,
    member: AnySymbolId,
    selected: &BTreeSet<AnySymbolId>,
    keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
) -> Result<Option<ExportRelationshipInput>, PackageInterfaceExportError> {
    let Some(owner) = graph
        .runtime_default_subject(member)
        .or_else(|| graph.containing_symbol(member))
    else {
        return Ok(None);
    };

    if !selected.contains(&owner) {
        return Ok(None);
    }

    let kind = SymbolRelationshipKind::between(owner.kind(), member.kind())
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationSemantics(member.kind()))?;

    let ordinal = exported_relationship_ordinal(graph, owner, member, selected)
        .map(SymbolOrdinal::raw)
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationSemantics(member.kind()))?;

    let owner_key = keys
        .get(&owner)
        .cloned()
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationSemantics(owner.kind()))?;

    let member_key = keys
        .get(&member)
        .cloned()
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationSemantics(member.kind()))?;

    let relationship = ExportRelationshipInput::new(kind, owner_key, member_key, ordinal);

    Ok(Some(with_field_properties(graph, member, relationship)))
}

fn exported_relationship_ordinal(
    graph: &bray_symbols::SymbolGraph,
    owner: AnySymbolId,
    member: AnySymbolId,
    selected: &BTreeSet<AnySymbolId>,
) -> Option<SymbolOrdinal> {
    let kind = SymbolRelationshipKind::between(owner.kind(), member.kind())?;

    graph
        .declaration_children(owner)
        .iter()
        .copied()
        .filter(|child| selected.contains(child))
        .filter(|child| SymbolRelationshipKind::between(owner.kind(), child.kind()) == Some(kind))
        .position(|child| child == member)
        .and_then(|ordinal| u32::try_from(ordinal).ok())
        .map(SymbolOrdinal::new)
}

fn with_field_properties(
    graph: &bray_symbols::SymbolGraph,
    symbol: AnySymbolId,
    relationship: ExportRelationshipInput,
) -> ExportRelationshipInput {
    match symbol {
        AnySymbolId::StructField(field) => {
            let Some(field) = graph.struct_field(field) else {
                return relationship;
            };

            if field.allows_mutation() {
                relationship.with_mutation()
            } else {
                relationship
            }
        }
        AnySymbolId::UnionPayloadField(field) => {
            let Some(field) = graph.union_payload_field(field) else {
                return relationship;
            };

            let relationship = relationship.with_position(field.position());

            if field.allows_mutation() {
                relationship.with_mutation()
            } else {
                relationship
            }
        }
        _ => relationship,
    }
}

fn direct_exports(
    graph: &bray_symbols::SymbolGraph,
    selected: &BTreeSet<AnySymbolId>,
    keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
) -> Result<Vec<ExportLookupInput>, PackageInterfaceExportError> {
    selected
        .iter()
        .copied()
        .filter_map(|symbol| {
            let owner = graph.containing_symbol(symbol)?;

            if owner.kind() != SymbolKind::Module {
                return None;
            }

            let member = graph.member_entry(symbol)?;

            if !member.visibility().is_public() {
                return None;
            }

            Some((owner, member.name().clone(), symbol))
        })
        .map(|(owner, name, symbol)| {
            let owner = keys.get(&owner).cloned().ok_or(
                PackageInterfaceExportError::IncompletePublicDeclarationSemantics(owner.kind()),
            )?;

            let target = keys.get(&symbol).cloned().ok_or(
                PackageInterfaceExportError::IncompletePublicDeclarationSemantics(symbol.kind()),
            )?;

            Ok(ExportLookupInput::new(
                owner,
                name,
                ExportedLookupKind::Direct,
                ExportSymbolReferenceInput::Local(target),
            ))
        })
        .collect()
}

fn module_keys(
    graph: &bray_symbols::SymbolGraph,
    keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
) -> Result<BTreeMap<ModuleSymbolId, ExternalSymbolKey>, PackageInterfaceExportError> {
    graph
        .modules()
        .iter()
        .filter(|module| module.origin() == SymbolOrigin::Source && module.visibility().is_public())
        .map(|module| {
            keys.get(&module.id().into())
                .cloned()
                .map(|key| (module.id(), key))
                .ok_or(
                    PackageInterfaceExportError::IncompletePublicDeclarationSemantics(
                        SymbolKind::Module,
                    ),
                )
        })
        .collect()
}

impl Compilation {
    fn public_module_re_exports(
        &self,
        module_keys: &BTreeMap<ModuleSymbolId, ExternalSymbolKey>,
    ) -> Result<Vec<ExportLookupInput>, PackageInterfaceExportError> {
        let semantics = self
            .binding_context(&self.state.cancellation)
            .map_err(super::super::super::invalid_compilation_fact_error)?;

        let mut exports = Vec::new();

        for (module, owner_key) in module_keys {
            let surface = semantics
                .resolve_symbol_query(SymbolQueryRequest::<ModuleSurfaceQuery>::new(*module))
                .map_err(super::super::super::invalid_compilation_binding_error)?;

            if surface.diagnostics().has_errors() {
                // rust-style: allow(context-erasing-failure-conversion, reason = "source and semantic diagnostics retain the exact causes")
                return Err(PackageInterfaceExportError::InvalidCompilation);
            }

            for edge in surface
                .value()
                .re_exports()
                .iter()
                .filter(|edge| edge.visibility().is_public())
            {
                let AnySymbolId::Module(target) = edge.target() else {
                    return Err(
                        PackageInterfaceExportError::IncompletePublicDeclarationSemantics(
                            edge.target().kind(),
                        ),
                    );
                };

                let Some(target_key) = module_keys.get(&target) else {
                    return Err(
                        PackageInterfaceExportError::IncompletePublicDeclarationSemantics(
                            SymbolKind::Module,
                        ),
                    );
                };

                exports.push(ExportLookupInput::new(
                    owner_key.clone(),
                    edge.name().clone(),
                    ExportedLookupKind::ReExport,
                    ExportSymbolReferenceInput::Local(target_key.clone()),
                ));
            }
        }

        Ok(exports)
    }
}
