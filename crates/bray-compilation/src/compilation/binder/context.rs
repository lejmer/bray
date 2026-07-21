use std::sync::Arc;

use bray_binder::{
    BinderFactContext, BinderFactError, BinderFactResult, ImportedPathRoot, TargetFactProvider,
    TargetFactResult,
};
use bray_declarations::DeclarationTable;
use bray_symbols::{
    AnySymbolId, ConstantSymbolId, ImportedSymbolFactAddress, ImportedSymbolSkeleton,
    SemanticValueStore, SymbolGraph,
};
use bray_syntax::SyntaxTree;

use super::super::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

pub(in crate::compilation) struct CompilationBinderFacts<'compilation> {
    pub(super) compilation: &'compilation Compilation,
    pub(super) symbols: &'compilation SymbolGraph,
    pub(super) semantic_values: &'compilation SemanticValueStore,
    pub(super) cancellation: &'compilation CancellationToken,
}

impl<'compilation> CompilationBinderFacts<'compilation> {
    pub(super) const fn new(
        compilation: &'compilation Compilation,
        symbols: &'compilation SymbolGraph,
        semantic_values: &'compilation SemanticValueStore,
        cancellation: &'compilation CancellationToken,
    ) -> Self {
        Self {
            compilation,
            symbols,
            semantic_values,
            cancellation,
        }
    }

    pub(in crate::compilation) const fn compilation(&self) -> &'compilation Compilation {
        self.compilation
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

    pub(super) fn imported_symbols(&self) -> BinderFactResult<Option<&ImportedSymbolSkeleton>> {
        let result = self
            .compilation
            .imported_symbol_skeleton_result_with_cancellation(self.cancellation)
            .map_err(|error| match error {
                FactQueryError::Cancelled => BinderFactError::Cancelled,
                _ => BinderFactError::DependencyUnavailable,
            })?;

        Ok(result.value().as_deref())
    }

    pub(super) fn imported_fact_address(
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
    type TargetFacts = CompilationTargetFacts;
    type SymbolFacts = Self;
    type Cancellation = CancellationToken;

    fn syntax(&self) -> &SyntaxTree {
        self.compilation.syntax_tree()
    }

    fn declarations(&self) -> &DeclarationTable {
        self.compilation.declaration_table()
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

    fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BinderFactResult<Option<ImportedPathRoot<'_>>> {
        CompilationBinderFacts::imported_path_root(self, components)
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.semantic_values
    }

    fn target_facts(&self) -> &Self::TargetFacts {
        &self.compilation.state.target_facts
    }

    fn symbol_facts(&self) -> &Self::SymbolFacts {
        self
    }

    fn cancellation(&self) -> &Self::Cancellation {
        self.cancellation
    }
}

#[derive(Debug)]
pub(in crate::compilation) struct CompilationTargetFacts;

impl TargetFactProvider for CompilationTargetFacts {
    fn target_fact(&self, _fact: ConstantSymbolId) -> BinderFactResult<Arc<TargetFactResult>> {
        Err(BinderFactError::DependencyUnavailable)
    }
}

impl Compilation {
    pub(in crate::compilation) fn binder_facts_for<'compilation>(
        &'compilation self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationBinderFacts<'compilation>, FactQueryError> {
        let facts = self.binder_facts(cancellation)?;

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

        Ok(CompilationBinderFacts::new(
            self,
            symbols,
            semantic_values,
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
        CallableCandidateTemplate, CallableCandidateTemplates, CandidateAbsence,
        ExpressionCandidateSet,
    };
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceValidationPolicy,
        test_support::encoded_template_test_interface,
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
        let fixture = encoded_template_test_interface();
        let compilation = compilation([valid_dependency(&fixture)]);
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
        let fixture = encoded_template_test_interface();
        let components = fixture.package.as_str().split('.').collect::<Vec<_>>();
        let compilation = compilation([valid_dependency(&fixture)]);
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
        let fixture = encoded_template_test_interface();
        let components = fixture.package.as_str().split('.').collect::<Vec<_>>();
        let compilation = compilation([valid_dependency(&fixture)]);
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
            "}\n",
            "\n",
            "extern func fixed(value: [i32; 4] = [0; 4]) -> [i32; 4];\n",
            "extern func alternate(value: i32) -> i32;\n",
            "overload choose = {fixed, alternate}\n",
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

        assert!(results.iter().any(|result| matches!(
            result.value(),
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Absent {
                reason: CandidateAbsence::UnresolvedReference,
                ..
            })
        )));

        assert!(results.iter().any(|result| matches!(
            result.value(),
            ExpressionCandidateSet::Unsupported {
                kind: SelectionKind::Operator,
                ..
            }
        )));

        let repeated = candidates(&facts, bound.value(), first_overload.value().expression());

        assert_eq!(&repeated, first_overload);
    }

    #[test]
    fn imported_callable_candidates_remain_explicit_until_signature_facts_are_portable() {
        let fixture = encoded_template_test_interface();
        let compilation = compilation_with_source(
            concat!(
                "module current.package;\n",
                "\n",
                "func main()\n",
                "{\n",
                "}\n",
            ),
            [valid_dependency(&fixture)],
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

        assert!(matches!(
            result.value(),
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Absent {
                reason: CandidateAbsence::UnavailableDeclarationFacts,
                ..
            })
        ));

        assert!(
            compilation
                .state
                .imported_semantic_graphs
                .iter()
                .all(|graph| graph.get().is_none())
        );
    }

    fn candidates(
        facts: &CompilationBinderFacts<'_>,
        unit: &BoundUnit,
        expression: BoundExpressionId,
    ) -> bray_diagnostics::DiagnosticResult<ExpressionCandidateSet> {
        let result = bind_expression_candidates(facts, unit, expression)
            .unwrap_or_else(|error| panic!("candidate enumeration must complete: {error:?}"));

        assert!(result.diagnostics().is_empty());

        result
    }

    fn candidate_expressions(unit: &BoundUnit) -> Vec<BoundExpressionId> {
        let mut expressions = Vec::new();

        let outcome = walk_bound_unit_view(unit.view(), unit_root(unit), |event| {
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
                    | BoundExpression::Binary(_)
            ) {
                expressions.push(expression);
            }

            BoundWalkControl::Continue
        });

        assert_eq!(outcome, bray_bound_tree::BoundWalkOutcome::Completed);

        expressions
    }

    const fn unit_root(unit: &BoundUnit) -> AnyBoundNodeId {
        match unit.root() {
            BoundUnitRoot::CallableBody(body) => AnyBoundNodeId::CallableBody(body),
            BoundUnitRoot::AnonymousCallable { body, .. } => AnyBoundNodeId::CallableBody(body),
            BoundUnitRoot::Expression(expression) => AnyBoundNodeId::Expression(expression),
            BoundUnitRoot::ExpressionSequence(block) => AnyBoundNodeId::Block(block),
        }
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
            BoundUnitRoot::CallableBody(body),
        )
        .unwrap_or_else(|error| panic!("test bound unit must be valid: {error:?}"));

        (unit, call)
    }

    fn valid_dependency(
        fixture: &bray_package_interface::test_support::EncodedTemplateTestInterface,
    ) -> DependencyInterfaceInput {
        DependencyInterfaceInput::new(
            fixture.package.clone(),
            fixture.product.clone(),
            "dependency.brayi",
            Arc::<[u8]>::from(fixture.bytes.clone()),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
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
