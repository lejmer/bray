use bray_binder::{
    BinderFactContext, BinderFactError, BinderFactResult, ImportedPathRoot, NameAccess,
    SymbolFactProvider, bind_surface_path_with_re_exports,
};
use bray_declarations::DeclarationTable;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
    ImportedSymbolFactAddress, ImportedSymbolSkeleton, MemberLookupResult, ModuleSurfaceFact,
    ModuleSymbolId, SemanticValueStore, SymbolFactRequest, SymbolGraph,
};
use bray_syntax::{PathSyntax, SyntaxTree};

use super::super::Compilation;
use super::CompilationSymbolFacts;
use crate::fact::{CancellationToken, FactQueryError};

pub(in crate::compilation) struct CompilationBinderFacts<'compilation> {
    pub(super) compilation: &'compilation Compilation,
    declarations: &'compilation DeclarationTable,
    pub(super) symbols: &'compilation SymbolGraph,
    pub(super) semantic_values: &'compilation SemanticValueStore,
    pub(super) symbol_facts: &'compilation CompilationSymbolFacts,
    pub(super) cancellation: &'compilation CancellationToken,
}

impl<'compilation> CompilationBinderFacts<'compilation> {
    pub(super) const fn new(
        compilation: &'compilation Compilation,
        declarations: &'compilation DeclarationTable,
        symbols: &'compilation SymbolGraph,
        semantic_values: &'compilation SemanticValueStore,
        symbol_facts: &'compilation CompilationSymbolFacts,
        cancellation: &'compilation CancellationToken,
    ) -> Self {
        Self {
            compilation,
            declarations,
            symbols,
            semantic_values,
            symbol_facts,
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
    ) -> BinderFactResult<DiagnosticResult<MemberLookupResult<AnySymbolId>>> {
        bind_surface_path_with_re_exports(
            self,
            module,
            path,
            access,
            &mut |module, name, access| self.module_re_export_lookup(module, name, access),
        )
    }

    pub(in crate::compilation) const fn declarations(&self) -> &'compilation DeclarationTable {
        self.declarations
    }

    pub(super) fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>> {
        let mut package_prefix = String::new();
        let mut selected = None;

        for component in components {
            if !package_prefix.is_empty() {
                package_prefix.push('.');
            }

            package_prefix.push_str(component);

            let dependency = self
                .compilation
                .state
                .dependency_interfaces
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

        let identity = self.compilation.state.dependency_interfaces[dependency].package();

        let symbols = self
            .imported_symbols()?
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let package = symbols
            .package_by_identity(identity)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        ImportedPathRoot::for_path(symbols, package.id(), components)
            .map(Some)
            .ok_or(BinderFactError::DependencyUnavailable)
    }

    pub(in crate::compilation) fn imported_symbols(
        &self,
    ) -> BinderFactResult<Option<&ImportedSymbolSkeleton>> {
        let result = self
            .compilation
            .imported_symbol_skeleton_result_with_cancellation(self.cancellation)
            .map_err(|error| match error {
                FactQueryError::Cancelled => BinderFactError::Cancelled,
                _ => BinderFactError::DependencyUnavailable,
            })?;

        Ok(result.value().as_deref())
    }

    pub(in crate::compilation) fn imported_fact_address(
        &self,
        symbol: AnySymbolId,
    ) -> BinderFactResult<Option<ImportedSymbolFactAddress>> {
        if self.symbols.symbol_key(symbol).is_some() {
            return Ok(None);
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.imported_fact_address(symbol)))
    }
}

impl BinderFactContext for CompilationBinderFacts<'_> {
    type SymbolFacts = Self;
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
    ) -> BinderFactResult<Option<&bray_symbols::SymbolKey>> {
        if let Some(key) = self.symbols.symbol_key(symbol) {
            return Ok(Some(key));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.symbol_key(symbol)))
    }

    fn symbol_is_recovered(&self, symbol: AnySymbolId) -> BinderFactResult<Option<bool>> {
        if let Some(is_recovered) = self.symbols.symbol_is_recovered(symbol) {
            return Ok(Some(is_recovered));
        }

        Ok(self
            .imported_symbols()?
            .and_then(|symbols| symbols.symbol_is_recovered(symbol)))
    }

    fn callable_parameter_default_provider(
        &self,
        parameter: CallableParameterSymbolId,
    ) -> BinderFactResult<Option<CallableParameterDefaultProviderSymbolId>> {
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

    fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>> {
        CompilationBinderFacts::imported_path_root(self, components)
    }

    fn imported_symbols(&self) -> BinderFactResult<Option<&ImportedSymbolSkeleton>> {
        CompilationBinderFacts::imported_symbols(self)
    }

    fn module_re_export_lookup(
        &self,
        module: ModuleSymbolId,
        name: &str,
        access: bray_binder::NameAccess,
    ) -> BinderFactResult<MemberLookupResult<AnySymbolId>> {
        let surface = self.symbol_fact(SymbolFactRequest::<ModuleSurfaceFact>::new(module))?;

        Ok(match access {
            bray_binder::NameAccess::Public => surface.value().lookup_public(name),
            bray_binder::NameAccess::Internal => surface.value().lookup(name),
        })
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn selected_target(&self) -> &bray_target::TargetProfile {
        self.compilation.selected_target().target().profile()
    }

    fn symbol_facts(&self) -> &Self::SymbolFacts {
        self
    }

    fn cancellation(&self) -> &Self::Cancellation {
        self.cancellation
    }
}

impl Compilation {
    pub(in crate::compilation) fn binder_facts_for<'compilation>(
        &'compilation self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBinderFacts<'compilation>, FactQueryError> {
        let facts = if key.kind() == bray_bound_tree::BoundUnitKind::TargetGate {
            self.discovery_binder_facts(cancellation)?
        } else {
            self.binder_facts(cancellation)?
        };

        if !facts.symbols.contains_symbol_key(key.declared_owner()) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        Ok(facts)
    }

    pub(in crate::compilation) fn binder_facts<'compilation>(
        &'compilation self,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBinderFacts<'compilation>, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let semantic_values = self.semantic_value_store()?;
        let declarations = self.product_source_graph()?.declarations();

        Ok(CompilationBinderFacts::new(
            self,
            declarations,
            symbols,
            semantic_values,
            &self.state.symbol_facts,
            cancellation,
        ))
    }

    pub(in crate::compilation) fn discovery_binder_facts<'compilation>(
        &'compilation self,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBinderFacts<'compilation>, FactQueryError> {
        let symbols = self.discovery_symbol_graph()?;
        let semantic_values = self.semantic_value_store()?;

        Ok(CompilationBinderFacts::new(
            self,
            self.declaration_table(),
            symbols,
            semantic_values,
            &self.state.discovery_symbol_facts,
            cancellation,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{BinderFactError, bind_expression_candidates};
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

    use super::CompilationBinderFacts;
    use crate::{CancellationToken, Compilation, CompilationRequest, DependencyInterfaceInput};

    #[test]
    fn unrelated_paths_do_not_demand_dependency_interfaces() {
        let fixture = encoded_semantic_test_interface();
        let compilation = compilation([crate::test_support::encoded_semantic_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());

        assert!(matches!(
            CompilationBinderFacts::imported_path_root(&facts, &["local", "value"]),
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

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        let root = CompilationBinderFacts::imported_path_root(&facts, &components)
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
    fn cancelled_imported_path_lookup_does_not_publish_a_skeleton() {
        let fixture = encoded_semantic_test_interface();
        let components = fixture.package.as_str().split('.').collect::<Vec<_>>();
        let compilation = compilation([crate::test_support::encoded_semantic_dependency(&fixture)]);
        let cancellation = CancellationToken::new();

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        cancellation.cancel();

        assert!(matches!(
            CompilationBinderFacts::imported_path_root(&facts, &components),
            Err(BinderFactError::Cancelled)
        ));

        assert!(compilation.state.imported_symbol_skeleton.get().is_none());
    }

    #[test]
    fn selected_invalid_dependencies_are_unavailable_instead_of_absent() {
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

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        assert!(matches!(
            CompilationBinderFacts::imported_path_root(&facts, &["invalid", "package"]),
            Err(BinderFactError::DependencyUnavailable)
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

        let facts = compilation
            .binder_facts_for(&key, &cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        let semantic_values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let expressions = candidate_expressions(bound.value());

        let results = expressions
            .iter()
            .copied()
            .map(|expression| candidates(&facts, bound.value(), expression))
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

        let repeated = candidates(&facts, bound.value(), first_overload.value().expression());

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

        let facts = compilation
            .binder_facts_for(&key, &cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        let expressions = candidate_expressions(bound.value());

        let [expression] = expressions.as_slice() else {
            panic!("test source must contain one candidate expression");
        };

        let owner = facts
            .symbols
            .symbol_for_key(bound.value().key().declared_owner())
            .unwrap_or_else(|| panic!("candidate unit owner must resolve"));

        let scope = super::super::symbol::type_scope(&facts, owner)
            .unwrap_or_else(|error| panic!("candidate type scope must bind: {error:?}"));

        let result = bind_expression_candidates(&facts, bound.value(), *expression, &scope)
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

        let facts = compilation
            .binder_facts_for(&key, &cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        let imported = imported_function(&facts, &fixture.package);

        let (unit, expression) = imported_call_unit(bound.value(), imported);

        let result = candidates(&facts, &unit, expression);

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
            .parameter_type_template(*parameter, 0, facts.semantic_values)
            .unwrap_or_else(|error| panic!("imported parameter type must resolve: {error:?}"));

        let TypeExpressionTemplate::Resolved(parameter_type) = parameter_type else {
            panic!("imported parameter type must use canonical semantic identity");
        };

        let parameter_type = facts
            .semantic_values
            .type_data(parameter_type)
            .unwrap_or_else(|error| panic!("imported parameter type must be interned: {error:?}"));

        let bray_symbols::TypeData::Array { length, .. } = parameter_type.as_ref() else {
            panic!("imported parameter must retain its array type");
        };

        let length = facts
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
        facts: &CompilationBinderFacts<'_>,
        unit: &BoundUnit,
        expression: BoundExpressionId,
    ) -> bray_diagnostics::DiagnosticResult<ExpressionCandidateSet> {
        let owner = facts
            .symbols
            .symbol_for_key(unit.key().declared_owner())
            .unwrap_or_else(|| panic!("candidate unit owner must resolve"));

        let scope = match super::super::symbol::type_scope(facts, owner) {
            Ok(scope) => scope,
            Err(error) => panic!("candidate type scope must bind: {error:?}"),
        };

        let result = bind_expression_candidates(facts, unit, expression, &scope)
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
        facts: &CompilationBinderFacts<'_>,
        package_identity: &PackageIdentity,
    ) -> AnySymbolId {
        let symbols = facts
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
