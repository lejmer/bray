use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::malformed_directive_argument_diagnostic;
use bray_bound_tree::{BoundReferenceTarget, BoundUnitKey, BoundUnitKind};
use bray_checker::{
    CheckerUnitView, ConstantEvaluationInput, ConstantEvaluator, ConstantReferenceResolution,
    DefaultConstantEvaluator,
};
use bray_compiler_known::RepresentationRole;
use bray_declarations::ModulePartId;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticRelatedLocation,
    DiagnosticRelatedLocationKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, ConstantSymbolId, ConstantValueData, ConstantValueId,
    ConstantValueKind, DirectiveArgumentTemplate, DirectiveKind, DirectiveSurface,
    DirectiveTemplate, IntegerConstant, IntegerSign, ModuleContributionGate, ModuleSymbol,
    NamedTypeSymbolId, ProductKind, StructSymbolId, SymbolProvider, TargetFactDependency, TypeId,
};
use bray_target::{TargetFactKind, TargetFactValue};

use super::Compilation;
use super::binder::bind_module_part_directives_for_selection;
use super::checker::checker_result;
use super::constant::collect_constant_references;
use super::diagnostics::source_diagnostic;
use super::directive::{directive_source_text, first_directive};
use super::substitution::named_type;
use super::unit::semantic_unit_context_for;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn module_contribution_gate(
        &self,
        part: ModulePartId,
    ) -> Result<Arc<DiagnosticResult<ModuleContributionGate>>, FactQueryError> {
        self.module_contribution_gate_with_cancellation(part, &self.state.cancellation)
    }

    pub(in crate::compilation) fn module_contribution_gate_with_cancellation(
        &self,
        part: ModulePartId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<ModuleContributionGate>>, FactQueryError> {
        let cell = self.state.module_contribution_gates.cell(part)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ModuleContributionGate(part),
            cancellation,
            || {
                self.compute_module_contribution_gate(part, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    pub(in crate::compilation) fn target_dependencies_hold(
        &self,
        dependencies: &[TargetFactDependency],
    ) -> Result<bool, FactQueryError> {
        let provider = self.available_compiler_known_symbols().provider();

        for dependency in dependencies {
            let Some(fact) = provider.symbol_target_fact(dependency.fact()) else {
                return Ok(false);
            };

            let Some(symbol) =
                SymbolProvider::<ConstantSymbolId>::symbol(provider, dependency.fact())
            else {
                return Ok(false);
            };

            if symbol.key() != dependency.key() {
                return Ok(false);
            }

            if self.target_fact_value(fact)? != dependency.value() {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub(in crate::compilation) fn target_constant_value(
        &self,
        definition: AnyConstantDefinitionId,
    ) -> Result<Option<ConstantValueId>, FactQueryError> {
        let AnyConstantDefinitionId::Constant(symbol) = definition else {
            return Ok(None);
        };

        let Some(fact) = self
            .available_compiler_known_symbols()
            .provider()
            .symbol_target_fact(symbol)
        else {
            return Ok(None);
        };

        self.target_fact_value(fact).map(Some)
    }

    fn module_contribution_gate_key(
        &self,
        module: &ModuleSymbol,
        argument: &DirectiveArgumentTemplate,
    ) -> Result<BoundUnitKey, FactQueryError> {
        let source = self.bound_source(argument.expression().syntax())?;

        // The unit key retains the module's Arc-backed identity after this graph lookup.
        BoundUnitKey::target_gate(module.key().clone(), source)
            .ok_or(FactQueryError::InfrastructureFailure)
    }

    fn compute_module_contribution_gate(
        &self,
        part: ModulePartId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ModuleContributionGate>, FactQueryError> {
        let declarations = self.declaration_table();

        let part = declarations
            .module_part(part)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let symbols = self.discovery_symbol_graph()?;

        let module = symbols
            .module_for_part(part.id())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let context = self.discovery_binder_facts(cancellation)?;

        let directives = bind_module_part_directives_for_selection(&context, module.id(), part)
            .map_err(super::binder::binder_fact_error)?;

        let (directives, mut diagnostics) = directives.into_parts();

        add_duplicate_gate_diagnostics(&directives, &mut diagnostics);

        let test_directive = first_directive(&directives, DirectiveKind::Test);

        if let Some(directive) = test_directive
            && !directive.arguments().is_empty()
        {
            let text = directive_source_text(self, directive)?;

            diagnostics.add(
                source_diagnostic(
                    directive.syntax(),
                    DiagnosticKind::CheckingInvalidTestModuleDirective,
                )
                .with_arg(DiagnosticArg::actual_count(
                    u64::try_from(directive.arguments().len()).unwrap_or(u64::MAX),
                ))
                .with_arg(DiagnosticArg::token_text(text))
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::InvalidProductConfiguration,
                    SourceSpan::new(
                        directive.syntax().source_id(),
                        directive.syntax().full_range(),
                    ),
                ))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::TestDirectiveRequirements,
                )),
            );
        }

        let test_enabled = test_directive.is_none_or(|_| self.product_kind() == ProductKind::Test);

        let target_gate = match first_directive(&directives, DirectiveKind::Target) {
            Some(directive) => match directive.arguments().first() {
                Some(argument) => {
                    let key = self.module_contribution_gate_key(module, argument)?;

                    self.evaluate_module_target_gate(key, cancellation)?
                }
                None => {
                    let syntax = directive.syntax();

                    diagnostics.add(malformed_directive_argument_diagnostic(SourceSpan::new(
                        syntax.source_id(),
                        syntax.full_range(),
                    )));

                    DiagnosticResult::without_diagnostics(ModuleContributionGate::new(false, []))
                }
            },
            None => DiagnosticResult::without_diagnostics(ModuleContributionGate::new(true, [])),
        };

        let (target_gate, target_diagnostics) = target_gate.into_parts();

        diagnostics.add_range(target_diagnostics);

        // The combined gate retains the target gate's Arc-backed dependency identities.
        Ok(DiagnosticResult::new(
            ModuleContributionGate::new(
                test_enabled && target_gate.is_enabled(),
                target_gate.dependencies().iter().cloned(),
            ),
            diagnostics,
        ))
    }

    fn evaluate_module_target_gate(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ModuleContributionGate>, FactQueryError> {
        if key.kind() != BoundUnitKind::TargetGate {
            return Err(FactQueryError::InfrastructureFailure);
        }

        // Independently cached semantic facts retain the same Arc-backed unit identity.
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let semantics = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;
        let context = self.checker_context_for(&key, cancellation)?;

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let mut dependencies_by_expression = BTreeMap::new();

        let references = collect_constant_references(
            bound.result().value(),
            &semantics.result().value().1,
            |expression, target| {
                let (resolution, dependency) = self.target_gate_reference(target)?;

                if let Some(dependency) = dependency {
                    dependencies_by_expression.insert(expression, dependency);
                }

                Ok(resolution)
            },
        )?;

        let input = ConstantEvaluationInput::new(
            &semantics.result().value().0,
            &semantics.result().value().1,
        )
        .with_references(references);

        let unit = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
            .map_err(|error| {
                FactQueryError::CheckerInfrastructure(
                    bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
                )
            })?;

        let evaluated = checker_result(
            DefaultConstantEvaluator.evaluate_constant_with_references(unit, &input),
        )?;

        let diagnostics =
            DiagnosticBag::merged_all([semantics.result().diagnostics(), evaluated.diagnostics()]);

        let enabled = self.target_gate_is_enabled(evaluated.value().value(), &diagnostics)?;

        let dependencies: Vec<_> = if diagnostics.has_errors() {
            dependencies_by_expression.into_values().collect()
        } else {
            evaluated
                .value()
                .evaluated_references()
                .iter()
                .filter_map(|expression| dependencies_by_expression.get(expression).cloned())
                .collect()
        };

        Ok(DiagnosticResult::new(
            ModuleContributionGate::new(enabled, dependencies),
            diagnostics,
        ))
    }

    fn target_gate_reference(
        &self,
        target: BoundReferenceTarget,
    ) -> Result<(ConstantReferenceResolution, Option<TargetFactDependency>), FactQueryError> {
        let BoundReferenceTarget::Surface(AnySymbolId::Constant(symbol)) = target else {
            return Ok((ConstantReferenceResolution::Invalid, None));
        };

        let provider = self.available_compiler_known_symbols().provider();

        let Some(fact) = provider.symbol_target_fact(symbol) else {
            return Ok((ConstantReferenceResolution::Invalid, None));
        };

        let value = self.target_fact_value(fact)?;

        // Dependency evidence owns the provider's Arc-backed symbol key beyond this lookup.
        let key = SymbolProvider::<ConstantSymbolId>::symbol(provider, symbol)
            .map(bray_symbols::ConstantSymbol::key)
            .cloned()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        Ok((
            ConstantReferenceResolution::Value(value),
            Some(TargetFactDependency::new(key, symbol, value)),
        ))
    }

    fn target_gate_is_enabled(
        &self,
        value: ConstantValueId,
        diagnostics: &DiagnosticBag,
    ) -> Result<bool, FactQueryError> {
        if diagnostics.has_errors() {
            return Ok(false);
        }

        let value = self
            .semantic_value_store()?
            .constant_value_data(value)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match value.kind() {
            ConstantValueKind::Boolean(value) => Ok(*value),
            ConstantValueKind::Error => Ok(false),
            _ => Ok(false),
        }
    }

    pub(in crate::compilation) fn target_fact_value(
        &self,
        fact: TargetFactKind,
    ) -> Result<ConstantValueId, FactQueryError> {
        let fact_value = self.requested_target().profile().fact(fact);

        let value = match fact_value {
            TargetFactValue::String(value) => ConstantValueKind::String(Arc::from(value)),
            TargetFactValue::Usize(value) => ConstantValueKind::Integer(unsigned_integer(value)),
            TargetFactValue::Boolean(value) => ConstantValueKind::Boolean(value),
        };

        let ty = self.target_fact_type(fact)?;

        self.semantic_value_store()?
            .intern_constant_value(ConstantValueData::new(ty, value))
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    pub(in crate::compilation) fn target_fact_type(
        &self,
        fact: TargetFactKind,
    ) -> Result<TypeId, FactQueryError> {
        let role = match self.requested_target().profile().fact(fact) {
            TargetFactValue::String(_) => RepresentationRole::String,
            TargetFactValue::Usize(_) => RepresentationRole::ScalarUsize,
            TargetFactValue::Boolean(_) => RepresentationRole::ScalarBool,
        };

        let definition = self
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        named_type(
            self.semantic_value_store()?,
            NamedTypeSymbolId::Struct(definition),
        )
    }
}

fn add_duplicate_gate_diagnostics(directives: &DirectiveSurface, diagnostics: &mut DiagnosticBag) {
    for kind in [DirectiveKind::Target, DirectiveKind::Test] {
        let matching = directives
            .directives()
            .iter()
            .filter(|directive| directive.kind() == kind)
            .collect::<Vec<_>>();

        for (index, directive) in matching.iter().copied().enumerate().skip(1) {
            diagnostics.add(duplicate_module_contribution_directive(
                directive,
                &matching[..index],
            ));
        }
    }
}

fn duplicate_module_contribution_directive(
    directive: &DirectiveTemplate,
    prior: &[&DirectiveTemplate],
) -> Diagnostic {
    let syntax = directive.syntax();
    let span = SourceSpan::new(syntax.source_id(), syntax.full_range());

    let mut diagnostic = Diagnostic::new(
        DiagnosticId::new(span.range().start().bytes()),
        DiagnosticKind::CheckingDuplicateModuleContributionDirective,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_syntax_kind(syntax.syntax_kind()))
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::DuplicateModuleContribution,
        span,
    ));

    for prior in prior {
        let prior = prior.syntax();

        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::FirstDeclaration,
            SourceSpan::new(prior.source_id(), prior.full_range()),
        ));
    }

    diagnostic
}

fn unsigned_integer(value: u64) -> IntegerConstant {
    let bytes = value.to_be_bytes();

    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());

    IntegerConstant::new(IntegerSign::NonNegative, bytes[first..].iter().copied())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::{DiagnosticArg, DiagnosticKind};
    use bray_symbols::{
        AnyConstantDefinitionId, ConstantInstanceKey, ConstantValueKind, TargetFactDependency,
    };
    use bray_target::TargetFactKind;

    use super::super::constant::empty_concrete_substitution;
    use crate::test_support::compilation;

    #[test]
    fn target_gates_publish_exact_dependencies_and_selected_results() {
        let enabled = compilation("@target(target.scalar.U64) module app;");
        let enabled_gate = module_gate(&enabled);

        assert!(
            enabled_gate.diagnostics().is_empty(),
            "{:?}, dependencies: {:?}",
            enabled_gate.diagnostics(),
            enabled_gate.value().dependencies()
        );

        assert!(enabled_gate.value().is_enabled());

        let [dependency] = enabled_gate.value().dependencies() else {
            panic!("target gate must retain one exact target dependency");
        };

        assert_dependency(&enabled, dependency, TargetFactKind::ScalarU64);

        let disabled = compilation("@target(target.atomic.U64) module app;");
        let disabled_gate = module_gate(&disabled);

        assert!(
            disabled_gate.diagnostics().is_empty(),
            "{:?}",
            disabled_gate.diagnostics()
        );

        assert!(!disabled_gate.value().is_enabled());

        let [dependency] = disabled_gate.value().dependencies() else {
            panic!("target gate must retain one exact target dependency");
        };

        assert_dependency(&disabled, dependency, TargetFactKind::AtomicU64);
    }

    #[test]
    fn target_gates_use_ordinary_constant_boolean_operators() {
        let negated = compilation("@target(!target.atomic.U64) module app;");
        let gate = module_gate(&negated);

        assert!(gate.diagnostics().is_empty(), "{:?}", gate.diagnostics());
        assert!(gate.value().is_enabled());

        let [dependency] = gate.value().dependencies() else {
            panic!("target gate must retain one exact target dependency");
        };

        assert_dependency(&negated, dependency, TargetFactKind::AtomicU64);

        let composed = compilation("@target(target.scalar.U64 && !target.atomic.U64) module app;");
        let gate = module_gate(&composed);

        assert!(gate.diagnostics().is_empty(), "{:?}", gate.diagnostics());
        assert!(gate.value().is_enabled());
        assert_eq!(gate.value().dependencies().len(), 2);
    }

    #[test]
    fn target_gates_use_ordinary_constant_comparisons() {
        let compilation = compilation("@target(target.pointer.BITS == 64) module app;");
        let gate = module_gate(&compilation);

        assert!(gate.diagnostics().is_empty(), "{:?}", gate.diagnostics());
        assert!(gate.value().is_enabled());

        let [dependency] = gate.value().dependencies() else {
            panic!("target gate must retain one exact target dependency");
        };

        assert_dependency(&compilation, dependency, TargetFactKind::PointerBits);
    }

    #[test]
    fn target_gates_compare_c_abi_scalar_spellings() {
        let compilation = compilation("@target(target.c.LONG == \"i64\") module app;");
        let gate = module_gate(&compilation);

        assert!(gate.diagnostics().is_empty(), "{:?}", gate.diagnostics());
        assert!(gate.value().is_enabled());

        let [dependency] = gate.value().dependencies() else {
            panic!("target gate must retain the exact C ABI target dependency");
        };

        assert_dependency(&compilation, dependency, TargetFactKind::CLong);
    }

    #[test]
    fn target_gate_dependencies_exclude_short_circuited_references() {
        let compilation =
            compilation("@target(target.scalar.U64 || target.atomic.U64) module app;");

        let gate = module_gate(&compilation);

        assert!(gate.diagnostics().is_empty(), "{:?}", gate.diagnostics());
        assert!(gate.value().is_enabled());

        let [dependency] = gate.value().dependencies() else {
            panic!("target gate must retain only the evaluated target dependency");
        };

        assert_dependency(&compilation, dependency, TargetFactKind::ScalarU64);
    }

    #[test]
    fn repeated_and_concurrent_target_gate_demand_is_stable() {
        let compilation = compilation("@target(target.scalar.U64) module app;");

        let [part] = compilation.declaration_table().module_parts() else {
            panic!("fixture must contain one module contribution");
        };

        let results = std::thread::scope(|scope| {
            let first = scope.spawn(|| compilation.module_contribution_gate(part.id()));
            let second = scope.spawn(|| compilation.module_contribution_gate(part.id()));

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("target gate demand must not panic"))
                    .unwrap_or_else(|error| panic!("target gate demand must complete: {error:?}"))
            })
        });

        assert!(Arc::ptr_eq(&results[0], &results[1]));
        assert_eq!(results[0], results[1]);
    }

    #[test]
    fn non_boolean_target_gates_publish_disabled_recovered_results() {
        let compilation = compilation("@target(target.pointer.BITS) module app;");
        let gate = module_gate(&compilation);

        assert!(!compilation.check_diagnostics().is_empty());
        assert!(!gate.diagnostics().is_empty());
        assert!(!gate.value().is_enabled());

        let [dependency] = gate.value().dependencies() else {
            panic!("target gate must retain its selected target dependency");
        };

        assert_dependency(&compilation, dependency, TargetFactKind::PointerBits);
    }

    #[test]
    fn malformed_target_gates_do_not_contribute_to_the_selected_graph() {
        let compilation = compilation("@target() module app;");
        let gate = module_gate(&compilation);

        assert!(!gate.diagnostics().is_empty());
        assert!(!gate.value().is_enabled());

        assert!(
            compilation
                .product_source_graph()
                .unwrap_or_else(|error| panic!("source graph must be available: {error:?}"))
                .declarations()
                .module_parts()
                .is_empty()
        );
    }

    #[test]
    fn module_test_directives_reject_entry_constraints() {
        let compilation = compilation("@test(serial) module app;");
        let gate = module_gate(&compilation);

        assert!(
            gate.diagnostics().iter().any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::CheckingInvalidTestModuleDirective)
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            gate.diagnostics(),
            DiagnosticKind::CheckingInvalidTestModuleDirective,
        );

        let diagnostic = gate
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingInvalidTestModuleDirective)
            .next()
            .unwrap_or_else(|| panic!("invalid module test directive must be produced"));

        assert_eq!(diagnostic.args().first(), Some(&DiagnosticArg::actual_count(1)));
        assert_eq!(diagnostic.args().len(), 2);
    }

    #[test]
    fn target_facts_are_ordinary_selected_target_constants() {
        let compilation = compilation("module app;");
        let provider = compilation.available_compiler_known_symbols().provider();

        let symbol = provider
            .target_fact_symbol(TargetFactKind::ScalarU64)
            .unwrap_or_else(|| panic!("compiler-known target fact must be available"));

        let definition = AnyConstantDefinitionId::Constant(symbol);

        let substitution = empty_concrete_substitution(
            compilation
                .semantic_value_store()
                .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}")),
            definition,
        )
        .unwrap_or_else(|error| panic!("target fact substitution must be valid: {error:?}"));

        let instance = ConstantInstanceKey::new(definition, substitution, None);

        let value = compilation
            .constant_instance(instance)
            .unwrap_or_else(|error| panic!("target fact instance must be available: {error:?}"));

        assert!(value.diagnostics().is_empty());

        let data = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"))
            .constant_value_data(value.value().value())
            .unwrap_or_else(|error| panic!("target fact value must be available: {error:?}"));

        assert_eq!(data.kind(), &ConstantValueKind::Boolean(true));
    }

    fn module_gate(
        compilation: &crate::Compilation,
    ) -> Arc<bray_diagnostics::DiagnosticResult<bray_symbols::ModuleContributionGate>> {
        let [part] = compilation.declaration_table().module_parts() else {
            panic!("fixture must contain one module contribution");
        };

        compilation
            .module_contribution_gate(part.id())
            .unwrap_or_else(|error| panic!("target gate must be available: {error:?}"))
    }

    fn assert_dependency(
        compilation: &crate::Compilation,
        dependency: &TargetFactDependency,
        expected: TargetFactKind,
    ) {
        let provider = compilation.available_compiler_known_symbols().provider();

        assert_eq!(
            provider.symbol_target_fact(dependency.fact()),
            Some(expected)
        );

        assert_eq!(
            dependency.value(),
            compilation
                .target_fact_value(expected)
                .unwrap_or_else(|error| panic!(
                    "selected target fact must be available: {error:?}"
                ))
        );
    }
}
