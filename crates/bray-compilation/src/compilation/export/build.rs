use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_package_interface::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    ExportedLookupKind, InterfaceDependency, InterfaceProductKind, PackageInterfaceExportBundle,
    SymbolRelationshipKind, build_package_interface_surface,
};
use bray_symbols::{
    AnySymbolId, ExternalSymbolKey, ModulePathKey, ModuleSurfaceQuery, ModuleSymbolId, ProductKind,
    SymbolFactRequest, SymbolKeyData, SymbolKind, SymbolOrdinal, SymbolOrigin,
    SynthesizedSymbolKey, SynthesizedSymbolRole,
};

use super::PackageInterfaceExportError;
use crate::compilation::Compilation;
use crate::fact::CompilationFactKey;

impl Compilation {
    /// Returns the current library product's interface export, when configured.
    pub fn package_interface_export_bundle(
        &self,
    ) -> Option<&Result<Arc<PackageInterfaceExportBundle>, PackageInterfaceExportError>> {
        self.state.package_interface_export.as_ref()?;

        Some(self.fact(
            CompilationFactKey::PackageInterfaceExportBundle,
            &self.state.package_interface_export_bundle,
            || {
                let request = self
                    .package_interface_export_request()
                    .unwrap_or_else(|| panic!("export semantics requires its configured request"));

                let span = self.state.fact_runtime.profile().map(|profile| {
                    profile.start(crate::profile::ProfileOperation::InterfaceExport, None)
                });

                let result = self.build_package_interface_export_bundle(request);

                if let Some(span) = span {
                    span.finish(crate::profile::result_outcome(&result));
                }

                result
            },
        ))
    }

    fn build_package_interface_export_bundle(
        &self,
        request: &crate::PackageInterfaceExportRequest,
    ) -> Result<Arc<PackageInterfaceExportBundle>, PackageInterfaceExportError> {
        if self.product_kind() != ProductKind::Library
            || request.identity().kind() != InterfaceProductKind::Library
            || request.identity().package() != self.package_identity()
        {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        let source_graph = self
            .product_source_graph()
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        if self.source_diagnostics().has_errors()
            || self.syntax_tree_result().diagnostics().has_errors()
            || source_graph.diagnostics().has_errors()
            || self.imported_diagnostics().has_errors()
        {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        let product = self
            .product_semantics()
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        if product.diagnostics().has_errors() || product.value().is_recovered() {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        let symbols = self
            .symbol_graph()
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        let identity = build_identity_surface(self, symbols, product.value().public_symbols())?;

        let dependencies = self
            .loaded_interface_views(&self.state.cancellation)
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?
            .ok_or(PackageInterfaceExportError::InvalidCompilation)?
            .into_iter()
            .map(|dependency| {
                let identity = dependency.surface().identity();

                InterfaceDependency::new(
                    identity.package().clone(),
                    identity.product().clone(),
                    dependency.content_hash(),
                )
            })
            .collect::<Vec<_>>();

        // Package-interface identities are Arc-backed and retained by the immutable surface.
        let package_identity = request.identity().clone();

        let surface = build_package_interface_surface(
            package_identity,
            dependencies,
            identity.symbols,
            identity.relationships,
            identity.exports,
        )
        .map_err(PackageInterfaceExportError::Surface)?;

        let (semantics, executable_templates, native_boundaries) =
            super::semantic::build_semantics(
                self,
                symbols,
                &surface,
                &identity.selected,
                &identity.keys,
            )?;

        let implementation_configuration = self
            .package_implementation_configuration(None)
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        PackageInterfaceExportBundle::try_new(
            surface,
            semantics,
            request.language_revision(),
            implementation_configuration,
        )
        .and_then(|bundle| bundle.with_executable_templates(executable_templates))
        .and_then(|bundle| bundle.with_native_boundaries(native_boundaries))
        .map(Arc::new)
        .map_err(PackageInterfaceExportError::Bundle)
    }
}

struct ExportIdentitySurface {
    symbols: Vec<ExportSymbolInput>,
    relationships: Vec<ExportRelationshipInput>,
    exports: Vec<ExportLookupInput>,
    selected: BTreeSet<AnySymbolId>,
    keys: BTreeMap<AnySymbolId, ExternalSymbolKey>,
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

fn build_identity_surface(
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

    let relationships = selected
        .iter()
        .copied()
        .filter_map(|symbol| export_relationship(graph, symbol, &selected, &keys).transpose())
        .collect::<Result<Vec<_>, _>>()?;

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

fn package_symbol(
    graph: &bray_symbols::SymbolGraph,
    package_identity: &bray_symbols::PackageIdentity,
) -> Result<AnySymbolId, PackageInterfaceExportError> {
    graph
        .packages()
        .iter()
        .find(|package| package.identity() == package_identity)
        .map(|package| AnySymbolId::from(package.id()))
        .ok_or(PackageInterfaceExportError::InvalidCompilation)
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
        let owner = graph
            .containing_symbol(symbol)
            .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationSemantics(symbol.kind()))?;

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

    let containing_symbol =
        match graph
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

    let Some(kind) = SymbolRelationshipKind::between(owner.kind(), member.kind()) else {
        return Ok(None);
    };

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
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        let mut exports = Vec::new();

        for (module, owner_key) in module_keys {
            let surface = semantics
                .symbol_fact(SymbolFactRequest::<ModuleSurfaceQuery>::new(*module))
                .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

            if surface.diagnostics().has_errors() {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_ir::{MirOperationKind, MirProjectionKind};
    use bray_package_interface::{
        InterfaceConstantValueKind, InterfaceLanguageRevision, InterfaceProductIdentity,
        InterfaceSymbolReference, InterfaceValidationLimits, InterfaceValidationPolicy,
        PackageImplementationArtifact, PackageInterfaceExportBundle, ValidatedPackageInterface,
        encode_package_interface,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};
    use bray_symbols::{
        AnySymbolId, CallableParameterDefaultValue, ExternalSymbolKey, IntegerConstant,
        MemberLookupResult, ModulePathKey, PackageIdentity, ProductKind,
        RuntimeDefaultTemplateReference, SymbolKey, SymbolKind, SymbolName, TypeExpressionTemplate,
    };
    use bray_testing::test_source_inputs;

    use crate::test_support::{
        package_version, source_function_body_key,
        source_named_trait_callable_fulfillment_body_key,
    };
    use crate::{
        Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput,
        PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
    };

    #[test]
    fn module_only_library_exports_are_cached_on_demand() {
        let compilation = compilation("module app;");

        assert!(
            compilation
                .state
                .package_interface_export_bundle
                .get()
                .is_none()
        );

        let first = export(&compilation);
        let second = export(&compilation);

        assert!(std::ptr::eq(first, second));
        assert_eq!(first.surface().symbols().symbols().len(), 2);

        assert!(
            compilation
                .state
                .package_interface_export_bundle
                .get()
                .is_some()
        );
    }

    #[test]
    fn internal_owner_chains_retain_identity_without_entering_exported_lookup() {
        let compilation = compilation(concat!(
            "module app;\n",
            "internal struct Hidden\n",
            "{\n",
            "    func method()\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        let bundle = export(&compilation);

        assert_eq!(bundle.surface().symbols().symbols().len(), 5);
        assert!(bundle.surface().exports().is_empty());
    }

    #[test]
    fn public_module_re_exports_enter_the_interface_lookup_surface() {
        let compilation =
            compilation_from_sources(["module a;\n", concat!("module b;\n", "\n", "export a;\n",)]);

        let bundle = export(&compilation);

        let [edge] = bundle.surface().exports() else {
            panic!(
                "expected one module re-export: {:?}",
                bundle.surface().exports()
            );
        };

        assert_eq!(edge.name().as_str(), "a");

        assert_eq!(
            edge.kind(),
            bray_package_interface::ExportedLookupKind::ReExport
        );
    }

    #[test]
    fn public_callable_and_type_semantics_round_trip_without_source() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "public struct Boxed<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "\n",
            "public union Maybe<T>\n",
            "{\n",
            "    Some(value: T);\n",
            "    None;\n",
            "}\n",
            "\n",
            "public func identity<T>(pos value: T) -> T with(true)\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "public func count(pos value: i32 = 1) -> usize requires(value > 0)\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let bundle = export(&compilation);

        let artifact = encode_package_interface(bundle)
            .unwrap_or_else(|error| panic!("public interface must encode: {error:?}"));

        let validated = ValidatedPackageInterface::try_new(
            artifact.shared_bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
        .unwrap_or_else(|error| panic!("public interface must validate: {error:?}"));

        let surface = validated
            .decode_identity_surface()
            .unwrap_or_else(|error| panic!("identity surface must decode: {error:?}"));

        let semantics = validated
            .decode_semantics(&surface)
            .unwrap_or_else(|error| panic!("semantics must decode: {error:?}"));

        assert_eq!(semantics.callable_signatures().len(), 2);
        assert_eq!(semantics.generic_declarations().len(), 4);
        assert_eq!(semantics.constraints().len(), 1);
        assert_eq!(semantics.checked_templates().len(), 3);
        assert_eq!(semantics.declaration_templates().len(), 3);
        assert_eq!(semantics.declared_types().len(), 2);
        assert_eq!(semantics.type_representations().len(), 2);
    }

    #[test]
    fn callable_signatures_export_fixed_array_lengths() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "internal func first(pos values: &[u8; 32]) -> u8\n",
            "{\n",
            "    return values[0];\n",
            "}\n",
        ));

        let bundle = export(&compilation);

        assert_eq!(bundle.semantics().callable_signatures().len(), 1);
    }

    #[test]
    fn generic_container_lifecycle_bodies_publish_executable_templates() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "public struct Boxed<T>\n",
            "{\n",
            "    value: T;\n",
            "\n",
            "    construct(value: T) -> Self\n",
            "    {\n",
            "        return { value = value, };\n",
            "    }\n",
            "\n",
            "    destruct()\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let bundle = export(&compilation);

        assert_eq!(bundle.executable_templates().len(), 2);
    }

    #[test]
    fn exported_callable_and_type_semantics_intern_without_provider_source() {
        let provider = compilation(concat!(
            "module app;\n",
            "\n",
            "public struct Boxed<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "\n",
            "public trait Provides\n",
            "{\n",
            "    type Item;\n",
            "}\n",
            "\n",
            "public impl Boxed<i32>\n",
            "{\n",
            "    type Local = i32;\n",
            "}\n",
            "\n",
            "public impl Boxed<i32>(Provides)\n",
            "{\n",
            "    type Item = i32;\n",
            "}\n",
            "\n",
            "public func count(pos value: i32 = 1) -> usize requires(value > 0)\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let artifact = encode_package_interface(export(&provider))
            .unwrap_or_else(|error| panic!("provider interface must encode: {error:?}"));

        let provider_package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("provider package identity must be valid"));

        let provider_product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("provider product identity must be valid"));

        let dependency = DependencyInterfaceInput::new(
            provider_package,
            provider_product,
            "provider.brayi",
            artifact.shared_bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let consumer_package = PackageIdentity::try_new("consumer.package")
            .unwrap_or_else(|| panic!("consumer package identity must be valid"));

        let source = SourceInput::virtual_text(
            SourceIdentity::new(0),
            "consumer.bray",
            SourceVersion::new(0),
            "module app;\n",
        );

        let request = CompilationRequest::new(consumer_package, vec![source])
            .with_dependency_interfaces([dependency]);

        let consumer = Compilation::load(request)
            .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

        assert!(
            consumer.imported_diagnostics().is_empty(),
            "{:?}",
            consumer.imported_diagnostics()
        );

        let imported = consumer
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported skeleton must build: {error:?}"));

        let skeleton = imported
            .value()
            .as_deref()
            .unwrap_or_else(|| panic!("provider interface must contribute an imported skeleton"));

        assert_eq!(skeleton.structures().len(), 1);
        assert_eq!(skeleton.functions().len(), 1);

        let [inherent_type_member] = skeleton.inherent_type_members() else {
            panic!("provider must export one inherent type-valued member");
        };

        let [trait_type_fulfillment] = skeleton.trait_type_fulfillments() else {
            panic!("provider must export one trait type-valued fulfillment");
        };

        for symbol in [
            AnySymbolId::InherentTypeMember(inherent_type_member.id()),
            AnySymbolId::TraitTypeFulfillment(trait_type_fulfillment.id()),
        ] {
            let semantics = consumer
                .symbol_type_template(symbol)
                .unwrap_or_else(|error| panic!("imported type semantics must resolve: {error:?}"))
                .unwrap_or_else(|| panic!("imported symbol must carry a declared type"));

            assert!(semantics.diagnostics().is_empty(), "{:?}", semantics.diagnostics());
            assert!(matches!(semantics.value(), TypeExpressionTemplate::Resolved(_)));
        }

        let [parameter] = skeleton.callable_parameters() else {
            panic!("provider must export one callable parameter");
        };

        let default = consumer
            .callable_parameter_default(parameter.id())
            .unwrap_or_else(|error| panic!("imported parameter default must resolve: {error:?}"));

        assert!(
            default.diagnostics().is_empty(),
            "{:?}",
            default.diagnostics()
        );

        let CallableParameterDefaultValue::Valid(default) = default.value().value() else {
            panic!("imported parameter default must be valid");
        };

        assert!(matches!(
            default.template_reference(),
            RuntimeDefaultTemplateReference::Interface { .. }
        ));
    }

    #[test]
    fn standard_memory_surface_exports_uninitialized_storage() {
        let compilation = standard_library_compilation([
            include_str!("../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../standard-library/std/src/memory.bray"),
        ]);

        let source_graph = compilation
            .product_source_graph()
            .unwrap_or_else(|error| panic!("standard memory source graph must build: {error:?}"));

        assert!(
            source_graph.diagnostics().is_empty(),
            "standard memory source graph diagnostics: {:?}",
            source_graph.diagnostics()
        );

        let product = compilation
            .product_semantics()
            .unwrap_or_else(|error| panic!("standard memory product semantics must build: {error:?}"));

        assert!(
            product.diagnostics().is_empty(),
            "standard memory product diagnostics: {:?}",
            product.diagnostics()
        );

        assert!(
            !product.value().is_recovered(),
            "standard memory product semantics must not recover"
        );

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("standard memory symbol graph must build: {error:?}"));

        let identity = super::build_identity_surface(
            &compilation,
            symbols,
            product.value().public_symbols(),
        )
        .unwrap_or_else(|error| panic!("standard memory identity surface must build: {error:?}"));

        let request = compilation
            .package_interface_export_request()
            .unwrap_or_else(|| panic!("standard memory export request must exist"));

        let surface = super::build_package_interface_surface(
            request.identity().clone(),
            [],
            identity.symbols,
            identity.relationships,
            identity.exports,
        )
        .unwrap_or_else(|error| panic!("standard memory interface surface must build: {error:?}"));

        let (semantics, _, _) = super::super::semantic::build_semantics(
            &compilation,
            symbols,
            &surface,
            &identity.selected,
            &identity.keys,
        )
        .unwrap_or_else(|error| panic!("standard memory semantic export must build: {error:?}"));

        for (template_index, template) in semantics.checked_templates().iter().enumerate() {
            for (node_index, node) in template.nodes().iter().enumerate() {
                let bray_package_interface::InterfaceCheckedTemplateOperation::Borrow {
                    kind,
                    operand,
                } = node.operation()
                else {
                    continue;
                };

                let operand_index = usize::try_from(operand.raw()).unwrap_or_else(|error| {
                    panic!(
                        "standard memory borrow operand for template {template_index} node {node_index} must fit: {error:?}"
                    )
                });

                let operand_ty = template.nodes()[operand_index].ty();

                let node_ty = semantics.types().iter().enumerate().find_map(|(index, ty)| {
                    u32::try_from(index)
                        .ok()
                        .filter(|index| {
                            bray_package_interface::InterfaceTypeId::new(*index) == node.ty()
                        })
                        .map(|_| ty)
                });

                assert!(
                    matches!(
                        node_ty,
                        Some(bray_package_interface::InterfaceType::Borrow {
                            kind: type_kind,
                            target,
                        }) if *type_kind == *kind && *target == operand_ty
                    ),
                    "standard memory borrow template {template_index} node {node_index} kind {kind:?} operand type {operand_ty:?} must match node type {node_ty:?}"
                );
            }
        }

        assert_strictly_canonical("constraints", semantics.constraints());
        assert_strictly_canonical("callable contracts", semantics.callable_contracts());
        assert_strictly_canonical("callable signatures", semantics.callable_signatures());
        assert_strictly_canonical("generic declarations", semantics.generic_declarations());

        assert_strictly_canonical(
            "callable parameter defaults",
            semantics.callable_parameter_defaults(),
        );

        assert_strictly_canonical("predicate definitions", semantics.predicate_definitions());
        assert_strictly_canonical("declared types", semantics.declared_types());
        assert_strictly_canonical("type representations", semantics.type_representations());
        assert_strictly_canonical("implementations", semantics.implementations());
        assert_strictly_canonical("coherence", semantics.coherence());
        assert_strictly_canonical("target dependencies", semantics.target_dependencies());
        assert_strictly_canonical("ABI dependencies", semantics.abi_dependencies());
        assert_strictly_canonical("runtime requirements", semantics.runtime_requirements());
        assert_strictly_canonical("provenance", semantics.provenance());

        compilation
            .package_implementation_configuration(None)
            .unwrap_or_else(|error| {
                panic!("standard memory implementation configuration must build: {error:?}")
            });

        let _ = export(&compilation);
    }

    #[test]
    fn standard_memory_api_fixture_checks() {
        let compilation = standard_library_compilation([
            include_str!("../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../standard-library/std/src/memory.bray"),
            include_str!("../../../../../standard-library/std/tests/api/memory.bray"),
        ]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "standard memory API diagnostics: {:?}",
            compilation.check_diagnostics()
        );

        let mut recovered = Vec::new();

        walk_syntax_tree(compilation.syntax_tree(), |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && node.is_recovered()
            {
                recovered.push((node.kind(), node.full_range()));
            }

            SyntaxWalkControl::Continue
        });

        assert!(
            recovered.is_empty(),
            "standard memory API syntax must not recover: {recovered:?}"
        );
    }

    #[test]
    fn standard_formatting_surface_round_trips_and_specializes_without_provider_source() {
        let provider = standard_library_compilation([
            include_str!("../../../../../standard-library/std/src/std.bray"),
            concat!(
                "module std.memory;\n",
                "union MemoryLayoutError\n",
                "{\n",
                "    SizeOverflow;\n",
                "    UnsupportedAlignment;\n",
                "}\n",
                "extern func slice_length<T>(pos values: &[T]) -> usize;\n",
                "extern func byte_slice_pointer_mut(pos bytes: &mut [u8]) -> RawPointer<u8>;\n",
                "extern trusted func byte_slice_copy(\n",
                "    pos source: &[u8],\n",
                "    destination: RawPointer<u8>,\n",
                ");\n",
                "extern trusted func byte_buffer_fill(\n",
                "    destination: RawPointer<u8>,\n",
                "    value: u8,\n",
                "    count: usize,\n",
                ");\n",
            ),
            concat!(
                "module std.bytes;\n",
                "using std.memory;\n",
                "struct Buffer\n",
                "{\n",
                "    internal value: bool;\n",
                "\n",
                "    construct(capacity: usize = 0)\n",
                "        -> Result<Self, std.memory.MemoryLayoutError>\n",
                "    {\n",
                "        let buffer: Buffer =\n",
                "        {\n",
                "            value = false,\n",
                "        };\n",
                "\n",
                "        return Ok(buffer);\n",
                "    }\n",
                "}\n",
                "extern func as_slice(pos buffer: &Buffer) -> &[u8];\n",
                "extern func length(pos buffer: &Buffer) -> usize;\n",
                "extern func slice_length(pos bytes: &[u8]) -> usize;\n",
                "extern func push(pos buffer: &mut Buffer, value: u8)\n",
                "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
                "extern func append(pos buffer: &mut Buffer, bytes: &[u8])\n",
                "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
                "extern func append_repeated(pos buffer: &mut Buffer, value: u8, count: usize)\n",
                "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
                "extern func reserve(pos buffer: &mut Buffer, additional: usize)\n",
                "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
                "extern func resize(pos buffer: &mut Buffer, new_length: usize, fill: u8 = 0)\n",
                "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
            ),
            concat!(
                "module std.string;\n",
                "union Utf8Error\n",
                "{\n",
                "    InvalidEncoding;\n",
                "}\n",
                "extern func utf8(pos value: &string) -> &[u8];\n",
                "extern func from_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>;\n",
            ),
            include_str!("../../../../../standard-library/std/src/character.bray"),
            include_str!("../../../../../standard-library/std/src/numeric/checked.bray"),
            include_str!("../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../standard-library/std/src/format/options.bray"),
            include_str!("../../../../../standard-library/std/src/format/argument.bray"),
            include_str!("../../../../../standard-library/std/src/format/sink.bray"),
            include_str!("../../../../../standard-library/std/src/format/integer_width.bray"),
            include_str!("../../../../../standard-library/std/src/format/rendering.bray"),
            concat!(
                "trusted module std.io;\n",
                "union IoErrorKind\n",
                "{\n",
                "    BrokenStream;\n",
                "}\n",
                "struct IoError\n",
                "{\n",
                "    kind: IoErrorKind;\n",
                "    transferred: usize;\n",
                "}\n",
                "trait Writer\n",
                "{\n",
                "    mut func write(pos source: &[u8]) -> Result<usize, IoError>\n",
                "        requires(blocking_execution());\n",
                "    mut func flush() -> Result<unit, IoError>\n",
                "        requires(blocking_execution());\n",
                "}\n",
                "internal func smaller(pos left: usize, pos right: usize) -> usize\n",
                "{\n",
                "    if left < right\n",
                "    {\n",
                "        return left;\n",
                "    }\n",
                "    return right;\n",
                "}\n",
                "internal func advance_write_all(\n",
                "    pos result: Result<usize, IoError>,\n",
                "    written: usize,\n",
                "    remaining: usize,\n",
                ") -> Result<usize, IoError>\n",
                "{\n",
                "    match consume result\n",
                "    {\n",
                "        case Ok(count)\n",
                "        {\n",
                "            if count == 0 || count > remaining\n",
                "            {\n",
                "                return Error({ kind = IoErrorKind.BrokenStream, transferred = written });\n",
                "            }\n",
                "            return Ok(written + count);\n",
                "        }\n",
                "        case Error(error) { return Error(error); }\n",
                "    }\n",
                "}\n",
            ),
            include_str!("../../../../../standard-library/std/src/io/formatting.bray"),
        ]);

        assert!(
            provider.syntax_tree_result().diagnostics().is_empty(),
            "{:?}",
            provider.syntax_tree_result().diagnostics()
        );

        assert!(
            provider.declaration_diagnostics().is_empty(),
            "{:?}",
            provider.declaration_diagnostics()
        );

        let product = provider
            .product_semantics()
            .unwrap_or_else(|error| panic!("formatting product semantics must build: {error:?}"));

        assert!(
            product.diagnostics().is_empty(),
            "{:?}",
            product.diagnostics()
        );

        assert!(!product.value().is_recovered());

        assert!(
            provider.check_diagnostics().is_empty(),
            "{:?}",
            provider.check_diagnostics()
        );

        let adapter = provider
            .lowered_unit(source_named_trait_callable_fulfillment_body_key(
                &provider,
                "WriterFormattingSink",
                "write",
            ))
            .unwrap_or_else(|error| panic!("writer formatting adapter must lower: {error:?}"));

        let adapter = adapter
            .value()
            .as_ref()
            .and_then(bray_lowering::LoweredUnit::mir)
            .unwrap_or_else(|| panic!("writer formatting adapter must produce MIR: {adapter:#?}"));

        assert!(adapter.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Borrow { place, .. }
                if place
                    .projections()
                    .iter()
                    .any(|projection| matches!(projection.kind(), MirProjectionKind::Field(_)))
                    && matches!(
                        place.projections().last().map(bray_ir::MirProjection::kind),
                        Some(MirProjectionKind::Dereference)
                    )
        )), "generic writer field borrow must reach the destination value: {adapter:#?}");

        let interface = export(&provider);

        let runtime_capabilities: BTreeSet<_> = interface
            .semantics()
            .runtime_requirements()
            .iter()
            .flat_map(|requirement| requirement.requirements().capabilities())
            .copied()
            .collect();

        assert!(
            runtime_capabilities
                .contains(&bray_runtime_interface::RuntimeCapability::StringOperations)
        );

        assert!(
            runtime_capabilities
                .contains(&bray_runtime_interface::RuntimeCapability::CharacterOperations)
        );

        let artifact = encode_package_interface(interface)
            .unwrap_or_else(|error| panic!("formatting interface must encode: {error:?}"));

        let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

        let validated = ValidatedPackageInterface::try_new(artifact.bytes(), policy)
            .unwrap_or_else(|error| panic!("formatting interface must validate: {error:?}"));

        let implementation = PackageImplementationArtifact::try_new(
            &validated,
            interface.surface(),
            interface.semantics(),
            interface.implementation_configuration().clone(),
            [],
            interface.executable_templates().iter().cloned(),
            [],
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("formatting implementation must encode: {error:?}"));

        let provider_package = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

        let provider_product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

        let dependency = DependencyInterfaceInput::new(
            provider_package.clone(),
            provider_product,
            "std.brayi",
            artifact.shared_bytes(),
            policy,
        )
        .with_implementation_artifact("std.brayimpl", Arc::new(implementation));

        let consumer_package = PackageIdentity::try_new("example.application")
            .unwrap_or_else(|| panic!("consumer package identity must be valid"));

        let source = SourceInput::virtual_text(
            SourceIdentity::new(0),
            "consumer.bray",
            SourceVersion::new(0),
            concat!(
                "module app;\n",
                "using std.format;\n",
                "using std.format.ByteSinkFormatting;\n",
                "using std.format.StringFormat;\n",
                "using std.format.I32Format;\n",
                "using std.format.U32Format;\n",
                "using std.bytes;\n",
                "using std.io;\n",
                "using std.io.WriterFormattingSink;\n",
                "using std.memory;\n",
                "struct RecordingWriter\n",
                "{\n",
                "    mut written: usize;\n",
                "}\n",
                "impl RecordingWriterIo = RecordingWriter(std.io.Writer)\n",
                "{\n",
                "    mut func write(pos source: &[u8]) -> Result<usize, std.io.IoError>\n",
                "        requires(blocking_execution())\n",
                "    {\n",
                "        let length: usize = std.bytes.slice_length(source);\n",
                "        self.written += length;\n",
                "        return Ok(length);\n",
                "    }\n",
                "    mut func flush() -> Result<unit, std.io.IoError>\n",
                "        requires(blocking_execution())\n",
                "    {\n",
                "        return Ok(unit);\n",
                "    }\n",
                "}\n",
                "func render(pos destination: &mut std.format.ByteSink, pos value: string)\n",
                "    -> Result<unit, std.memory.MemoryLayoutError>\n",
                "    requires(blocking_execution())\n",
                "{\n",
                "    return std.format.write(\n",
                "        destination,\n",
                "        std.format.Argument<string>(&value),\n",
                "    );\n",
                "}\n",
                "func render_integer(pos destination: &mut std.format.ByteSink, pos value: i32)\n",
                "    -> Result<unit, std.memory.MemoryLayoutError>\n",
                "    requires(blocking_execution())\n",
                "{\n",
                "    return std.format.write(\n",
                "        destination,\n",
                "        std.format.Argument<i32>(&value),\n",
                "    );\n",
                "}\n",
                "func resolved_defaults() -> std.format.Options\n",
                "{\n",
                "    return std.format.Options.default();\n",
                "}\n",
                "public trusted func stream_integer(pos writer: &mut RecordingWriter, pos value: u32)\n",
                "    -> Result<unit, std.io.IoError>\n",
                "    requires(blocking_execution())\n",
                "{\n",
                "    let mut destination: std.io.FormattingSink<RecordingWriter> =\n",
                "        std.io.FormattingSink<RecordingWriter>(writer);\n",
                "    return trusted std.format.write_to<\n",
                "        u32,\n",
                "        std.io.FormattingSink<RecordingWriter>,\n",
                "        std.io.IoError\n",
                "    >(\n",
                "        &mut destination,\n",
                "        std.format.Argument<u32>(&value),\n",
                "    );\n",
                "}\n",
            ),
        );

        let options = CompilationOptions::new(
            WorkerBudget::default(),
            ProductKind::Library,
            SelectedTarget::default(),
        );

        let request = CompilationRequest::with_options(consumer_package, vec![source], options)
            .with_dependency_interfaces([dependency]);

        let consumer = Compilation::load(request)
            .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

        assert!(
            consumer.imported_diagnostics().is_empty(),
            "{:?}",
            consumer.imported_diagnostics()
        );

        let imported = consumer
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("formatting skeleton must build: {error:?}"));

        let skeleton = imported
            .value()
            .as_deref()
            .unwrap_or_else(|| panic!("formatting interface must contribute a skeleton"));

        let package = skeleton
            .package_by_identity(&provider_package)
            .unwrap_or_else(|| panic!("standard-library package must be imported"));

        assert!(
            consumer
                .symbol_graph()
                .unwrap_or_else(|error| panic!("consumer source symbols must build: {error:?}"))
                .packages()
                .iter()
                .all(|package| package.identity() != &provider_package),
            "provider symbols must come only from the package interface"
        );

        let format_path = ModulePathKey::try_new(["format"])
            .unwrap_or_else(|| panic!("format module path must be valid"));

        let format = skeleton
            .module_by_path(package.id(), &format_path)
            .unwrap_or_else(|| panic!("format module must be imported"));

        for name in ["Argument", "ByteSink", "ByteSinkFormatting"] {
            assert!(
                matches!(
                    skeleton.lookup(format.id().into(), name),
                    MemberLookupResult::Found(_)
                ),
                "{name} must be supplied by the imported package interface"
            );
        }

        let bytes_path = ModulePathKey::try_new(["bytes"])
            .unwrap_or_else(|| panic!("bytes module path must be valid"));

        let bytes = skeleton
            .module_by_path(package.id(), &bytes_path)
            .unwrap_or_else(|| panic!("bytes module must be imported"));

        assert!(matches!(
            skeleton.lookup(bytes.id().into(), "slice_length"),
            MemberLookupResult::Found(_)
        ));

        let io_path = ModulePathKey::try_new(["io"])
            .unwrap_or_else(|| panic!("io module path must be valid"));

        let io = skeleton
            .module_by_path(package.id(), &io_path)
            .unwrap_or_else(|| panic!("io module must be imported"));

        for name in ["IoError", "Writer", "WriterFormattingSink"] {
            assert!(
                matches!(
                    skeleton.lookup(io.id().into(), name),
                    MemberLookupResult::Found(_)
                ),
                "{name} must be supplied by the imported package interface"
            );
        }

        assert!(
            consumer.check_diagnostics().is_empty(),
            "{:?}",
            consumer.check_diagnostics()
        );

        let lowered = consumer
            .lowered_unit(source_function_body_key(&consumer, "resolved_defaults"))
            .unwrap_or_else(|error| panic!("imported named constructor must lower: {error:?}"));

        assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        assert!(!skeleton.traits().is_empty());
        assert!(!skeleton.structures().is_empty());
        assert!(!skeleton.named_trait_implementations().is_empty());

        let streamed = consumer
            .lowered_unit(source_function_body_key(&consumer, "stream_integer"))
            .unwrap_or_else(|error| panic!("imported formatting adapter must lower: {error:?}"));

        assert!(streamed.value().is_some(), "{:#?}", streamed.diagnostics());
        assert!(streamed.diagnostics().is_empty(), "{:#?}", streamed.diagnostics());

        let imported_instances = consumer
            .imported_codegen_instance_count_for_test()
            .unwrap_or_else(|error| panic!("imported formatting reachability must close: {error:?}"));

        assert!(imported_instances > 0);
    }

    #[test]
    fn non_library_products_cannot_export_package_interfaces() {
        for product_kind in [ProductKind::Executable, ProductKind::Test] {
            let compilation = compilation_from_sources_for_product(["module app;"], product_kind);

            assert_eq!(
                compilation.package_interface_export_bundle(),
                Some(&Err(super::PackageInterfaceExportError::InvalidCompilation))
            );
        }
    }

    #[test]
    fn target_gated_contributions_do_not_invalidate_package_interface_export() {
        let compilation = compilation_from_sources([
            concat!(
                "@target(false)\n",
                "module app;\n",
                "\n",
                "func disabled()\n",
                "{\n",
                "}\n",
            ),
            concat!(
                "@target(target.pointer.BITS == 64)\n",
                "module app;\n",
                "\n",
                "func enabled()\n",
                "{\n",
                "}\n",
            ),
        ]);

        let bundle = export(&compilation);

        assert_eq!(bundle.surface().symbols().symbols().len(), 3);

        let [dependency] = bundle.semantics().target_dependencies() else {
            panic!("selected contribution must retain one exact target dependency");
        };

        let package = ExternalSymbolKey::package(
            PackageIdentity::try_new("example.package")
                .unwrap_or_else(|| panic!("test package identity must be valid")),
        );

        let module = ExternalSymbolKey::module(
            package,
            ModulePathKey::try_new(["app"])
                .unwrap_or_else(|| panic!("test module path must be valid")),
        )
        .unwrap_or_else(|| panic!("test module key must be valid"));

        let enabled = ExternalSymbolKey::named(
            module,
            SymbolKind::Function,
            SymbolName::try_new("enabled")
                .unwrap_or_else(|| panic!("test function name must be valid")),
        )
        .unwrap_or_else(|| panic!("test function key must be valid"));

        let owner = bundle
            .surface()
            .symbol_by_external_key(&enabled)
            .unwrap_or_else(|| panic!("enabled function must be exported"));

        assert_eq!(dependency.owner(), &InterfaceSymbolReference::Local(owner));

        let InterfaceSymbolReference::CompilerKnown(semantics) = dependency.property() else {
            panic!("target dependency must retain its compiler-known semantics");
        };

        let declaration = CompilerKnownDeclarationKey::try_new("TargetPointerBits")
            .unwrap_or_else(|| panic!("target pointer-bits key must be valid"));

        let semantic_key = SymbolKey::compiler_known_declaration(declaration, SymbolKind::Constant)
            .unwrap_or_else(|| panic!("target pointer-bits symbol key must be valid"));

        assert_eq!(semantics.key(), &semantic_key);

        let value = bundle
            .semantics()
            .constant_values()
            .get(
                usize::try_from(dependency.value().raw())
                    .unwrap_or_else(|_| panic!("target dependency value ID must fit usize")),
            )
            .unwrap_or_else(|| panic!("target dependency value must be exported"));

        assert_eq!(
            value.kind(),
            &InterfaceConstantValueKind::Integer(IntegerConstant::from_u64(64))
        );
    }

    #[test]
    fn runtime_defaults_export_after_disabled_target_gated_contributions() {
        let compilation = compilation_from_sources([
            concat!(
                "@target(false)\n",
                "module app;\n",
                "\n",
                "func disabled()\n",
                "{\n",
                "}\n",
            ),
            concat!(
                "module app;\n",
                "\n",
                "func selected(pos value: i64? = none) -> i64?\n",
                "{\n",
                "    return value;\n",
                "}\n",
            ),
        ]);

        let bundle = export(&compilation);

        assert_eq!(
            bundle.semantics().callable_parameter_defaults().len(),
            1
        );
    }

    fn export(compilation: &Compilation) -> &Arc<PackageInterfaceExportBundle> {
        match compilation.package_interface_export_bundle() {
            Some(Ok(bundle)) => bundle,
            Some(Err(error)) => panic!("test library interface must build: {error:?}"),
            None => panic!("test compilation must configure a library interface"),
        }
    }

    fn assert_strictly_canonical<T>(table: &str, values: &[T])
    where
        T: std::fmt::Debug + Ord,
    {
        if let Some(pair) = values.windows(2).find(|pair| pair[0] >= pair[1]) {
            panic!(
                "standard memory {table} are not canonical: {:?} then {:?}",
                pair[0], pair[1]
            );
        }
    }

    fn compilation(source: &str) -> Compilation {
        compilation_from_sources([source])
    }

    fn compilation_from_sources<const N: usize>(sources: [&str; N]) -> Compilation {
        compilation_from_sources_for_product(sources, ProductKind::Library)
    }

    fn compilation_from_sources_for_product<const N: usize>(
        sources: [&str; N],
        product_kind: ProductKind,
    ) -> Compilation {
        let package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
            package.clone(),
            package_version(),
            product,
            bray_package_interface::InterfaceProductKind::Library,
            "public",
        )
        .unwrap_or_else(|| panic!("test export identity must be valid"));

        let export =
            PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

        let sources = test_source_inputs("test", sources);

        let options = CompilationOptions::new(
            WorkerBudget::default(),
            product_kind,
            SelectedTarget::default(),
        );

        let request = CompilationRequest::with_options(package, sources, options)
            .with_package_interface_export(export);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn standard_library_compilation<const N: usize>(sources: [&str; N]) -> Compilation {
        let package = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

        let product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

        let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
            package.clone(),
            package_version(),
            product,
            bray_package_interface::InterfaceProductKind::Library,
            "public",
        )
        .unwrap_or_else(|| panic!("standard-library export identity must be valid"));

        let export =
            PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

        let sources = test_source_inputs("standard", sources);

        let request = CompilationRequest::new(package, sources)
            .with_standard_library_source_authority()
            .with_package_interface_export(export);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("standard-library compilation must load: {error:?}"))
    }
}
