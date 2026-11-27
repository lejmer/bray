use bray_symbols::{CallableParameterDefaultQuery, CallableParameterDefaultValue, StructFieldDefaultQuery, StructFieldDefaultValue, UnionPayloadDefaultValue, UnionPayloadFieldDefaultQuery};

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_checker::ResultDependencyOwner;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableResultDependenciesQuery, CallableSymbolId, DependencyContractTemplateId,
    SymbolQueryRequest,
};

use super::super::binding::{CompilationSymbolQueryEvaluator, binder_error};
use super::super::cache::CompilationSymbolSemantics;
use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};

use crate::fact::SymbolQueryCache;
use super::discovery::source_component;

pub(in crate::compilation) fn source_body_semantics(
    context: &CompilationBindingContext<'_>,
    key: &bray_bound_tree::BoundUnitKey,
) -> BindingQueryResult<DiagnosticResult<std::sync::Arc<bray_bound_tree::CheckedBodySemantics>>> {
    let owner = if key.kind() == BoundUnitKind::AnonymousCallable {
        ResultDependencyOwner::Anonymous(key.clone())
    } else {
        ResultDependencyOwner::Declaration(
            context
                .symbols()
                .symbol_for_key(key.declared_owner())
                .expect("declared body retains its symbol"),
        )
    };

    let component =
        source_component(context, &owner)?.expect("source body belongs to a source component");

    // The consumer shares the component's immutable body and diagnostic allocations.
    Ok(component.result().value()[&owner].body.clone())
}

impl CompilationSymbolQueryEvaluator<CallableResultDependenciesQuery>
    for CompilationSymbolSemantics
{
    fn cache(&self) -> &SymbolQueryCache<CallableResultDependenciesQuery> {
        &self.callable_result_dependencies
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<CallableResultDependenciesQuery>,
    ) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
        infer_reachable_results(context, request.owner())
    }
}

fn infer_reachable_results(
    context: &CompilationBindingContext<'_>,
    root: CallableSymbolId,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
    if let Some(address) = context.imported_semantic_address(root.into_any())? {
        return super::super::imported::imported_result_dependencies(context, address);
    }

    if let Some(template) = context
        .compilation()
        .available_compiler_known_symbols()
        .symbol_implementation(root.into_any())
        .and_then(|hook| {
            bray_checker::generator_result_dependencies(
                context.compilation().available_compiler_known_symbols(),
                hook,
            )
        })
    {
        let template = context
            .semantic_values()
            .intern_dependency_contract_template(template)
            .expect("generator result contract retains its receiver values");

        return Ok(DiagnosticResult::without_diagnostics(template));
    }

    if let Some(result) = source_summary(context, root.into_any())? {
        return Ok(result);
    }

    let allocation = if crate::compilation::standard_library::recognized_declaration(
        context,
        root.into_any(),
        "StandardPlatformHeapAlignedAllocate",
    ).map_err(binder_error)? {
        // The fallible allocator establishes only origin separation, not a live
        // allocation or any access condition. Null remains an ordinary result.
        Some(bray_symbols::DependencyBorrowRelation::new_allocation(
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result),
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result),
            bray_symbols::DependencyReferenceKind::Address,
        ))
    } else {
        None
    };

    // These leaves access OS-managed handles or thread state without changing Bray storage.
    // They remain impure and still require foreign-call capability.
    let preserves_storage = [
        "StandardLinuxCurrentThreadIdentity",
        "StandardLinuxThreadStorageLoad",
        "StandardWindowsCurrentThreadIdentity",
        "StandardWindowsFiberStorageLoad",
        "StandardWindowsFindClose",
        "StandardWindowsLastErrorRead",
        "StandardWindowsLastErrorWrite",
    ].into_iter().try_fold(false, |matched, key| {
        if matched {
            return Ok(true);
        }

        crate::compilation::standard_library::recognized_declaration(context, root.into_any(), key)
            .map_err(binder_error)
    })?;

    let effects = bray_symbols::DependencyStorageEffect::capture_free_call()
        .into_iter().filter(|_| !preserves_storage);

    let rows = allocation.map(|relation| bray_symbols::CapturedValueOutcome::new(None, [])
        .with_storage([relation], []).with_effects(effects.clone()));

    // A declaration without a body has explicit arguments and an explicit receiver.
    let template = context
        .semantic_values()
        .intern_dependency_contract_template(
            bray_symbols::DependencyContractTemplateData::new([])
                .with_possible_effects(effects)
                .with_captured_values(rows),
        )
        .expect("declared result dependency template satisfies the semantic store contract");

    Ok(DiagnosticResult::new(template, DiagnosticBag::new()))
}

pub(in crate::compilation) fn expression_result_dependencies(
    context: &CompilationBindingContext<'_>,
    key: &bray_bound_tree::BoundUnitKey,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
    if key.kind() == BoundUnitKind::RuntimeDefault {
        let owner = context
            .symbols()
            .symbol_for_key(key.declared_owner())
            .expect("runtime default retains its provider");

        return source_summary(context, owner)
            .map(|result| result.expect("source default belongs to a summary component"));
    }

    let body = context
        .compilation()
        .body_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(binder_error)?;

    let diagnostics = body.result().diagnostics().clone();

    let values = context.semantic_values();

    let result = values
        .intern_dependency_contract_template(body.result().value().storage_flow().dependencies().clone())
        .expect("checked default summary retains its semantic identities");

    Ok(DiagnosticResult::new(result, diagnostics))
}

fn source_summary(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
) -> BindingQueryResult<Option<DiagnosticResult<DependencyContractTemplateId>>> {
    // Diagnostic bags share their persistent collection with the component publication.
    Ok(
        source_component(context, &ResultDependencyOwner::Declaration(owner))?.map(|component| {
            DiagnosticResult::new(
                component.result().value()[&ResultDependencyOwner::Declaration(owner)].dependencies,
                component.result().diagnostics().clone(),
            )
        }),
    )
}

pub(in crate::compilation) fn default_result_dependencies(
    context: &CompilationBindingContext<'_>,
    provider: bray_symbols::DefaultValueProvider,
) -> BindingQueryResult<DependencyContractTemplateId> {
    summary(
        context,
        &ResultDependencyOwner::Declaration(provider.symbol()),
    )
    .map(|result| *result.value())
}

pub(super) fn summary(
    context: &CompilationBindingContext<'_>,
    owner: &ResultDependencyOwner,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {

    let owner = match owner {
        ResultDependencyOwner::Declaration(symbol) => *symbol,
        ResultDependencyOwner::Anonymous(key) => {
            return anonymous_result_dependencies(context, key);
        }
    };

    if let AnySymbolId::Static(declaration) = owner {
        return static_result_dependencies(context, declaration);
    }

    // Returned diagnostics share the checked query's persistent bag in every branch.
    if let Some(callable) = CallableSymbolId::try_from_any(owner) {
        let result = context.resolve_symbol_query(SymbolQueryRequest::<
            CallableResultDependenciesQuery,
        >::new(callable))?;

        return Ok(DiagnosticResult::new(
            *result.value(),
            result.diagnostics().clone(),
        ));
    }

    if let Some(result) = source_summary(context, owner)? {
        return Ok(result);
    }

    let subject = context
        .runtime_default_subject(owner)?
        .expect("selected default retains its declaration subject");

    match subject {
        AnySymbolId::CallableParameter(parameter) => {
            let result = context.resolve_symbol_query(SymbolQueryRequest::<
                CallableParameterDefaultQuery,
            >::new(parameter))?;

            let CallableParameterDefaultValue::Valid(surface) = result.value().value() else {
                panic!("selected imported parameter default retains its checked surface");
            };

            Ok(DiagnosticResult::new(
                surface.behavior().result_dependencies(),
                result.diagnostics().clone(),
            ))
        }
        AnySymbolId::StructField(field) => {
            let result = context
                .resolve_symbol_query(SymbolQueryRequest::<StructFieldDefaultQuery>::new(field))?;

            let StructFieldDefaultValue::Valid(surface) = result.value().value() else {
                panic!("selected imported field default retains its checked surface");
            };

            Ok(DiagnosticResult::new(
                surface.behavior().result_dependencies(),
                result.diagnostics().clone(),
            ))
        }
        AnySymbolId::UnionPayloadField(field) => {
            let result = context.resolve_symbol_query(SymbolQueryRequest::<
                UnionPayloadFieldDefaultQuery,
            >::new(field))?;

            let UnionPayloadDefaultValue::Valid(surface) = result.value().value() else {
                panic!("selected imported payload default retains its checked surface");
            };

            Ok(DiagnosticResult::new(
                surface.behavior().result_dependencies(),
                result.diagnostics().clone(),
            ))
        }
        _ => panic!("runtime default {owner:?} has a non-default subject {subject:?}"),
    }
}

pub(in crate::compilation) fn anonymous_result_dependencies(
    context: &CompilationBindingContext<'_>,
    key: &BoundUnitKey,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
    let owner = ResultDependencyOwner::Anonymous(key.clone());

    let component = source_component(context, &owner)?
        .expect("anonymous callable belongs to a source component");

    Ok(DiagnosticResult::new(
        component.result().value()[&owner].dependencies,
        component.result().diagnostics().clone(),
    ))
}

/// Static authority uses the initializer's component, without demanding lifecycle graph validation.
pub(in crate::compilation) fn static_result_dependencies(
    context: &CompilationBindingContext<'_>,
    declaration: bray_symbols::StaticSymbolId,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
    if let Some(result) = source_summary(context, declaration.into())? {
        return Ok(result);
    }

    let template = context.resolve_symbol_query(SymbolQueryRequest::<
        bray_symbols::StaticInstanceTemplateQuery,
    >::new(declaration))?;

    Ok(DiagnosticResult::new(
        template.value().dependency_contract(),
        template.diagnostics().clone(),
    ))
}
