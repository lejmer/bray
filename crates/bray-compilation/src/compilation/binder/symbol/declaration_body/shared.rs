use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult};
use bray_bound_tree::{
    BoundUnitKey, BoundUnitRoot, StorageAccessPurpose, StorageIdentity, StorageOperationStatus,
    StoragePlan, StorageProjection,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    BorrowKind, DependencyContractTemplateData, DependencyContractTemplateId, DependencyProjection,
    DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal, TypeId,
};

use crate::compilation::binder::CompilationBinderFacts;

pub(super) struct CheckedSourceExpression {
    pub(super) result: TypeId,
    pub(super) dependency_contract: DependencyContractTemplateId,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) is_recovered: bool,
}

pub(super) fn checked_source_expression(
    context: &CompilationBinderFacts<'_>,
    key: BoundUnitKey,
) -> BinderFactResult<CheckedSourceExpression> {
    let compilation = context.compilation();

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let BoundUnitRoot::Expression(root) = bound.result().value().root() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let control_flow = compilation
        .control_flow_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let storage = compilation
        .storage_plan_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let storage_flow = compilation
        .storage_flow_facts_with_cancellation(key, context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let (types, _) = semantics.result().value();

    let result = types
        .expression(root)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let expression = bound
        .result()
        .value()
        .view()
        .expression(root)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let diagnostics = DiagnosticBag::merged_all([
        bound.result().diagnostics(),
        semantics.result().diagnostics(),
        control_flow.result().diagnostics(),
        storage.result().diagnostics(),
        storage_flow.result().diagnostics(),
    ]);

    let dependency_contract = dependency_contract(
        context,
        storage.result().value(),
        storage_flow.result().value(),
    )?;

    Ok(CheckedSourceExpression {
        result: result.ty(),
        dependency_contract,
        diagnostics,
        is_recovered: expression.is_recovered()
            || result.is_recovered()
            || storage_flow.result().value().is_recovered(),
    })
}

fn dependency_contract(
    context: &CompilationBinderFacts<'_>,
    storage: &StoragePlan,
    flow: &bray_bound_tree::StorageFlowFacts,
) -> BinderFactResult<DependencyContractTemplateId> {
    let mut requirements = Vec::new();

    for operation in flow
        .operations()
        .iter()
        .filter(|operation| operation.status() == StorageOperationStatus::Valid)
    {
        let Some(subject) = portable_subject(context, storage, operation.access())? else {
            continue;
        };

        requirements.extend(
            requirement_kinds(operation.purpose())
                .map(|kind| DependencyRequirement::direct(subject.clone(), kind)),
        );

        let Some(access) = storage.access(operation.access()) else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        let bray_bound_tree::StorageAccessRoot::Borrow(capability) = access.root() else {
            continue;
        };

        let Some(capability) = storage.borrow_capability(capability) else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        if capability.entry_binding().is_some() {
            requirements.push(DependencyRequirement::direct(
                subject,
                DependencyRequirementKind::BorrowCapabilityActive(capability.kind()),
            ));
        }
    }

    context
        .semantic_values()
        .intern_dependency_contract_template(DependencyContractTemplateData::new(requirements))
        .map_err(|_| BinderFactError::DependencyUnavailable)
}

fn portable_subject(
    context: &CompilationBinderFacts<'_>,
    storage: &StoragePlan,
    access: bray_bound_tree::StorageAccessId,
) -> BinderFactResult<Option<DependencySubject>> {
    let Some(identity) = storage
        .root_identity(access)
        .and_then(|identity| storage.identity(identity))
    else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let root = match identity {
        StorageIdentity::Parameter(parameter) => {
            let parameter = context
                .symbols()
                .callable_parameter(parameter)
                .ok_or(BinderFactError::DependencyUnavailable)?;

            DependencySubjectRoot::Parameter(SymbolOrdinal::new(parameter.ordinal()))
        }
        StorageIdentity::PredicateParameter(parameter) => {
            let parameter = context
                .symbols()
                .predicate_parameter(parameter)
                .ok_or(BinderFactError::DependencyUnavailable)?;

            DependencySubjectRoot::Parameter(SymbolOrdinal::new(parameter.ordinal()))
        }
        StorageIdentity::Receiver(_) => DependencySubjectRoot::Receiver,
        StorageIdentity::Result(_) => DependencySubjectRoot::Result,
        StorageIdentity::LocalOwned(_)
        | StorageIdentity::AnonymousParameter(_)
        | StorageIdentity::Temporary(_)
        | StorageIdentity::Allocation(_)
        | StorageIdentity::CompilerCreated(_)
        | StorageIdentity::Error(_) => return Ok(None),
    };

    let projections = storage
        .resolved_projections(access)
        .ok_or(BinderFactError::DependencyUnavailable)?
        .iter()
        .map(|projection| portable_projection(*projection))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();

    Ok(Some(DependencySubject::new(root, projections)))
}

fn portable_projection(projection: StorageProjection) -> Option<DependencyProjection> {
    match projection {
        StorageProjection::ProductField(field) => Some(DependencyProjection::ProductField(field)),
        StorageProjection::TupleElement(ordinal) => {
            Some(DependencyProjection::TupleElement(ordinal))
        }
        StorageProjection::ActiveUnionPayloadField { field, .. } => {
            Some(DependencyProjection::UnionPayloadField(field))
        }
        StorageProjection::NullableValue => Some(DependencyProjection::NullableValue),
        StorageProjection::OwnedTarget => Some(DependencyProjection::OwnedTarget),
        StorageProjection::ElementFromStart(_)
        | StorageProjection::ElementFromEnd(_)
        | StorageProjection::Element(_)
        | StorageProjection::SliceRange { .. } => None,
    }
}

fn requirement_kinds(
    purpose: StorageAccessPurpose,
) -> impl Iterator<Item = DependencyRequirementKind> {
    let initialized = matches!(
        purpose,
        StorageAccessPurpose::Read
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Borrow(_)
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection
    )
    .then_some(DependencyRequirementKind::StorageInitialized);

    let exclusive = matches!(
        purpose,
        StorageAccessPurpose::Write
            | StorageAccessPurpose::Assignment
            | StorageAccessPurpose::Borrow(BorrowKind::Mutable)
    )
    .then_some(DependencyRequirementKind::ExclusiveMutationAuthority);

    std::iter::once(DependencyRequirementKind::StorageAlive)
        .chain(initialized)
        .chain(exclusive)
}

pub(super) fn syntax_diagnostics(
    context: &CompilationBinderFacts<'_>,
    anchor: bray_declarations::SyntaxAnchor,
) -> DiagnosticBag {
    let mut diagnostics = DiagnosticBag::new();

    diagnostics.add_range(
        context
            .compilation()
            .syntax_tree_result()
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.primary_span().is_some_and(|span| {
                    span.source_id() == anchor.source_id()
                        && anchor.full_range().contains_range(span.range())
                })
            })
            .cloned(),
    );

    diagnostics
}
