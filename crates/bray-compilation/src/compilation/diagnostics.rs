use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_bound_tree::{
    BoundUnit, BoundUnitKey, BoundUnitKind, CheckedControlFlowFacts, CheckedPatternFacts,
    DeclaredValueTypeTemplates, StoragePlan,
};
use bray_declarations::{DeclarationKind, DeclarationRecord, SyntaxAnchor};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, ConstantDefinitionState, ConstantInstanceValueFact, ModuleSurface,
    ModuleSurfaceFact, NamedTypeSymbolId, SemanticFactResult, SymbolFactRequest, SymbolGraph,
    SymbolKey, SymbolOrigin, TypeAssociatedSurface,
};
use bray_syntax::{SyntaxKind, SyntaxTree, SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};

use super::binder::has_visible_generic_parameters;
use super::constant::{constant_definition_id, empty_concrete_substitution};
use super::facts::{CheckedExpressionSemantics, Compilation};
use crate::fact::{CompilationFactKey, FactQueryError, PublishedUnitFact};

impl Compilation {
    /// Returns diagnostics produced by binding and semantic analysis of this package.
    pub fn semantic_diagnostics(&self) -> &DiagnosticBag {
        self.fact(
            CompilationFactKey::SemanticDiagnostics,
            &self.state.semantic_diagnostics,
            || match self.compute_semantic_diagnostics() {
                Ok(diagnostics) => diagnostics,
                Err(FactQueryError::Cancelled) => {
                    panic!("uncancellable semantic diagnostics were unexpectedly cancelled")
                }
                Err(FactQueryError::Cycle(cycle)) => {
                    panic!("semantic diagnostic dependencies formed a cycle: {cycle:?}")
                }
                Err(FactQueryError::InfrastructureFailure) => {
                    panic!("semantic diagnostic infrastructure failed")
                }
                Err(FactQueryError::SemanticUnitContext(error)) => {
                    panic!("semantic unit context failed: {error:?}")
                }
                Err(FactQueryError::CheckerInfrastructure(error)) => {
                    panic!("semantic checker infrastructure failed: {error:?}")
                }
            },
        )
    }

    /// Returns diagnostics for the current whole-package check request.
    pub fn check_diagnostics(&self) -> &DiagnosticBag {
        self.fact(
            CompilationFactKey::CheckDiagnostics,
            &self.state.check_diagnostics,
            || {
                let diagnostics = self.map_facts(4, |index| match index {
                    0 => self.source_diagnostics(),
                    1 => self.syntax_tree_result().diagnostics(),
                    2 => self.imported_diagnostics(),
                    3 => self.semantic_diagnostics(),
                    _ => unreachable!("scheduled diagnostic index must be in range"),
                });

                DiagnosticBag::merged_all(diagnostics)
            },
        )
    }

    fn compute_semantic_diagnostics(&self) -> Result<DiagnosticBag, FactQueryError> {
        let source_graph = self.product_source_graph()?;
        let mut pending = BTreeSet::new();

        for key in self.declared_unit_keys()? {
            pending.insert(unit_order_key(key));
        }

        let symbols = self.symbol_graph()?;
        let mut facts = Vec::new();
        let binder = self.binder_facts(&self.state.cancellation)?;

        for module in symbols
            .modules()
            .iter()
            .filter(|module| module.origin() == SymbolOrigin::Source)
        {
            let surface = binder
                .symbol_fact(SymbolFactRequest::<ModuleSurfaceFact>::new(module.id()))
                .map_err(super::binder::binder_fact_error)?;

            facts.push(SemanticDiagnosticFact::ModuleSurface(surface));
        }

        for subject in symbols
            .structures()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| NamedTypeSymbolId::from(symbol.id()))
            .chain(
                symbols
                    .unions()
                    .iter()
                    .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
                    .map(|symbol| NamedTypeSymbolId::from(symbol.id())),
            )
        {
            facts.push(SemanticDiagnosticFact::TypeSurface(
                self.type_associated_surface_result(subject)?,
            ));
        }

        while let Some((_, _, _, key)) = pending.pop_first() {
            let bound = self.bound_unit(key.clone())?;

            for nested in bound.value().nested_units() {
                // Nested unit keys are Arc-backed immutable identities shared with their owner.
                pending.insert(unit_order_key(nested.clone()));
            }

            let declared_types = self.declared_value_type_templates(key.clone())?;

            let embedded_constants = self.checked_constant_terms_for_templates_with_cancellation(
                declared_types
                    .value()
                    .evidence()
                    .iter()
                    .map(|evidence| evidence.template())
                    .chain(declared_types.value().callable_type())
                    .chain(declared_types.value().callable_result()),
                &self.state.cancellation,
            )?;

            let expression_semantics =
                self.expression_semantics_with_cancellation(key.clone(), &self.state.cancellation)?;

            let control_flow = self.control_flow(key.clone())?;
            let patterns = self.pattern_facts(key.clone())?;
            let storage = self.storage_plan(key.clone())?;

            // TODO(BRA-199): Finalized invocation and layout facts must request their exact
            // target-validity facts and retain those diagnostics in their semantic results.

            facts.push(SemanticDiagnosticFact::Bound(bound));
            facts.push(SemanticDiagnosticFact::DeclaredTypes(declared_types));
            facts.push(SemanticDiagnosticFact::EmbeddedConstants(
                embedded_constants,
            ));

            facts.push(SemanticDiagnosticFact::ExpressionSemantics(
                expression_semantics,
            ));

            facts.push(SemanticDiagnosticFact::ControlFlow(control_flow));
            facts.push(SemanticDiagnosticFact::Patterns(patterns));
            facts.push(SemanticDiagnosticFact::Storage(storage));

            if key.kind() == BoundUnitKind::ConstantTemplate {
                let symbols = self.symbol_graph()?;

                let owner = symbols
                    .symbol_for_key(key.declared_owner())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let definition =
                    constant_definition_id(owner).ok_or(FactQueryError::InfrastructureFailure)?;

                let template = self.constant_definition(definition)?;

                facts.push(SemanticDiagnosticFact::ConstantTemplate(template));

                if !has_visible_generic_parameters(symbols, owner) {
                    let substitution =
                        empty_concrete_substitution(self.semantic_value_store()?, definition)?;

                    let instance =
                        bray_symbols::ConstantInstanceKey::new(definition, substitution, None);

                    let value = self
                        .constant_instance_with_cancellation(instance, &self.state.cancellation)?;

                    facts.push(SemanticDiagnosticFact::ConstantInstance(value));
                }
            }
        }

        let fact_diagnostics =
            DiagnosticBag::merged_all(facts.iter().map(SemanticDiagnosticFact::diagnostics));

        Ok(source_graph.diagnostics().merged(&fact_diagnostics))
    }

    pub(in crate::compilation) fn declared_unit_keys(
        &self,
    ) -> Result<Vec<BoundUnitKey>, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let syntax = self.syntax_tree();
        let declarations = self.product_source_graph()?.declarations();
        let syntax_index = SemanticSyntaxIndex::new(syntax, declarations.declarations());

        let mut keys = Vec::new();

        for declaration in declarations.declarations() {
            let Some(symbol) = symbols.symbol_for_declaration(declaration.id()) else {
                continue;
            };

            // Stable symbol keys are Arc-backed and retained by each bound-unit key.
            let owner = symbols
                .symbol_key(symbol)
                .cloned()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            self.push_primary_unit_key(&mut keys, declaration, owner.clone(), &syntax_index)?;

            self.push_surface_unit_keys(
                &mut keys,
                declaration,
                symbol,
                owner,
                symbols,
                &syntax_index,
            )?;
        }

        Ok(keys)
    }

    #[cfg(test)]
    pub(in crate::compilation) fn declared_unit_keys_for_test(
        &self,
    ) -> Result<Vec<BoundUnitKey>, FactQueryError> {
        self.declared_unit_keys()
    }

    fn push_primary_unit_key(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        owner: SymbolKey,
        syntax: &SemanticSyntaxIndex,
    ) -> Result<(), FactQueryError> {
        let anchor = declaration.syntax_anchor();

        if callable_body_kind(declaration.kind()) && syntax.has_callable_body(anchor) {
            push_key(
                keys,
                BoundUnitKey::callable_body(owner, self.bound_source(anchor)?),
            )?;

            return Ok(());
        }

        let constructor = match declaration.kind() {
            DeclarationKind::Constant | DeclarationKind::TraitConstantMember => {
                BoundUnitKey::constant_template
            }
            DeclarationKind::Predicate | DeclarationKind::TraitPredicateMember => {
                BoundUnitKey::predicate_definition
            }
            _ => return Ok(()),
        };

        let Some(expression) = syntax.first_expression(anchor) else {
            return Ok(());
        };

        push_key(keys, constructor(owner, self.bound_source(expression)?))
    }

    fn push_surface_unit_keys(
        &self,
        keys: &mut Vec<BoundUnitKey>,
        declaration: &DeclarationRecord,
        symbol: AnySymbolId,
        owner: SymbolKey,
        symbols: &SymbolGraph,
        syntax: &SemanticSyntaxIndex,
    ) -> Result<(), FactQueryError> {
        for anchor in declaration.surface().constraints() {
            if syntax.has_expression(*anchor) {
                push_key(
                    keys,
                    BoundUnitKey::constraint(owner.clone(), self.bound_source(*anchor)?),
                )?;
            }
        }

        for anchor in declaration.surface().contract_clauses() {
            if syntax.has_expression(*anchor) {
                push_key(
                    keys,
                    BoundUnitKey::contract_clause(owner.clone(), self.bound_source(*anchor)?),
                )?;
            }
        }

        let Some(default) = declaration.surface().runtime_default() else {
            return Ok(());
        };

        let Some(expression) = syntax.first_expression(default) else {
            return Ok(());
        };

        let provider = symbols
            .runtime_default_provider(symbol)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // Synthesized provider keys are Arc-backed and retained by the runtime-default unit key.
        let provider = symbols
            .symbol_key(provider)
            .cloned()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        push_key(
            keys,
            BoundUnitKey::runtime_default(provider, self.bound_source(expression)?),
        )
    }
}

enum SemanticDiagnosticFact {
    Bound(Arc<DiagnosticResult<BoundUnit>>),
    DeclaredTypes(Arc<DiagnosticResult<DeclaredValueTypeTemplates>>),
    EmbeddedConstants(DiagnosticResult<bray_checker::CheckedConstantTerms>),
    ExpressionSemantics(Arc<PublishedUnitFact<CheckedExpressionSemantics>>),
    ControlFlow(Arc<DiagnosticResult<CheckedControlFlowFacts>>),
    Patterns(Arc<DiagnosticResult<CheckedPatternFacts>>),
    Storage(Arc<DiagnosticResult<StoragePlan>>),
    ConstantTemplate(Arc<DiagnosticResult<ConstantDefinitionState>>),
    ConstantInstance(Arc<SemanticFactResult<ConstantInstanceValueFact>>),
    ModuleSurface(Arc<DiagnosticResult<ModuleSurface>>),
    TypeSurface(Arc<DiagnosticResult<TypeAssociatedSurface>>),
}

impl SemanticDiagnosticFact {
    fn diagnostics(&self) -> &DiagnosticBag {
        match self {
            Self::Bound(result) => result.diagnostics(),
            Self::DeclaredTypes(result) => result.diagnostics(),
            Self::EmbeddedConstants(result) => result.diagnostics(),
            Self::ExpressionSemantics(result) => result.result().diagnostics(),
            Self::ControlFlow(result) => result.diagnostics(),
            Self::Patterns(result) => result.diagnostics(),
            Self::Storage(result) => result.diagnostics(),
            Self::ConstantTemplate(result) => result.diagnostics(),
            Self::ConstantInstance(result) => result.diagnostics(),
            Self::ModuleSurface(result) => result.diagnostics(),
            Self::TypeSurface(result) => result.diagnostics(),
        }
    }
}

struct SemanticSyntaxIndex {
    entries: HashMap<SyntaxAnchor, SemanticSyntaxEntry>,
}

impl SemanticSyntaxIndex {
    fn new<'declaration>(
        syntax: &SyntaxTree,
        declarations: impl IntoIterator<Item = &'declaration DeclarationRecord>,
    ) -> Self {
        let mut entries: HashMap<SyntaxAnchor, SemanticSyntaxEntry> = HashMap::new();

        for declaration in declarations {
            entries.entry(declaration.syntax_anchor()).or_default();

            for anchor in declaration
                .surface()
                .constraints()
                .iter()
                .chain(declaration.surface().contract_clauses())
                .copied()
                .chain(declaration.surface().runtime_default())
            {
                entries.entry(anchor).or_default();
            }
        }

        let mut active = Vec::new();

        walk_syntax_tree(syntax, |event| {
            let node = match event {
                SyntaxWalkEvent::EnterNode(node) => node,
                SyntaxWalkEvent::ExitNode(node) => {
                    let anchor = SyntaxAnchor::from_node(&node);

                    if active.last() == Some(&anchor) {
                        active.pop();
                    }

                    return SyntaxWalkControl::Continue;
                }
                SyntaxWalkEvent::Token(_) => return SyntaxWalkControl::Continue,
            };

            let anchor = SyntaxAnchor::from_node(&node);

            if entries.contains_key(&anchor) {
                active.push(anchor);
            }

            if node.kind() == SyntaxKind::CallableBodyBlockExpression
                && let Some(active_anchor) = active.last()
                && let Some(entry) = entries.get_mut(active_anchor)
            {
                entry.has_callable_body = true;
            }

            for active_anchor in &active {
                let Some(entry) = entries.get_mut(active_anchor) else {
                    continue;
                };

                if node.kind() == SyntaxKind::Expression && entry.first_expression.is_none() {
                    entry.first_expression = Some(anchor);
                }
            }

            SyntaxWalkControl::Continue
        });

        Self { entries }
    }

    fn has_callable_body(&self, anchor: SyntaxAnchor) -> bool {
        self.entries
            .get(&anchor)
            .is_some_and(|entry| entry.has_callable_body)
    }

    fn has_expression(&self, anchor: SyntaxAnchor) -> bool {
        self.first_expression(anchor).is_some()
    }

    fn first_expression(&self, anchor: SyntaxAnchor) -> Option<SyntaxAnchor> {
        self.entries.get(&anchor)?.first_expression
    }
}

#[derive(Default)]
struct SemanticSyntaxEntry {
    first_expression: Option<SyntaxAnchor>,
    has_callable_body: bool,
}

fn callable_body_kind(kind: DeclarationKind) -> bool {
    matches!(
        kind,
        DeclarationKind::Function
            | DeclarationKind::TraitCallableMember
            | DeclarationKind::TypeConstructorMember
            | DeclarationKind::FinalizerMember
            | DeclarationKind::DestructorMember
            | DeclarationKind::ScopeEnterMember
            | DeclarationKind::ScopeExitMember
            | DeclarationKind::TypeCallableMember
    )
}

fn push_key(keys: &mut Vec<BoundUnitKey>, key: Option<BoundUnitKey>) -> Result<(), FactQueryError> {
    let key = key.ok_or(FactQueryError::InfrastructureFailure)?;

    keys.push(key);

    Ok(())
}

fn unit_order_key(
    key: BoundUnitKey,
) -> (
    bray_source::SourceId,
    bray_source::TextRange,
    BoundUnitKind,
    BoundUnitKey,
) {
    let source = key.source().syntax();

    (source.source_id(), source.full_range(), key.kind(), key)
}

#[cfg(test)]
mod tests {
    use bray_binder::semantic_unit_context;
    use bray_bound_tree::{BoundUnitKind, BoundUnitRoot};
    use bray_checker::SemanticUnitContext;
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{CallableContractClauseKind, ConstantTermData};

    use crate::WorkerBudget;
    use crate::fact::FactCellTestEvent;
    use crate::test_support::{
        FactTestGate, compilation, compilation_with_sources_and_worker_budget, diagnostic_kinds,
    };

    #[test]
    fn package_semantic_diagnostics_request_all_declared_unit_categories() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "predicate valid() = true;\n",
            "struct value with(true)\n",
            "{\n",
            "}\n",
            "func check(value: i32 = 1) requires(true) ensures(true) with(true)\n",
            "{\n",
            "}\n",
        ));

        let mut kinds = match compilation.declared_unit_keys() {
            Ok(keys) => keys.into_iter().map(|key| key.kind()).collect::<Vec<_>>(),
            Err(error) => panic!("semantic unit keys must be discoverable: {error:?}"),
        };

        kinds.sort_unstable();

        assert_eq!(
            kinds,
            [
                BoundUnitKind::CallableBody,
                BoundUnitKind::RuntimeDefault,
                BoundUnitKind::ConstantTemplate,
                BoundUnitKind::PredicateDefinition,
                BoundUnitKind::Constraint,
                BoundUnitKind::ContractClause,
                BoundUnitKind::ContractClause,
                BoundUnitKind::ContractClause,
            ]
        );
    }

    #[test]
    fn check_diagnostics_lazily_request_and_cache_semantic_facts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let value = 1;\n",
            "    let value = 2;\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("callable key must be discoverable: {error:?}"),
        };

        let [key] = keys.as_slice() else {
            panic!("test package must contain one semantic unit");
        };

        assert_eq!(compilation.state.bound_units.is_published(key), Ok(false));

        assert_eq!(
            compilation.state.checked_control_flow.is_published(key),
            Ok(false)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(key),
            Ok(false)
        );

        let first = compilation.check_diagnostics();
        let second = compilation.check_diagnostics();

        assert!(std::ptr::eq(first, second));
        assert_eq!(compilation.state.bound_units.is_published(key), Ok(true));

        assert_eq!(
            compilation.state.checked_control_flow.is_published(key),
            Ok(true)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(key),
            Ok(true)
        );

        assert_eq!(
            diagnostic_kinds(first),
            [DiagnosticKind::BindingNameAlreadyDefined]
        );
    }

    #[test]
    fn check_diagnostics_request_embedded_constant_expressions() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(pos source: [i32; false])\n",
            "{\n",
            "    let copy: [i32; false] = source;\n",
            "}\n",
        ));

        let keys = compilation
            .declared_unit_keys()
            .unwrap_or_else(|error| panic!("callable unit must be discoverable: {error:?}"));

        let [key] = keys.as_slice() else {
            panic!("test source must produce one callable unit");
        };

        let declared = compilation
            .declared_value_type_templates(key.clone())
            .unwrap_or_else(|error| panic!("declared types must publish: {error:?}"));

        let occurrence = declared
            .value()
            .evidence()
            .iter()
            .flat_map(|evidence| evidence.template().constant_expressions())
            .next()
            .unwrap_or_else(|| panic!("array type must retain its length expression"));

        let checked = compilation
            .embedded_constant_term(occurrence)
            .unwrap_or_else(|error| panic!("embedded constant must recover: {error:?}"));

        assert!(!checked.diagnostics().is_empty());

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingIncompatibleExpressionType)
        );
    }

    #[test]
    fn bare_generic_constant_arguments_bind_as_embedded_values() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Buffer<const size: usize>\n",
            "{\n",
            "}\n",
            "func use_buffer<const count: usize>(pos value: Buffer<count>)\n",
            "{\n",
            "}\n",
        ));

        let keys = compilation
            .declared_unit_keys()
            .unwrap_or_else(|error| panic!("callable unit must be discoverable: {error:?}"));

        let key = keys
            .into_iter()
            .find(|key| key.kind() == BoundUnitKind::CallableBody)
            .unwrap_or_else(|| panic!("test source must produce one callable body"));

        let declared = compilation
            .declared_value_type_templates(key)
            .unwrap_or_else(|error| panic!("declared types must publish: {error:?}"));

        let occurrence = declared
            .value()
            .evidence()
            .iter()
            .flat_map(|evidence| evidence.template().constant_expressions())
            .next()
            .unwrap_or_else(|| panic!("generic argument must retain its constant expression"));

        let checked = compilation
            .embedded_constant_term(occurrence)
            .unwrap_or_else(|error| panic!("embedded constant must bind: {error:?}"));

        let term = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"))
            .constant_term_data(*checked.value())
            .unwrap_or_else(|error| panic!("embedded constant term must resolve: {error:?}"));

        assert!(checked.diagnostics().is_empty());

        assert!(matches!(term.as_ref(), ConstantTermData::Parameter(_)));
    }

    #[test]
    fn malformed_calls_to_known_functions_publish_diagnostics_without_panicking() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func known(pos value: i32)\n",
            "{\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    known(value = );\n",
            "}\n",
        ));

        assert!(!compilation.check_diagnostics().is_empty());
    }

    #[test]
    fn package_diagnostics_include_nested_unit_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let callable = lambda()\n",
            "    {\n",
            "        let value = 1;\n",
            "        let value = 2;\n",
            "    };\n",
            "}\n",
        ));

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [DiagnosticKind::BindingNameAlreadyDefined]
        );
    }

    #[test]
    fn package_diagnostics_bind_every_contract_clause_expression() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func check() requires(true, missing)\n",
            "{\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("contract-clause key must be discoverable: {error:?}"),
        };

        let Some(key) = keys
            .into_iter()
            .find(|key| key.kind() == BoundUnitKind::ContractClause)
        else {
            panic!("test source must produce a contract-clause key");
        };

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [DiagnosticKind::BindingUnresolvedName]
        );

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("contract clause must bind: {error:?}"),
        };

        let BoundUnitRoot::ExpressionSequence(root) = bound.value().root() else {
            panic!("contract clause must publish an expression sequence");
        };

        let Some(sequence) = bound.value().tree().block(root) else {
            panic!("contract-clause sequence root must resolve");
        };

        assert_eq!(sequence.items().len(), 2);
    }

    #[test]
    fn postcondition_result_reaches_the_semantic_unit_context() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func check() -> i32 ensures(result == 1)\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("contract-clause key must be discoverable: {error:?}"),
        };

        let Some(key) = keys
            .into_iter()
            .find(|key| key.kind() == BoundUnitKind::ContractClause)
        else {
            panic!("test source must produce a contract-clause key");
        };

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("contract clause must bind: {error:?}"),
        };

        assert!(bound.diagnostics().is_empty());

        let symbols = match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("symbol graph must be available: {error:?}"),
        };

        let entry = match semantic_unit_context(symbols, bound.value()) {
            Ok(entry) => entry,
            Err(error) => panic!("semantic unit context must be available: {error:?}"),
        };

        let SemanticUnitContext::ContractClause(entry) = entry else {
            panic!("contract clause must produce a contract-clause checker entry");
        };

        let [result] = bound.value().local_symbols().postcondition_results() else {
            panic!("value-producing postcondition must declare one result symbol");
        };

        assert_eq!(entry.kind(), CallableContractClauseKind::Ensures);
        assert_eq!(entry.result(), Some(result.id()));
    }

    #[test]
    fn contract_clause_entries_preserve_kind_and_exact_result_availability() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func omitted() ensures(true)\n",
            "{\n",
            "}\n",
            "func unit_result() -> unit ensures(true)\n",
            "{\n",
            "}\n",
            "func never_result() -> never ensures(true)\n",
            "{\n",
            "}\n",
            "func value_result() -> i32 requires(true) ensures(result == 1) with(true)\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("contract-clause keys must be discoverable: {error:?}"),
        };

        let symbols = match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("symbol graph must be available: {error:?}"),
        };

        let mut entries = Vec::new();

        for key in keys
            .into_iter()
            .filter(|key| key.kind() == BoundUnitKind::ContractClause)
        {
            let source_start = key.source().syntax().full_range().start();

            let bound = match compilation.bound_unit(key) {
                Ok(bound) => bound,
                Err(error) => panic!("contract clause must bind: {error:?}"),
            };

            assert!(bound.diagnostics().is_empty());

            let entry = match semantic_unit_context(symbols, bound.value()) {
                Ok(entry) => entry,
                Err(error) => panic!("semantic unit context must be available: {error:?}"),
            };

            let SemanticUnitContext::ContractClause(entry) = entry else {
                panic!("contract clause must produce a contract-clause checker entry");
            };

            entries.push((source_start, entry.kind(), entry.result().is_some()));
        }

        entries.sort_unstable_by_key(|entry| entry.0);

        assert_eq!(
            entries
                .into_iter()
                .map(|(_, kind, has_result)| (kind, has_result))
                .collect::<Vec<_>>(),
            [
                (CallableContractClauseKind::Ensures, false),
                (CallableContractClauseKind::Ensures, false),
                (CallableContractClauseKind::Ensures, false),
                (CallableContractClauseKind::Requires, false),
                (CallableContractClauseKind::Ensures, true),
                (CallableContractClauseKind::Static, false),
            ]
        );
    }

    #[test]
    fn declarations_without_semantic_units_do_not_force_binding() {
        let compilation = compilation(concat!(
            "module app;\n",
            "extern func write(value: i32);\n",
            "trait Writer\n",
            "{\n",
            "    func flush();\n",
            "}\n",
        ));

        assert!(
            compilation
                .declared_unit_keys()
                .is_ok_and(|keys| keys.is_empty())
        );

        assert!(compilation.check_diagnostics().is_empty());
    }

    #[test]
    fn malformed_bodies_publish_recovered_semantic_facts_without_panicking() {
        let cases = [
            concat!(
                "module app;\n",
                "func missing_initializer()\n",
                "{\n",
                "    let value = ;\n",
                "}\n",
            ),
            concat!(
                "module app;\n",
                "func malformed_pattern()\n",
                "{\n",
                "    let (first, second = 1;\n",
                "}\n",
            ),
        ];

        for source in cases {
            let compilation = compilation(source);

            let keys = match compilation.declared_unit_keys() {
                Ok(keys) => keys,
                Err(error) => panic!("recovered unit keys must be discoverable: {error:?}"),
            };

            let mut recovered = false;

            assert!(!keys.is_empty(), "{source}");

            for key in keys {
                let checked = match compilation.control_flow(key) {
                    Ok(checked) => checked,
                    Err(error) => panic!("recovered semantic check failed: {source}: {error:?}"),
                };

                recovered |= checked.value().is_recovered();
            }

            assert!(recovered, "{source}");
            assert!(!compilation.check_diagnostics().is_empty(), "{source}");
        }
    }

    #[test]
    fn concurrent_package_diagnostic_requests_publish_one_cached_result() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    missing;\n",
            "}\n",
        ));

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        if let Err(error) = compilation
            .state
            .check_diagnostics
            .set_test_observer(gate.observer())
        {
            panic!("package diagnostic fact must accept a test observer: {error:?}");
        }

        let diagnostics = std::thread::scope(|scope| {
            let owner = scope.spawn(|| compilation.check_diagnostics());

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let waiter = scope.spawn(|| compilation.check_diagnostics());

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            [owner, waiter].map(|handle| match handle.join() {
                Ok(diagnostics) => diagnostics,
                Err(_) => panic!("package diagnostic request panicked"),
            })
        });

        assert!(
            diagnostics
                .iter()
                .skip(1)
                .all(|result| std::ptr::eq(diagnostics[0], *result))
        );

        assert_eq!(
            diagnostic_kinds(diagnostics[0]),
            [DiagnosticKind::BindingUnresolvedName]
        );
    }

    #[test]
    fn semantic_diagnostics_ignore_serial_parallel_and_reversed_demand_order() {
        let sources = [
            concat!(
                "module app;\n",
                "func first()\n",
                "{\n",
                "    let value = 1;\n",
                "    let value = 2;\n",
                "}\n",
            ),
            concat!(
                "module app;\n",
                "func second()\n",
                "{\n",
                "    missing;\n",
                "}\n",
            ),
        ];

        let serial = compilation_with_sources_and_worker_budget(&sources, WorkerBudget::serial());

        let serial_keys = match serial.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("serial unit keys must be discoverable: {error:?}"),
        };

        for key in serial_keys {
            if let Err(error) = serial.control_flow(key) {
                panic!("serial semantic demand failed: {error:?}");
            }
        }

        let parallel_budget = match WorkerBudget::new(4) {
            Ok(budget) => budget,
            Err(error) => panic!("parallel test budget must be valid: {error:?}"),
        };

        let parallel = compilation_with_sources_and_worker_budget(&sources, parallel_budget);

        let mut parallel_keys = match parallel.declared_unit_keys() {
            Ok(keys) => keys,
            Err(error) => panic!("parallel unit keys must be discoverable: {error:?}"),
        };

        parallel_keys.reverse();

        assert_eq!(parallel_keys.len(), 2);

        let gates = parallel_keys
            .iter()
            .map(|key| {
                let gate = FactTestGate::holding(FactCellTestEvent::Computing);

                if let Err(error) = parallel
                    .state
                    .checked_control_flow
                    .set_test_observer(key, gate.observer())
                {
                    panic!("control-flow fact must accept a test observer: {error:?}");
                }

                gate
            })
            .collect::<Vec<_>>();

        // Bound-unit keys share immutable identity storage across worker requests.
        std::thread::scope(|scope| {
            let parallel = &parallel;

            let handles = parallel_keys
                .into_iter()
                .map(|key| scope.spawn(move || parallel.control_flow(key)))
                .collect::<Vec<_>>();

            for gate in &gates {
                gate.wait_until_observed(FactCellTestEvent::Computing, 1);
            }

            for (gate, handle) in gates.iter().zip(handles) {
                gate.release();

                match handle.join() {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => panic!("parallel semantic demand failed: {error:?}"),
                    Err(_) => panic!("parallel semantic demand panicked"),
                }
            }
        });

        assert_eq!(serial.check_diagnostics(), parallel.check_diagnostics());

        assert_eq!(
            diagnostic_kinds(serial.check_diagnostics()),
            [
                DiagnosticKind::BindingNameAlreadyDefined,
                DiagnosticKind::BindingUnresolvedName,
            ]
        );
    }
}
