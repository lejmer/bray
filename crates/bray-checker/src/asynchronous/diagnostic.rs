use bray_bound_tree::{
    BoundDependencyContractId, BoundDependencySubject, BoundExpressionId, BoundNodeOrigin,
    BoundSourceAnchor, CheckedDependencyContracts, CheckedRefinements, StorageIdentity,
    StoragePlan,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
    SeverityKind,
};

use super::dependency::{UnsatisfiedDependency, unsatisfied_dependency_subjects};
use crate::diagnostic::{bound_node_origin, diagnostic_id, expression_span};
use crate::{CheckerRequestContext, CheckerUnitView};

#[expect(
    clippy::too_many_arguments,
    reason = "await dependency validation consumes the exact independently checked inputs"
)]
pub(super) fn check_await_dependencies<C>(
    request: CheckerUnitView<'_, C>,
    dependencies: &CheckedDependencyContracts,
    storage: &StoragePlan,
    refinements: &CheckedRefinements,
    expression: BoundExpressionId,
    dependency_contract: Option<BoundDependencyContractId>,
    suspension_state: &bray_bound_tree::StorageSuspensionState,
    syntax_recovered: bool,
    diagnostics: &mut DiagnosticBag,
) where
    C: CheckerRequestContext + ?Sized,
{
    if syntax_recovered {
        return;
    }

    let Some(dependency_contract) = dependency_contract else {
        panic!(
            "A non-recovered await expression has no selected dependency contract. in check_await_dependencies, expression: {:?}",
            expression
        );
    };

    let Some(contract) = dependencies.contract(dependency_contract) else {
        panic!(
            "An await expression names a dependency contract absent from its source body. in check_await_dependencies, expression: {:?}, contract: {:?}",
            expression, dependency_contract
        );
    };

    let unsatisfied = unsatisfied_dependency_subjects(
        request.semantic_values(),
        storage,
        refinements,
        expression,
        suspension_state,
        contract,
    ).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in check_await_dependencies: {error:?}"));

    if !unsatisfied.is_empty() {
        add_unavailable_await_dependency_diagnostic(
            request,
            storage,
            expression,
            &unsatisfied,
            diagnostics,
        );
    }
}

fn add_unavailable_await_dependency_diagnostic<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    expression: BoundExpressionId,
    dependencies: &[UnsatisfiedDependency],
    diagnostics: &mut DiagnosticBag,
) where
    C: CheckerRequestContext + ?Sized,
{
    let span = expression_span(request, expression);

    let mut diagnostic = Diagnostic::new(
        diagnostic_id(diagnostics.len()),
        DiagnosticKind::CheckingUnavailableAwaitDependency,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::UnavailableAwaitDependency,
        span,
    ));

    for (index, dependency) in dependencies.iter().enumerate() {
        let subject = DiagnosticArg::dependency_subject_kind(diagnostic_dependency_subject(
            dependency.subject,
        ));

        let requirement = DiagnosticArg::dependency_requirement_kind(
            diagnostic_dependency_requirement(dependency.requirement),
        );

        if index == 0 {
            diagnostic = diagnostic
                .with_arg(subject.clone())
                .with_arg(requirement.clone());
        }

        diagnostic = diagnostic.with_note(
            DiagnosticNote::new(DiagnosticNoteKind::AwaitDependencyUnavailable)
                .with_arg(subject)
                .with_arg(requirement),
        );

        if let Some(origin) = await_dependency_origin(request, storage, dependency.subject) {
            if origin == span {
                continue;
            }

            diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::RequirementOrigin,
                origin,
            ));
        }
    }

    diagnostics.add(diagnostic);
}

fn await_dependency_origin<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    subject: BoundDependencySubject,
) -> Option<bray_source::SourceSpan>
where
    C: CheckerRequestContext + ?Sized,
{
    let source = match subject {
        BoundDependencySubject::Storage(identity) => storage
            .identity(identity)
            .and_then(|identity| storage_identity_source(request, identity)),
        BoundDependencySubject::StorageAccess(access) => storage
            .access(access)
            .map(bray_bound_tree::StorageAccess::source),
        BoundDependencySubject::BorrowCapability(capability) => storage
            .borrow_capability(capability)
            .map(bray_bound_tree::PlannedBorrowCapability::source),
        BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::ProductStatic(_)
        | BoundDependencySubject::ExactThreadStatic(_)
        | BoundDependencySubject::LifecycleObligation(_) => None,
    };

    source.map(|source| request.source(source).span())
}

fn storage_identity_source<C>(
    request: CheckerUnitView<'_, C>,
    identity: StorageIdentity,
) -> Option<BoundSourceAnchor>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(node) = identity.definition_node() {
        return bound_node_origin(request, node).map(BoundNodeOrigin::source_anchor);
    }

    match identity {
        StorageIdentity::CompilerCreated(origin) => Some(origin.source_anchor()),
        StorageIdentity::Error(source) => Some(source),
        StorageIdentity::LocalOwned(_)
        | StorageIdentity::Parameter(_)
        | StorageIdentity::ContractParameter(_)
        | StorageIdentity::Receiver(_)
        | StorageIdentity::Static(_)
        | StorageIdentity::AnonymousParameter(_)
        | StorageIdentity::PredicateParameter(_)
        | StorageIdentity::PostconditionResult(_)
        | StorageIdentity::Result(_)
        | StorageIdentity::Temporary(_)
        | StorageIdentity::CustomIndexBorrow(_)
        | StorageIdentity::IterationCursor(_)
        | StorageIdentity::IterationElement(_)
        | StorageIdentity::ScopedCapability { .. }
        | StorageIdentity::Allocation(_)
        | StorageIdentity::Alternative { .. } => None,
    }
}

const fn diagnostic_dependency_subject(
    subject: BoundDependencySubject,
) -> bray_diagnostics::DiagnosticDependencySubjectKind {
    use bray_diagnostics::DiagnosticDependencySubjectKind;

    match subject {
        BoundDependencySubject::Storage(_) => DiagnosticDependencySubjectKind::Storage,
        BoundDependencySubject::StorageAccess(_) => DiagnosticDependencySubjectKind::StorageAccess,
        BoundDependencySubject::BorrowCapability(_) => {
            DiagnosticDependencySubjectKind::BorrowCapability
        }
        BoundDependencySubject::ScopedCapability(_) => {
            DiagnosticDependencySubjectKind::ScopedCapability
        }
        BoundDependencySubject::ImplementationWitness(_) => {
            DiagnosticDependencySubjectKind::SelectedImplementation
        }
        BoundDependencySubject::ProductStatic(_) => DiagnosticDependencySubjectKind::ProductStatic,
        BoundDependencySubject::ExactThreadStatic(_) => {
            DiagnosticDependencySubjectKind::ExactThreadStatic
        }
        BoundDependencySubject::LifecycleObligation(_) => {
            DiagnosticDependencySubjectKind::LifecycleObligation
        }
    }
}

const fn diagnostic_dependency_requirement(
    requirement: bray_bound_tree::BoundDependencyRequirementKind,
) -> bray_diagnostics::DiagnosticDependencyRequirementKind {
    use bray_bound_tree::BoundDependencyRequirementKind;
    use bray_diagnostics::DiagnosticDependencyRequirementKind;
    use bray_symbols::{BorrowKind, LifecycleObligationKind};

    match requirement {
        BoundDependencyRequirementKind::ValueDependencies => {
            DiagnosticDependencyRequirementKind::ValueDependencies
        }
        BoundDependencyRequirementKind::StorageAlive => {
            DiagnosticDependencyRequirementKind::StorageAlive
        }
        BoundDependencyRequirementKind::StorageInitialized => {
            DiagnosticDependencyRequirementKind::StorageInitialized
        }
        BoundDependencyRequirementKind::BorrowCapabilityActive(BorrowKind::Shared) => {
            DiagnosticDependencyRequirementKind::SharedBorrowActive
        }
        BoundDependencyRequirementKind::BorrowCapabilityActive(BorrowKind::Mutable) => {
            DiagnosticDependencyRequirementKind::MutableBorrowActive
        }
        BoundDependencyRequirementKind::ExclusiveMutationAuthority => {
            DiagnosticDependencyRequirementKind::ExclusiveMutationAuthority
        }
        BoundDependencyRequirementKind::ScopedCapabilityLive => {
            DiagnosticDependencyRequirementKind::ScopedCapabilityLive
        }
        BoundDependencyRequirementKind::LifecycleObligationAttached(
            LifecycleObligationKind::Destruction,
        ) => DiagnosticDependencyRequirementKind::DestructionAttached,
        BoundDependencyRequirementKind::LifecycleObligationAttached(
            LifecycleObligationKind::Finalization,
        ) => DiagnosticDependencyRequirementKind::FinalizationAttached,
        BoundDependencyRequirementKind::LifecycleObligationAttached(
            LifecycleObligationKind::Cancellation,
        ) => DiagnosticDependencyRequirementKind::CancellationAttached,
        BoundDependencyRequirementKind::LifecycleObligationAttached(
            LifecycleObligationKind::Joining,
        ) => DiagnosticDependencyRequirementKind::JoiningAttached,
    }
}
