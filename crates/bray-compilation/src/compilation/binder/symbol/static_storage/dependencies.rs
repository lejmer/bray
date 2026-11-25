use crate::compilation::binder::{
    BindingQueryResult, CompilationBindingContext,
    semantic_contract_binding_error as binding_contract,
};
use crate::compilation::{SemanticDataKind, SemanticQueryContext, SemanticQueryViolation};
use bray_binder::BindingQueryContext;
use bray_bound_tree::BoundUnitKey;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticDependencySubjectKind, DiagnosticId, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{StaticStorageDuration, StaticSymbolId};
pub(super) fn validate_static_dependency_duration(
    context: &CompilationBindingContext<'_>,
    duration: StaticStorageDuration,
    contract: bray_symbols::DependencyContractTemplateId,
    key: &BoundUnitKey,
) -> BindingQueryResult<Vec<Diagnostic>> {
    if duration != StaticStorageDuration::Product {
        return Ok(Vec::new());
    }

    let contract = context
        .semantic_values()
        .dependency_contract_template_data(contract);

    if !contract
        .requirements()
        .iter()
        .any(requirement_reaches_exact_thread)
    {
        return Ok(Vec::new());
    }

    let syntax = key.source().syntax();
    let span = SourceSpan::new(syntax.source_id(), syntax.full_range());

    Ok(vec![
        Diagnostic::new(
            DiagnosticId::new(span.start().bytes()),
            DiagnosticKind::CheckingStaticDependencyOutlivesOwner,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidConstantExpression,
            span,
        ))
        .with_arg(DiagnosticArg::dependency_subject_kind(
            DiagnosticDependencySubjectKind::ExactThreadStatic,
        )),
    ])
}

fn requirement_reaches_exact_thread(requirement: &bray_symbols::DependencyRequirement) -> bool {
    match requirement {
        bray_symbols::DependencyRequirement::CleanupDependencies { requirements, .. } => {
            requirements.iter().any(requirement_reaches_exact_thread)
        }
        bray_symbols::DependencyRequirement::FixedPoint {
            definitions,
            result,
        } => definitions
            .iter()
            .flat_map(|definition| definition.iter())
            .chain(result.iter())
            .any(requirement_reaches_exact_thread),
        bray_symbols::DependencyRequirement::Variable { .. }
        | bray_symbols::DependencyRequirement::SynchronousCleanup(_)
        | bray_symbols::DependencyRequirement::FinalizerBodyCleanup(_)
        | bray_symbols::DependencyRequirement::DefaultCleanup { .. } => false,
        bray_symbols::DependencyRequirement::ResultCall { inputs, .. } => {
            inputs.iter().any(|input| {
                input
                    .values()
                    .iter()
                    .chain(input.storage())
                    .any(requirement_reaches_exact_thread)
            })
        }
        bray_symbols::DependencyRequirement::Direct { subject, .. } => subject.subject_root().static_duration() == Some(StaticStorageDuration::ExactThread),
        bray_symbols::DependencyRequirement::Guarded(guarded) => {
            guard_reaches_exact_thread(guarded.guard())
                || guarded
                    .requirements()
                    .iter()
                    .any(requirement_reaches_exact_thread)
        }
    }
}

fn guard_reaches_exact_thread(guard: &bray_symbols::DependencyGuard) -> bool {
    let subject = match guard {
        bray_symbols::DependencyGuard::NullablePresent(subject)
        | bray_symbols::DependencyGuard::ActiveUnionVariant { subject, .. } => subject,
    };

    subject.subject_root().static_duration() == Some(StaticStorageDuration::ExactThread)
}

pub(in crate::compilation) fn static_dependencies_from_contract(
    context: &CompilationBindingContext<'_>,
    contract: bray_symbols::DependencyContractTemplateId,
) -> BindingQueryResult<Vec<StaticSymbolId>> {
    let contract = context
        .semantic_values()
        .dependency_contract_template_data(contract);

    let mut dependencies = Vec::new();

    for requirement in contract.requirements() {
        collect_static_requirement_dependencies(context.semantic_values(), requirement, &mut dependencies);
    }

    dependencies.sort_unstable();
    dependencies.dedup();

    Ok(dependencies)
}

pub(super) fn witness_requirements_from_contract(
    context: &CompilationBindingContext<'_>,
    contract: bray_symbols::DependencyContractTemplateId,
) -> BindingQueryResult<Vec<bray_symbols::SymbolKey>> {
    let contract = context
        .semantic_values()
        .dependency_contract_template_data(contract);

    let mut requirements = Vec::new();

    for requirement in contract.requirements() {
        collect_witness_requirements(context, requirement, &mut requirements)?;
    }

    requirements.sort();
    requirements.dedup();

    Ok(requirements)
}

fn collect_witness_requirements(
    context: &CompilationBindingContext<'_>,
    requirement: &bray_symbols::DependencyRequirement,
    requirements: &mut Vec<bray_symbols::SymbolKey>,
) -> BindingQueryResult<()> {
    match requirement {
        bray_symbols::DependencyRequirement::FixedPoint {
            definitions,
            result,
        } => {
            for nested in definitions
                .iter()
                .flat_map(|definition| definition.iter())
                .chain(result.iter())
            {
                collect_witness_requirements(context, nested, requirements)?;
            }
        }
        bray_symbols::DependencyRequirement::CleanupDependencies {
            requirements: nested,
            ..
        } => {
            for requirement in nested.iter() {
                collect_witness_requirements(context, requirement, requirements)?;
            }
        }
        bray_symbols::DependencyRequirement::Variable { .. }
        | bray_symbols::DependencyRequirement::SynchronousCleanup(_)
        | bray_symbols::DependencyRequirement::FinalizerBodyCleanup(_)
        | bray_symbols::DependencyRequirement::DefaultCleanup { .. } => {}
        bray_symbols::DependencyRequirement::ResultCall { inputs, .. } => {
            for nested in inputs
                .iter()
                .flat_map(|input| input.values().iter().chain(input.storage()))
            {
                collect_witness_requirements(context, nested, requirements)?;
            }
        }
        bray_symbols::DependencyRequirement::Direct { subject, .. } => {
            collect_witness_subject_requirement(context, subject.subject_root(), requirements)?;
        }
        bray_symbols::DependencyRequirement::Guarded(guarded) => {
            let subject = match guarded.guard() {
                bray_symbols::DependencyGuard::NullablePresent(subject)
                | bray_symbols::DependencyGuard::ActiveUnionVariant { subject, .. } => subject,
            };

            collect_witness_subject_requirement(context, subject.subject_root(), requirements)?;

            for requirement in guarded.requirements() {
                collect_witness_requirements(context, requirement, requirements)?;
            }
        }
    }

    Ok(())
}

fn collect_witness_subject_requirement(
    context: &CompilationBindingContext<'_>,
    root: bray_symbols::DependencySubjectRoot,
    requirements: &mut Vec<bray_symbols::SymbolKey>,
) -> BindingQueryResult<()> {
    let witness = match root {
        bray_symbols::DependencySubjectRoot::ImplementationWitness(witness) => witness,
        bray_symbols::DependencySubjectRoot::ImplementationRequirement(requirement) => {
            let selected = context
                .compilation()
                .implementation_selection_result_with_cancellation(
                    requirement,
                    context.cancellation(),
                )
                .map_err(super::super::binding::binder_error)?;

            let bray_symbols::ImplementationSelection::Selected(witness) = selected.value() else {
                return Ok(());
            };

            *witness
        }
        _ => return Ok(()),
    };

    let instance = context
        .semantic_values()
        .implementation_instance_data(witness);

    let key = context
        .symbols()
        .symbol_key(instance.definition().into_any())
        .ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(instance.definition().into_any()),
                SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
            )
        })?;

    requirements.push(key.clone());

    Ok(())
}

fn collect_static_requirement_dependencies(
    values: &bray_symbols::SemanticValueStore,
    requirement: &bray_symbols::DependencyRequirement,
    dependencies: &mut Vec<StaticSymbolId>,
) {
    match requirement {
        bray_symbols::DependencyRequirement::FixedPoint {
            definitions,
            result,
        } => {
            for nested in definitions
                .iter()
                .flat_map(|definition| definition.iter())
                .chain(result.iter())
            {
                collect_static_requirement_dependencies(values, nested, dependencies);
            }
        }
        bray_symbols::DependencyRequirement::CleanupDependencies { requirements, .. } => {
            for requirement in requirements.iter() {
                collect_static_requirement_dependencies(values, requirement, dependencies);
            }
        }
        bray_symbols::DependencyRequirement::Variable { .. }
        | bray_symbols::DependencyRequirement::SynchronousCleanup(_)
        | bray_symbols::DependencyRequirement::FinalizerBodyCleanup(_)
        | bray_symbols::DependencyRequirement::DefaultCleanup { .. } => {}
        bray_symbols::DependencyRequirement::ResultCall { inputs, .. } => {
            for nested in inputs
                .iter()
                .flat_map(|input| input.values().iter().chain(input.storage()))
            {
                collect_static_requirement_dependencies(values, nested, dependencies);
            }
        }
        bray_symbols::DependencyRequirement::Direct { subject, .. } => {
            dependencies.extend(subject.subject_root().static_declaration(values));
        }
        bray_symbols::DependencyRequirement::Guarded(guarded) => {
            let subject = match guarded.guard() {
                bray_symbols::DependencyGuard::NullablePresent(subject)
                | bray_symbols::DependencyGuard::ActiveUnionVariant { subject, .. } => subject,
            };

            dependencies.extend(subject.subject_root().static_declaration(values));

            for requirement in guarded.requirements() {
                collect_static_requirement_dependencies(values, requirement, dependencies);
            }
        }
    }
}
