use crate::compilation::binder::BindingQueryResult;
use std::sync::Arc;

use bray_binder::{
    BindingError, BindingQueryContext, BindingQueryError, ImportedPathRoot, NameAccess,
    SymbolQueryProvider, bind_owner_surface_path, bind_surface_path_with_re_exports,
};
use bray_declarations::DeclarationTable;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractSymbolId, CallableContractTypeQuery,
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbol, CallableParameterSymbolId,
    FunctionSymbol, FunctionSymbolId, ImportedSemanticAddress, ImportedSymbolSkeleton,
    MemberLookupResult, ModuleSurfaceQuery, ModuleSymbolId, NamedTypeSymbolId,
    ReceiverParameterSymbol, ReceiverParameterSymbolId, SemanticValueStore, StructFieldSymbol,
    StructFieldSymbolId, StructSymbol, StructSymbolId, SymbolGraph, SymbolQueryRequest,
    TypeAssociatedSurface, TypeExpressionTemplate, UnionPayloadFieldSymbol,
    UnionPayloadFieldSymbolId, UnionSymbol, UnionSymbolId, UnionVariantSymbol,
    UnionVariantSymbolId,
};
use bray_syntax::{PathSyntax, SyntaxTree};

use super::super::Compilation;
use super::CompilationSymbolSemantics;
use crate::fact::{CancellationToken, FactQueryError};

#[derive(Clone, Copy)]
pub(in crate::compilation) struct CompilationBindingContext<'compilation> {
    pub(super) compilation: &'compilation Compilation,
    declarations: &'compilation DeclarationTable,
    pub(super) symbols: &'compilation SymbolGraph,
    pub(super) semantic_values: &'compilation SemanticValueStore,
    pub(super) symbol_semantics: &'compilation CompilationSymbolSemantics,
    pub(super) cancellation: &'compilation CancellationToken,
}

impl<'compilation> CompilationBindingContext<'compilation> {
    pub(super) const fn new(
        compilation: &'compilation Compilation,
        declarations: &'compilation DeclarationTable,
        symbols: &'compilation SymbolGraph,
        semantic_values: &'compilation SemanticValueStore,
        symbol_semantics: &'compilation CompilationSymbolSemantics,
        cancellation: &'compilation CancellationToken,
    ) -> Self {
        Self {
            compilation,
            declarations,
            symbols,
            semantic_values,
            symbol_semantics,
            cancellation,
        }
    }

    pub(in crate::compilation) const fn compilation(&self) -> &'compilation Compilation {
        self.compilation
    }

    pub(in crate::compilation) fn bind_surface_path(
        &self,
        module: ModuleSymbolId,
        path: &PathSyntax,
        access: NameAccess,
    ) -> BindingQueryResult<DiagnosticResult<MemberLookupResult<AnySymbolId>>> {
        bind_surface_path_with_re_exports(
            self,
            module,
            path,
            access,
            &mut |module, name, access| self.module_re_export_lookup(module, name, access),
        )
    }

    pub(in crate::compilation) fn bind_owner_surface_path(
        &self,
        owner: AnySymbolId,
        path: &PathSyntax,
        access: NameAccess,
    ) -> BindingQueryResult<DiagnosticResult<MemberLookupResult<AnySymbolId>>> {
        bind_owner_surface_path(self, owner, path, access)
    }

    pub(in crate::compilation) const fn declarations(&self) -> &'compilation DeclarationTable {
        self.declarations
    }

    pub(super) fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BindingQueryResult<Option<ImportedPathRoot<'_>>> {
        let mut package_prefix = String::new();
        let mut selected = None;

        for component in components {
            if !package_prefix.is_empty() {
                package_prefix.push('.');
            }

            package_prefix.push_str(component);

            let dependency = self
                .compilation
                .dependency_interfaces()
                .binary_search_by(|dependency| {
                    dependency.package().as_str().cmp(package_prefix.as_str())
                })
                .ok();

            if let Some(dependency) = dependency {
                selected = Some(dependency);
            }
        }

        let Some(dependency) = selected else {
            return Ok(None);
        };

        let identity = self.compilation.dependency_interfaces()[dependency].package();

        let Some(symbols) = self.imported_symbols()? else {
            return Ok(None);
        };

        let package = symbols.package_by_identity(identity).ok_or_else(|| {
            // Package identities are Arc-backed and retained only on this failure path.
            BindingQueryError::Binding(BindingError::ImportedPackageUnavailable(identity.clone()))
        })?;

        ImportedPathRoot::for_path(symbols, package.id(), components)
            .map(Some)
            .ok_or(BindingQueryError::Binding(
                BindingError::ImportedPathUnavailable {
                    package: package.id(),
                    component_count: components.len(),
                },
            ))
    }

    pub(in crate::compilation) fn imported_symbols(
        &self,
    ) -> BindingQueryResult<Option<&ImportedSymbolSkeleton>> {
        let result = self
            .compilation
            .imported_symbol_skeleton_result_with_cancellation(self.cancellation)
            .map_err(super::symbol::binder_error)?;

        Ok(result.value().as_deref())
    }

    pub(in crate::compilation) fn imported_semantic_address(
        &self,
        symbol: AnySymbolId,
    ) -> BindingQueryResult<Option<ImportedSemanticAddress>> {
        if self.symbols.symbol_key(symbol).is_some() {
            return Ok(None);
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.imported_semantic_address(symbol)))
    }

    pub(in crate::compilation) fn lookup_member(
        &self,
        owner: AnySymbolId,
        name: &str,
    ) -> BindingQueryResult<MemberLookupResult<AnySymbolId>> {
        if self.symbols.symbol_key(owner).is_some() {
            return Ok(self.symbols.lookup_member(owner, name));
        }

        Ok(self
            .imported_symbols()?
            .map_or(MemberLookupResult::NotFound, |symbols| {
                symbols.lookup_member(owner, name)
            }))
    }

    pub(in crate::compilation) fn member_name(
        &self,
        member: AnySymbolId,
    ) -> BindingQueryResult<Option<&bray_symbols::SymbolName>> {
        if let Some(name) = self.symbols.member_name(member) {
            return Ok(Some(name));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.member_name(member)))
    }

    pub(in crate::compilation) fn structure(
        &self,
        id: StructSymbolId,
    ) -> BindingQueryResult<Option<&StructSymbol>> {
        if let Some(record) = self.symbols.structure(id) {
            return Ok(Some(record));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.structure(id)))
    }

    pub(in crate::compilation) fn struct_field(
        &self,
        id: StructFieldSymbolId,
    ) -> BindingQueryResult<Option<&StructFieldSymbol>> {
        if let Some(record) = self.symbols.struct_field(id) {
            return Ok(Some(record));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.struct_field(id)))
    }

    pub(in crate::compilation) fn function(
        &self,
        id: FunctionSymbolId,
    ) -> BindingQueryResult<Option<&FunctionSymbol>> {
        if let Some(record) = self.symbols.function(id) {
            return Ok(Some(record));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.function(id)))
    }

    pub(in crate::compilation) fn callable_parameter(
        &self,
        id: CallableParameterSymbolId,
    ) -> BindingQueryResult<Option<&CallableParameterSymbol>> {
        if let Some(record) = self.symbols.callable_parameter(id) {
            return Ok(Some(record));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.callable_parameter(id)))
    }

    pub(in crate::compilation) fn receiver_parameter(
        &self,
        id: ReceiverParameterSymbolId,
    ) -> BindingQueryResult<Option<&ReceiverParameterSymbol>> {
        if let Some(record) = self.symbols.receiver_parameter(id) {
            return Ok(Some(record));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.receiver_parameter(id)))
    }

    pub(in crate::compilation) fn union(
        &self,
        id: UnionSymbolId,
    ) -> BindingQueryResult<Option<&UnionSymbol>> {
        if let Some(record) = self.symbols.union(id) {
            return Ok(Some(record));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.union(id)))
    }

    pub(in crate::compilation) fn union_variant(
        &self,
        id: UnionVariantSymbolId,
    ) -> BindingQueryResult<Option<&UnionVariantSymbol>> {
        if let Some(record) = self.symbols.union_variant(id) {
            return Ok(Some(record));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.union_variant(id)))
    }

    pub(in crate::compilation) fn union_payload_field(
        &self,
        id: UnionPayloadFieldSymbolId,
    ) -> BindingQueryResult<Option<&UnionPayloadFieldSymbol>> {
        if let Some(record) = self.symbols.union_payload_field(id) {
            return Ok(Some(record));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.union_payload_field(id)))
    }
}

impl BindingQueryContext for CompilationBindingContext<'_> {
    type UpstreamError = FactQueryError;
    type SymbolSemantics = Self;
    type Cancellation = CancellationToken;

    fn syntax(&self) -> &SyntaxTree {
        self.compilation.syntax_tree()
    }

    fn declarations(&self) -> &DeclarationTable {
        self.declarations
    }

    fn symbols(&self) -> &SymbolGraph {
        self.symbols
    }

    fn symbol_key(
        &self,
        symbol: AnySymbolId,
    ) -> BindingQueryResult<Option<&bray_symbols::SymbolKey>> {
        if let Some(key) = self.symbols.symbol_key(symbol) {
            return Ok(Some(key));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.symbol_key(symbol)))
    }

    fn symbol_is_recovered(&self, symbol: AnySymbolId) -> BindingQueryResult<Option<bool>> {
        if let Some(is_recovered) = self.symbols.symbol_is_recovered(symbol) {
            return Ok(Some(is_recovered));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.symbol_is_recovered(symbol)))
    }

    fn containing_symbol(&self, symbol: AnySymbolId) -> BindingQueryResult<Option<AnySymbolId>> {
        if let Some(owner) = self.symbols.containing_symbol(symbol) {
            return Ok(Some(owner));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.containing_symbol(symbol)))
    }

    fn callable_parameter_default_provider(
        &self,
        parameter: CallableParameterSymbolId,
    ) -> BindingQueryResult<Option<CallableParameterDefaultProviderSymbolId>> {
        if let Some(provider) = self
            .symbols
            .callable_parameter(parameter)
            .and_then(|parameter| parameter.default_provider())
        {
            return Ok(Some(provider));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.callable_parameter(parameter))
            .and_then(|parameter| parameter.default_provider()))
    }

    fn runtime_default_subject(
        &self,
        provider: AnySymbolId,
    ) -> BindingQueryResult<Option<AnySymbolId>> {
        if let Some(subject) = self.symbols.runtime_default_subject(provider) {
            return Ok(Some(subject));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.runtime_default_subject(provider)))
    }

    fn lookup_member(
        &self,
        owner: AnySymbolId,
        name: &str,
    ) -> BindingQueryResult<MemberLookupResult<AnySymbolId>> {
        CompilationBindingContext::lookup_member(self, owner, name)
    }

    fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BindingQueryResult<Option<ImportedPathRoot<'_>>> {
        CompilationBindingContext::imported_path_root(self, components)
    }

    fn imported_symbols(&self) -> BindingQueryResult<Option<&ImportedSymbolSkeleton>> {
        CompilationBindingContext::imported_symbols(self)
    }

    fn callable_contract_type(
        &self,
        definition: CallableContractSymbolId,
    ) -> BindingQueryResult<Arc<DiagnosticResult<TypeExpressionTemplate>>> {
        SymbolQueryProvider::resolve_symbol_query(
            self,
            SymbolQueryRequest::<CallableContractTypeQuery>::new(definition),
        )
    }

    fn callable_contract_input_signature(
        &self,
        owner: AnySymbolId,
        source: bray_declarations::SyntaxAnchor,
    ) -> BindingQueryResult<DiagnosticResult<bray_symbols::CallableTypeTemplate>> {
        let root = self
            .syntax()
            .find_node(
                source.source_id(),
                source.syntax_kind(),
                source.full_range(),
                source.is_recovered(),
            )
            .ok_or(BindingQueryError::Binding(BindingError::SyntaxContract(
                source,
            )))?;

        super::type_binder(self, owner)?.bind_callable_contract_input_signature(root)
    }

    fn callable_type_contract(
        &self,
        owner: AnySymbolId,
        source: bray_declarations::SyntaxAnchor,
        signature: &bray_symbols::CallableTypeTemplate,
    ) -> BindingQueryResult<DiagnosticResult<bray_symbols::CallableContractSet>> {
        super::symbol::bind_callable_type_contract(self, owner, source, signature)
    }

    fn module_re_export_lookup(
        &self,
        module: ModuleSymbolId,
        name: &str,
        access: NameAccess,
    ) -> BindingQueryResult<MemberLookupResult<AnySymbolId>> {
        let surface =
            self.resolve_symbol_query(SymbolQueryRequest::<ModuleSurfaceQuery>::new(module))?;

        Ok(match access {
            NameAccess::Public => surface.value().lookup_public(name),
            NameAccess::Internal => surface.value().lookup(name),
        })
    }

    fn type_associated_surface(
        &self,
        subject: NamedTypeSymbolId,
    ) -> BindingQueryResult<Arc<DiagnosticResult<TypeAssociatedSurface>>> {
        self.compilation
            .type_associated_surface_result_with_cancellation(subject, self.cancellation)
            .map_err(super::symbol::binder_error)
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn selected_target(&self) -> &bray_target::TargetProfile {
        self.compilation.selected_target().target().profile()
    }

    fn symbol_semantics(&self) -> &Self::SymbolSemantics {
        self
    }

    fn cancellation(&self) -> &Self::Cancellation {
        self.cancellation
    }
}

impl Compilation {
    pub(in crate::compilation) fn binding_context_for<'compilation>(
        &'compilation self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBindingContext<'compilation>, FactQueryError> {
        let binding_context = if key.kind() == bray_bound_tree::BoundUnitKind::TargetGate {
            self.discovery_binding_context(cancellation)?
        } else {
            self.binding_context(cancellation)?
        };

        if !binding_context
            .symbols
            .contains_symbol_key(key.declared_owner())
        {
            return Err(crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Unit(key.clone()),
                crate::compilation::SemanticQueryViolation::Missing(
                    crate::compilation::SemanticDataKind::SymbolKey,
                ),
            )
            .into());
        }

        Ok(binding_context)
    }

    pub(in crate::compilation) fn binding_context<'compilation>(
        &'compilation self,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBindingContext<'compilation>, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let semantic_values = self.semantic_value_store()?;
        let declarations = self.product_source_graph()?.declarations();

        Ok(CompilationBindingContext::new(
            self,
            declarations,
            symbols,
            semantic_values,
            &self.state.symbol_semantics,
            cancellation,
        ))
    }

    pub(in crate::compilation) fn discovery_binding_context<'compilation>(
        &'compilation self,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBindingContext<'compilation>, FactQueryError> {
        let symbols = self.discovery_symbol_graph()?;
        let semantic_values = self.semantic_value_store()?;

        Ok(CompilationBindingContext::new(
            self,
            self.declaration_table(),
            symbols,
            semantic_values,
            &self.state.discovery_symbol_semantics,
            cancellation,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{BindingQueryContext, BindingQueryError, bind_expression_candidates};
    use bray_bound_tree::{
        AnyBoundNodeId, BoundBlock, BoundBlockItem, BoundCallExpression, BoundCallableBody,
        BoundExpression, BoundExpressionId, BoundNameExpression, BoundNodeOrigin,
        BoundReferenceTarget, BoundTreeBuilder, BoundUnit, BoundUnitRoot, BoundWalkControl,
        BoundWalkEvent, DeclaredValueTypeTerm, SelectionKind, testing::push_expression,
        walk_bound_unit_view,
    };
    use bray_checker::{
        CallableCandidateTemplate, CallableCandidateTemplates, ExpressionCandidateSet,
    };
    use bray_diagnostics::DiagnosticKind;
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceValidationPolicy,
        test_support::encoded_semantic_test_interface,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{
        AnySymbolId, MemberLookupResult, ModulePathKey, PackageIdentity, TypeExpressionTemplate,
        UnevaluatedDefaultTemplate,
    };

    use super::CompilationBindingContext;
    use crate::{CancellationToken, Compilation, CompilationRequest, DependencyInterfaceInput};

    #[test]
    fn unrelated_paths_do_not_demand_dependency_interfaces() {
        let fixture = encoded_semantic_test_interface();
        let compilation = compilation([crate::test_support::encoded_semantic_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let binding_context = compilation
            .binding_context(&cancellation)
            .unwrap_or_else(|error| panic!("binder context must be available: {error:?}"));

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());

        assert!(matches!(
            CompilationBindingContext::imported_path_root(&binding_context, &["local", "value"]),
            Ok(None)
        ));

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());

        assert!(
            compilation
                .state
                .loaded_dependency_interfaces
                .iter()
                .all(|interface| interface.get().is_none())
        );
    }

    #[test]
    fn matching_paths_demand_only_the_imported_identity_skeleton() {
        let fixture = encoded_semantic_test_interface();
        let components = fixture.package.as_str().split('.').collect::<Vec<_>>();
        let compilation = compilation([crate::test_support::encoded_semantic_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let binding_context = compilation
            .binding_context(&cancellation)
            .unwrap_or_else(|error| panic!("binder context must be available: {error:?}"));

        let root = CompilationBindingContext::imported_path_root(&binding_context, &components)
            .unwrap_or_else(|error| panic!("imported path root must be available: {error:?}"));

        assert!(root.is_some());
        assert!(compilation.state.imported_symbol_skeleton.get().is_some());

        assert!(
            compilation
                .state
                .imported_semantic_graphs
                .iter()
                .all(|graph| graph.get().is_none())
        );

        assert_eq!(
            root.map(|root| root.consumed_components()),
            Some(components.len())
        );
    }

    #[test]
    fn origin_neutral_member_lookup_resolves_imported_type_members() {
        let fixture = encoded_semantic_test_interface();
        let compilation = compilation([crate::test_support::encoded_semantic_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let binding_context = compilation
            .binding_context(&cancellation)
            .unwrap_or_else(|error| panic!("binder context must be available: {error:?}"));

        let symbols = binding_context
            .imported_symbols()
            .unwrap_or_else(|error| panic!("imported symbols must load: {error:?}"))
            .unwrap_or_else(|| panic!("test dependency must publish imported symbols"));

        let package = symbols
            .package_by_identity(&fixture.package)
            .unwrap_or_else(|| panic!("test dependency package must be present"));

        let path = ModulePathKey::try_new(["templates"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let module = symbols
            .module_by_path(package.id(), &path)
            .unwrap_or_else(|| panic!("test dependency module must be present"));

        let MemberLookupResult::Found(structure) = symbols.lookup(module.id().into(), "Record")
        else {
            panic!("test imported structure must resolve");
        };

        assert!(matches!(
            BindingQueryContext::lookup_member(&binding_context, structure, "direct"),
            Ok(MemberLookupResult::Found(AnySymbolId::TypeCallableMember(
                _
            )))
        ));
    }

    #[test]
    fn cancelled_imported_path_lookup_does_not_publish_a_skeleton() {
        let fixture = encoded_semantic_test_interface();
        let components = fixture.package.as_str().split('.').collect::<Vec<_>>();
        let compilation = compilation([crate::test_support::encoded_semantic_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let binding_context = compilation
            .binding_context(&cancellation)
            .unwrap_or_else(|error| panic!("binder context must be available: {error:?}"));

        cancellation.cancel();

        assert!(matches!(
            CompilationBindingContext::imported_path_root(&binding_context, &components),
            Err(BindingQueryError::Cancelled)
        ));

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());
    }

    #[test]
    fn selected_invalid_dependencies_leave_no_imported_path_root() {
        let dependency = DependencyInterfaceInput::new(
            package("invalid.package"),
            bray_package_interface::InterfaceProductIdentity::try_new("main")
                .unwrap_or_else(|| panic!("test product identity must be valid")),
            "invalid.brayi",
            Arc::<[u8]>::from(vec![0; 112]),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let compilation = compilation([dependency]);
        let cancellation = CancellationToken::new();

        let binding_context = compilation
            .binding_context(&cancellation)
            .unwrap_or_else(|error| panic!("binder context must be available: {error:?}"));

        assert!(matches!(
            CompilationBindingContext::imported_path_root(
                &binding_context,
                &["invalid", "package"]
            ),
            Ok(None)
        ));
    }

    #[test]
    fn production_candidate_enumeration_is_exact_deterministic_and_template_preserving() {
        let compilation = crate::test_support::compilation(concat!(
            "module app;\n",
            "\n",
            "func run()\n",
            "{\n",
            "    let callback: func(pos value: i32) -> i32 = alternate;\n",
            "    let inferred = alternate;\n",
            "    fixed([0; 4]);\n",
            "    choose(1);\n",
            "    callback(2);\n",
            "    inferred(3);\n",
            "    missing();\n",
            "    1 + 2;\n",
            "    [1, 2][0];\n",
            "    1 as i64;\n",
            "    Record { value = 1 };\n",
            "    let record = Record { value = 2 };\n",
            "    record.value;\n",
            "}\n",
            "\n",
            "extern func fixed(value: [i32; 4] = [0; 4]) -> [i32; 4];\n",
            "extern func alternate(value: i32) -> i32;\n",
            "overload choose = {fixed, alternate}\n",
            "struct Record\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
        ));

        assert!(
            compilation.syntax_tree_result().diagnostics().is_empty(),
            "test source must parse without recovery: {:?}",
            compilation.syntax_tree_result().diagnostics()
        );

        let key = crate::test_support::source_callable_body_key(&compilation);

        let bound = compilation
            .bound_unit(key.clone())
            .unwrap_or_else(|error| panic!("test callable must bind: {error:?}"));

        let cancellation = CancellationToken::new();

        let binding_context = compilation
            .binding_context_for(&key, &cancellation)
            .unwrap_or_else(|error| panic!("binder context must be available: {error:?}"));

        let semantic_values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let expressions = candidate_expressions(bound.value());

        let results = expressions
            .iter()
            .copied()
            .map(|expression| candidates(&binding_context, bound.value(), expression))
            .collect::<Vec<_>>();

        let direct = results.iter().find_map(|result| match result.value() {
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Present {
                candidates,
                ..
            }) if candidates.len() == 1
                && matches!(candidates[0], CallableCandidateTemplate::Declaration(_)) =>
            {
                Some(candidates)
            }
            _ => None,
        });

        let Some(direct) = direct else {
            panic!("direct call must produce one declaration candidate: {results:?}");
        };

        let [CallableCandidateTemplate::Declaration(direct)] = direct.as_ref() else {
            panic!("direct call must produce one declared callable");
        };

        assert!(matches!(
            direct.signature().callable_type(),
            TypeExpressionTemplate::Callable(_)
        ));

        let [parameter] = direct.signature().parameters() else {
            panic!("direct callable must retain one parameter identity");
        };

        let parameter_type = direct
            .signature()
            .parameter_type_template(*parameter, 0, semantic_values)
            .unwrap_or_else(|error| panic!("parameter template must be available: {error:?}"));

        assert!(matches!(
            parameter_type,
            TypeExpressionTemplate::Array { .. }
        ));

        assert!(matches!(
            direct.defaults()[0].value(),
            UnevaluatedDefaultTemplate::Present(_)
        ));

        assert!(direct.defaults()[0].provider().is_some());

        let first_overload = results.iter().find(|result| {
            matches!(
                result.value(),
                ExpressionCandidateSet::Callable(CallableCandidateTemplates::Present {
                    candidates,
                    ..
                }) if candidates.len() == 2
            )
        });

        let Some(first_overload) = first_overload else {
            panic!("overload call must expand its exact arm templates: {results:?}");
        };

        let ExpressionCandidateSet::Callable(CallableCandidateTemplates::Present {
            candidates: overload,
            ..
        }) = first_overload.value()
        else {
            unreachable!("overload result was classified above");
        };

        assert_eq!(overload.len(), 2);

        assert!(
            overload
                .iter()
                .all(|candidate| matches!(candidate, CallableCandidateTemplate::Declaration(_)))
        );

        let values = results
            .iter()
            .filter_map(|result| match result.value() {
                ExpressionCandidateSet::Callable(CallableCandidateTemplates::Present {
                    candidates,
                    ..
                }) if matches!(candidates.as_ref(), [CallableCandidateTemplate::Value(_)]) => {
                    Some(candidates)
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(values.len(), 2);

        assert!(values.iter().all(|candidates| matches!(
            candidates.as_ref(),
            [CallableCandidateTemplate::Value(value)]
                if matches!(
                    value.value(),
                    DeclaredValueTypeTerm::Value(BoundReferenceTarget::Local(_))
                )
        )));

        assert!(results.iter().any(|result| {
            let ExpressionCandidateSet::Operation(operation) = result.value() else {
                return false;
            };

            let Some(BoundExpression::Call(call)) =
                bound.value().view().expression(operation.expression())
            else {
                return false;
            };

            operation.kind() == SelectionKind::Construction
                && matches!(
                    bound.value().view().expression(call.callee()),
                    Some(BoundExpression::UnqualifiedVariant(_))
                )
        }));

        for kind in [
            SelectionKind::Member,
            SelectionKind::Operator,
            SelectionKind::Index,
            SelectionKind::Construction,
            SelectionKind::Conversion,
        ] {
            assert!(
                results.iter().any(|result| matches!(
                    result.value(),
                    ExpressionCandidateSet::Operation(operation)
                        if operation.kind() == kind
                )),
                "candidate enumeration must cover {kind:?}: {results:?}"
            );
        }

        let repeated = candidates(
            &binding_context,
            bound.value(),
            first_overload.value().expression(),
        );

        assert_eq!(&repeated, first_overload);
    }

    #[test]
    fn overloaded_call_reports_generic_argument_diagnostics_once() {
        let compilation = compilation_with_source(
            concat!(
                "module app;\n",
                "func run()\n",
                "{\n",
                "    choose<Missing>(1);\n",
                "}\n",
                "extern func first<T>(value: i32) -> i32;\n",
                "extern func second<T>(value: i32) -> i32;\n",
                "overload choose = {first, second}\n",
            ),
            [],
        );

        assert!(
            compilation.syntax_tree_result().diagnostics().is_empty(),
            "test source must parse without recovery: {:?}",
            compilation.syntax_tree_result().diagnostics()
        );

        let key = crate::test_support::source_callable_body_key(&compilation);

        let bound = compilation
            .bound_unit(key.clone())
            .unwrap_or_else(|error| panic!("test callable must bind: {error:?}"));

        let cancellation = CancellationToken::new();

        let binding_context = compilation
            .binding_context_for(&key, &cancellation)
            .unwrap_or_else(|error| panic!("binder context must be available: {error:?}"));

        let expressions = candidate_expressions(bound.value());

        let [expression] = expressions.as_slice() else {
            panic!("test source must contain one candidate expression");
        };

        let owner = binding_context
            .symbols
            .symbol_for_key(bound.value().key().declared_owner())
            .unwrap_or_else(|| panic!("candidate unit owner must resolve"));

        let scope = super::super::symbol::type_scope(&binding_context, owner)
            .unwrap_or_else(|error| panic!("candidate type scope must bind: {error:?}"));

        let result =
            bind_expression_candidates(&binding_context, bound.value(), *expression, &scope)
                .unwrap_or_else(|error| panic!("candidate enumeration must complete: {error:?}"));

        assert!(matches!(
            result.value(),
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Present {
                candidates,
                ..
            }) if candidates.len() == 2
        ));

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingUnresolvedName]
        );
    }

    #[test]
    fn imported_callable_candidates_retain_source_independent_templates() {
        let fixture = encoded_semantic_test_interface();

        let compilation = compilation_with_source(
            concat!(
                "module current.package;\n",
                "\n",
                "func main()\n",
                "{\n",
                "}\n",
            ),
            [crate::test_support::encoded_semantic_dependency(&fixture)],
        );

        let key = crate::test_support::source_callable_body_key(&compilation);

        let bound = compilation
            .bound_unit(key.clone())
            .unwrap_or_else(|error| panic!("test callable must bind: {error:?}"));

        let cancellation = CancellationToken::new();

        let binding_context = compilation
            .binding_context_for(&key, &cancellation)
            .unwrap_or_else(|error| panic!("binder context must be available: {error:?}"));

        let imported = imported_function(&binding_context, &fixture.package);

        let (unit, expression) = imported_call_unit(bound.value(), imported);

        let result = candidates(&binding_context, &unit, expression);

        let ExpressionCandidateSet::Callable(CallableCandidateTemplates::Present {
            candidates,
            ..
        }) = result.value()
        else {
            panic!("imported callable must publish one candidate: {result:?}");
        };

        let [CallableCandidateTemplate::Declaration(candidate)] = candidates.as_ref() else {
            panic!("imported callable must publish one declaration candidate");
        };

        assert_eq!(candidate.signature().parameters().len(), 1);
        assert_eq!(candidate.generic().parameters().len(), 2);
        assert_eq!(candidate.generic().constraints().len(), 1);
        assert_eq!(candidate.defaults().len(), 1);

        assert_eq!(
            candidate.defaults()[0].value(),
            UnevaluatedDefaultTemplate::Resolved
        );

        assert!(candidate.defaults()[0].provider().is_some());

        let [parameter] = candidate.signature().parameters() else {
            unreachable!("parameter count was checked above");
        };

        let parameter_type = candidate
            .signature()
            .parameter_type_template(*parameter, 0, binding_context.semantic_values)
            .unwrap_or_else(|error| panic!("imported parameter type must resolve: {error:?}"));

        let TypeExpressionTemplate::Resolved(parameter_type) = parameter_type else {
            panic!("imported parameter type must use canonical semantic identity");
        };

        let parameter_type = binding_context
            .semantic_values
            .type_data(parameter_type)
            .unwrap_or_else(|error| panic!("imported parameter type must be interned: {error:?}"));

        let bray_symbols::TypeData::Array { length, .. } = parameter_type.as_ref() else {
            panic!("imported parameter must retain its array type");
        };

        let length = binding_context
            .semantic_values
            .constant_term_data(*length)
            .unwrap_or_else(|error| panic!("imported array length must be interned: {error:?}"));

        assert!(matches!(
            length.as_ref(),
            bray_symbols::ConstantTermData::Parameter(_)
        ));

        assert!(
            compilation
                .state
                .imported_semantic_graphs
                .iter()
                .any(|graph| graph.get().is_some())
        );
    }

    fn candidates(
        binding_context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        expression: BoundExpressionId,
    ) -> bray_diagnostics::DiagnosticResult<ExpressionCandidateSet> {
        let owner = binding_context
            .symbols
            .symbol_for_key(unit.key().declared_owner())
            .unwrap_or_else(|| panic!("candidate unit owner must resolve"));

        let scope = match super::super::symbol::type_scope(binding_context, owner) {
            Ok(scope) => scope,
            Err(error) => panic!("candidate type scope must bind: {error:?}"),
        };

        let result = bind_expression_candidates(binding_context, unit, expression, &scope)
            .unwrap_or_else(|error| panic!("candidate enumeration must complete: {error:?}"));

        assert!(result.diagnostics().is_empty());

        result
    }

    fn candidate_expressions(unit: &BoundUnit) -> Vec<BoundExpressionId> {
        let mut expressions = Vec::new();

        let outcome = walk_bound_unit_view(unit.view(), unit.root(), |event| {
            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
                return BoundWalkControl::Continue;
            };

            let Some(bound) = unit.view().expression(expression) else {
                return BoundWalkControl::Stop;
            };

            if matches!(
                bound,
                BoundExpression::Call(_)
                    | BoundExpression::ErrorCall(_)
                    | BoundExpression::MemberAccess(_)
                    | BoundExpression::TraitQualifiedMember(_)
                    | BoundExpression::Unary(_)
                    | BoundExpression::Binary(_)
                    | BoundExpression::Assignment(_)
                    | BoundExpression::Conversion(_)
                    | BoundExpression::StructConstruction(_)
                    | BoundExpression::LeadingDotVariant(_)
                    | BoundExpression::UnqualifiedVariant(_)
                    | BoundExpression::Structured(_)
            ) {
                expressions.push(expression);
            }

            BoundWalkControl::Continue
        });

        assert_eq!(outcome, bray_bound_tree::BoundWalkOutcome::Completed);

        expressions
    }

    fn imported_function(
        binding_context: &CompilationBindingContext<'_>,
        package_identity: &PackageIdentity,
    ) -> AnySymbolId {
        let symbols = binding_context
            .imported_symbols()
            .unwrap_or_else(|error| panic!("imported symbols must load: {error:?}"))
            .unwrap_or_else(|| panic!("test dependency must publish imported symbols"));

        let package = symbols
            .package_by_identity(package_identity)
            .unwrap_or_else(|| panic!("test dependency package must be present"));

        let path = ModulePathKey::try_new(["templates"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let module = symbols
            .module_by_path(package.id(), &path)
            .unwrap_or_else(|| panic!("test dependency module must be present"));

        match symbols.lookup(module.id().into(), "run") {
            MemberLookupResult::Found(symbol) => symbol,
            result => panic!("test imported callable must resolve: {result:?}"),
        }
    }

    fn imported_call_unit(
        template: &BoundUnit,
        imported: AnySymbolId,
    ) -> (BoundUnit, BoundExpressionId) {
        let origin = BoundNodeOrigin::source(template.key().source());
        let mut tree = BoundTreeBuilder::new(template.unit());

        let callee = push_expression(
            &mut tree,
            BoundExpression::Name(BoundNameExpression::new(
                origin,
                BoundReferenceTarget::Surface(imported),
                None,
                false,
            )),
        );

        let call = push_expression(
            &mut tree,
            BoundExpression::Call(BoundCallExpression::pending(origin, callee, [], [])),
        );

        let block = tree
            .push_block(BoundBlock::new(
                origin,
                [BoundBlockItem::Expression(call)],
                false,
            ))
            .unwrap_or_else(|error| panic!("test block must be valid: {error:?}"));

        let body = tree
            .push_callable_body(BoundCallableBody::block(origin, block))
            .unwrap_or_else(|error| panic!("test callable body must be valid: {error:?}"));

        let unit = BoundUnit::try_new(
            template.key().clone(),
            tree.finish(),
            template.local_symbols().clone(),
            template.nested_units().iter().cloned(),
            BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Synchronous,
                body,
            },
        )
        .unwrap_or_else(|error| panic!("test bound unit must be valid: {error:?}"));

        (unit, call)
    }

    fn compilation(
        dependencies: impl IntoIterator<Item = DependencyInterfaceInput>,
    ) -> Compilation {
        compilation_with_source("module current.package;", dependencies)
    }

    fn compilation_with_source(
        source: &str,
        dependencies: impl IntoIterator<Item = DependencyInterfaceInput>,
    ) -> Compilation {
        let request = CompilationRequest::new(
            package("current.package"),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(1),
                "main.bray",
                SourceVersion::new(1),
                source,
            )],
        )
        .with_dependency_interfaces(dependencies);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn package(value: &str) -> PackageIdentity {
        PackageIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }
}
