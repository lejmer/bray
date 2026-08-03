use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_package_interface::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    ExportedLookupKind, InterfaceDependency, InterfaceProductKind, PackageInterfaceExportBundle,
    SymbolRelationshipKind, build_package_interface_surface,
};
use bray_symbols::{
    AnySymbolId, ExternalSymbolKey, ModuleSurfaceFact, ModuleSymbolId, ProductKind,
    SymbolFactRequest, SymbolKeyData, SymbolKind, SymbolOrdinal, SymbolOrigin,
    SynthesizedSymbolRole,
};

use super::PackageInterfaceExportError;
use crate::compilation::Compilation;
use crate::fact::CompilationFactKey;

impl Compilation {
    /// Returns the current library product's interface export, when configured.
    pub fn package_interface_export_bundle(
        &self,
    ) -> Option<&Result<Arc<PackageInterfaceExportBundle>, PackageInterfaceExportError>> {
        let request = self.state.package_interface_export.as_ref()?;

        Some(self.fact(
            CompilationFactKey::PackageInterfaceExportBundle,
            &self.state.package_interface_export_bundle,
            || self.build_package_interface_export_bundle(request),
        ))
    }

    fn build_package_interface_export_bundle(
        &self,
        request: &crate::PackageInterfaceExportRequest,
    ) -> Result<Arc<PackageInterfaceExportBundle>, PackageInterfaceExportError> {
        if self.options().product_kind() != ProductKind::Library
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
            .product_semantic_facts()
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

        let semantic_facts = super::semantic::build_semantic_facts(
            self,
            symbols,
            &surface,
            &identity.selected,
            &identity.keys,
        )?;

        PackageInterfaceExportBundle::try_new(surface, semantic_facts, request.language_revision())
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

fn build_identity_surface(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    public_symbols: &[AnySymbolId],
) -> Result<ExportIdentitySurface, PackageInterfaceExportError> {
    let package_key = ExternalSymbolKey::package(compilation.package_identity().clone());

    let package_symbol = graph
        .packages()
        .iter()
        .find(|package| package.identity() == compilation.package_identity())
        .map(|package| AnySymbolId::from(package.id()))
        .ok_or(PackageInterfaceExportError::InvalidCompilation)?;

    let mut selected = BTreeSet::from_iter(public_symbols.iter().copied());

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

fn select_required_children(
    graph: &bray_symbols::SymbolGraph,
    owner: AnySymbolId,
    selected: &mut BTreeSet<AnySymbolId>,
) {
    for child in graph.declaration_children(owner).iter().copied() {
        if matches!(
            child.kind(),
            SymbolKind::ReceiverParameter
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
            .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationFacts(symbol.kind()))?;

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
        .containing_symbol(symbol)
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationFacts(symbol.kind()))?;

    let owner_key = external_key(graph, owner, keys)?;

    let key = match graph.symbol_key(symbol).map(|key| key.data()) {
        Some(SymbolKeyData::Module { path, .. }) => {
            ExternalSymbolKey::module(owner_key, path.clone())
        }
        Some(SymbolKeyData::Synthesized(synthesized)) => ExternalSymbolKey::synthesized(
            owner_key,
            synthesized.role(),
            external_synthesized_ordinal(synthesized.role(), synthesized.ordinal()),
        ),
        Some(SymbolKeyData::SourceDeclaration { .. }) => {
            external_declaration_key(graph, owner, owner_key, symbol)
        }
        _ => None,
    }
    .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationFacts(symbol.kind()))?;

    keys.insert(symbol, key.clone());

    Ok(key)
}

fn external_synthesized_ordinal(
    role: SynthesizedSymbolRole,
    ordinal: Option<SymbolOrdinal>,
) -> Option<SymbolOrdinal> {
    match role {
        SynthesizedSymbolRole::ReceiverParameter => None,
        SynthesizedSymbolRole::CallableParameterDefaultProvider
        | SynthesizedSymbolRole::StructFieldDefaultProvider
        | SynthesizedSymbolRole::UnionPayloadDefaultProvider => Some(SymbolOrdinal::new(0)),
        _ => ordinal,
    }
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
            | SymbolKind::Constructor
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
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationFacts(symbol.kind()))?;

    let containing_symbol =
        match graph.containing_symbol(symbol) {
            Some(owner) => Some(keys.get(&owner).cloned().ok_or(
                PackageInterfaceExportError::IncompletePublicDeclarationFacts(owner.kind()),
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
    let Some(owner) = graph.containing_symbol(member) else {
        return Ok(None);
    };

    if !selected.contains(&owner) {
        return Ok(None);
    }

    let Some(kind) = SymbolRelationshipKind::between(owner.kind(), member.kind()) else {
        return Ok(None);
    };

    let ordinal = relationship_ordinal(graph, owner, member)
        .map(SymbolOrdinal::raw)
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationFacts(member.kind()))?;

    let owner_key = keys
        .get(&owner)
        .cloned()
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationFacts(owner.kind()))?;

    let member_key = keys
        .get(&member)
        .cloned()
        .ok_or(PackageInterfaceExportError::IncompletePublicDeclarationFacts(member.kind()))?;

    let relationship = ExportRelationshipInput::new(kind, owner_key, member_key, ordinal);

    Ok(Some(with_field_properties(graph, member, relationship)))
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
                PackageInterfaceExportError::IncompletePublicDeclarationFacts(owner.kind()),
            )?;

            let target = keys.get(&symbol).cloned().ok_or(
                PackageInterfaceExportError::IncompletePublicDeclarationFacts(symbol.kind()),
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
                    PackageInterfaceExportError::IncompletePublicDeclarationFacts(
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
        let facts = self
            .binder_facts(&self.state.cancellation)
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        let mut exports = Vec::new();

        for (module, owner_key) in module_keys {
            let surface = facts
                .symbol_fact(SymbolFactRequest::<ModuleSurfaceFact>::new(*module))
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
                        PackageInterfaceExportError::IncompletePublicDeclarationFacts(
                            edge.target().kind(),
                        ),
                    );
                };

                let Some(target_key) = module_keys.get(&target) else {
                    return Err(
                        PackageInterfaceExportError::IncompletePublicDeclarationFacts(
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
    use std::sync::Arc;

    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
        PackageInterfaceExportBundle, ValidatedPackageInterface, encode_package_interface,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{
        AnySymbolId, CallableParameterDefaultValue, PackageIdentity, ProductKind,
        RuntimeDefaultTemplateReference, TypeExpressionTemplate,
    };

    use crate::{
        Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput,
        PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
    };

    #[test]
    fn module_only_library_exports_are_lazy_cached_facts() {
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
    fn internal_owner_chains_do_not_enter_the_public_export_selection() {
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

        assert_eq!(bundle.surface().symbols().symbols().len(), 2);
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
    fn public_callable_and_type_facts_round_trip_without_source() {
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

        let facts = validated
            .decode_semantic_facts(&surface)
            .unwrap_or_else(|error| panic!("semantic facts must decode: {error:?}"));

        assert_eq!(facts.callable_signatures().len(), 2);
        assert_eq!(facts.generic_declarations().len(), 4);
        assert_eq!(facts.constraints().len(), 1);
        assert_eq!(facts.checked_templates().len(), 4);
        assert_eq!(facts.declaration_templates().len(), 4);
        assert_eq!(facts.declared_types().len(), 2);
        assert_eq!(facts.type_representations().len(), 2);
    }

    #[test]
    fn exported_callable_and_type_facts_intern_without_provider_source() {
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
            let fact = consumer
                .symbol_type_template(symbol)
                .unwrap_or_else(|error| panic!("imported type fact must resolve: {error:?}"))
                .unwrap_or_else(|| panic!("imported symbol must carry a declared type"));

            assert!(fact.diagnostics().is_empty(), "{:?}", fact.diagnostics());
            assert!(matches!(fact.value(), TypeExpressionTemplate::Resolved(_)));
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
    fn non_library_products_cannot_export_package_interfaces() {
        for product_kind in [ProductKind::Executable, ProductKind::Test] {
            let compilation = compilation_from_sources_for_product(["module app;"], product_kind);

            assert_eq!(
                compilation.package_interface_export_bundle(),
                Some(&Err(super::PackageInterfaceExportError::InvalidCompilation))
            );
        }
    }

    fn export(compilation: &Compilation) -> &Arc<PackageInterfaceExportBundle> {
        match compilation.package_interface_export_bundle() {
            Some(Ok(bundle)) => bundle,
            Some(Err(error)) => panic!("test library interface must build: {error:?}"),
            None => panic!("test compilation must configure a library interface"),
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
            product,
            bray_package_interface::InterfaceProductKind::Library,
            "public",
        )
        .unwrap_or_else(|| panic!("test export identity must be valid"));

        let export =
            PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

        let sources = sources.into_iter().enumerate().map(|(index, source)| {
            let index = u32::try_from(index)
                .unwrap_or_else(|_| panic!("test source count must fit source identities"));

            SourceInput::virtual_text(
                SourceIdentity::new(index),
                format!("test-{index}.bray"),
                SourceVersion::new(0),
                source,
            )
        });

        let options = CompilationOptions::new(
            WorkerBudget::default(),
            product_kind,
            SelectedTarget::default(),
        );

        let request = CompilationRequest::with_options(package, sources.collect(), options)
            .with_package_interface_export(export);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }
}
