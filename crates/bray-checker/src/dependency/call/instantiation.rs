use bray_bound_tree::{
    BoundDependencyGuard, BoundDependencySubject, BoundExpressionId,
    DependencyContractInstantiationContext, SelectedArgument, SelectedCall, StorageAccessId,
    StorageIdentity, StoragePlan, StorageProjection,
};
use bray_symbols::{
    DependencyGuard, DependencyProjection, DependencyRequirementKind, DependencySubject,
    DependencySubjectRoot, SymbolOrdinal,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

enum CallInstantiationInput<'check> {
    Selected {
        expression: BoundExpressionId,
        call: &'check SelectedCall,
    },
    Hidden {
        receiver: StorageAccessId,
        result: StorageAccessId,
    },
}

pub(super) struct CallInstantiationContext<'check, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'check, C>,
    storage: &'check StoragePlan,
    input: CallInstantiationInput<'check>,
}

impl<'check, C> CallInstantiationContext<'check, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) const fn new(
        request: CheckerUnitView<'check, C>,
        storage: &'check StoragePlan,
        expression: BoundExpressionId,
        call: &'check SelectedCall,
    ) -> Self {
        Self {
            request,
            storage,
            input: CallInstantiationInput::Selected { expression, call },
        }
    }

    pub(super) const fn hidden(
        request: CheckerUnitView<'check, C>,
        storage: &'check StoragePlan,
        receiver: StorageAccessId,
        result: StorageAccessId,
    ) -> Self {
        Self {
            request,
            storage,
            input: CallInstantiationInput::Hidden { receiver, result },
        }
    }

    fn resolve_guard_access(
        &mut self,
        subject: &DependencySubject,
    ) -> Result<StorageAccessId, CheckerInfrastructureError> {
        match self.resolve_subject(subject, DependencyRequirementKind::StorageAlive)? {
            BoundDependencySubject::StorageAccess(access) => Ok(access),
            BoundDependencySubject::Storage(_)
            | BoundDependencySubject::BorrowCapability(_)
            | BoundDependencySubject::ScopedCapability(_)
            | BoundDependencySubject::ImplementationWitness(_)
            | BoundDependencySubject::LifecycleObligation(_) => {
                Err(CheckerInfrastructureError::InvalidSemanticSelectionInput)
            }
        }
    }

    fn selected_expression(&self, root: DependencySubjectRoot) -> Option<BoundExpressionId> {
        let CallInstantiationInput::Selected { expression, call } = self.input else {
            return None;
        };

        match root {
            DependencySubjectRoot::Parameter(ordinal) => {
                call.arguments().iter().find_map(|argument| match argument {
                    SelectedArgument::Explicit {
                        expression,
                        ordinal: actual,
                        ..
                    } if SymbolOrdinal::new(*actual) == ordinal => Some(*expression),
                    SelectedArgument::Explicit { .. } | SelectedArgument::Default { .. } => None,
                })
            }
            DependencySubjectRoot::Result => Some(expression),
            DependencySubjectRoot::Receiver => call
                .receiver()
                .map(bray_bound_tree::SelectedReceiver::expression),
            DependencySubjectRoot::ScopedCapability(_)
            | DependencySubjectRoot::ImplementationWitness(_) => None,
        }
    }

    fn hidden_access(&self, root: DependencySubjectRoot) -> Option<StorageAccessId> {
        let CallInstantiationInput::Hidden { receiver, result } = self.input else {
            return None;
        };

        match root {
            DependencySubjectRoot::Receiver => Some(receiver),
            DependencySubjectRoot::Result => Some(result),
            DependencySubjectRoot::Parameter(_)
            | DependencySubjectRoot::ScopedCapability(_)
            | DependencySubjectRoot::ImplementationWitness(_) => None,
        }
    }

    fn borrow_capability(
        &self,
        expression: Option<BoundExpressionId>,
        access: StorageAccessId,
        kind: bray_symbols::BorrowKind,
    ) -> Option<bray_bound_tree::BorrowCapabilityId> {
        self.storage
            .borrow_capability_entries()
            .find_map(|(id, capability)| {
                (capability.kind() == kind
                    && (capability.access() == access
                        || expression
                            .is_some_and(|expression| capability.expression() == Some(expression))))
                .then_some(id)
            })
    }
}

impl<C> DependencyContractInstantiationContext for CallInstantiationContext<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    type Error = CheckerInfrastructureError;

    fn unit(&self) -> bray_bound_tree::BoundUnitId {
        self.request.unit().unit()
    }

    fn resolve_subject(
        &mut self,
        subject: &DependencySubject,
        requirement: DependencyRequirementKind,
    ) -> Result<BoundDependencySubject, Self::Error> {
        if let DependencySubjectRoot::ImplementationWitness(witness) = subject.subject_root() {
            return Ok(BoundDependencySubject::ImplementationWitness(witness));
        }

        let root = subject.subject_root();
        let expression = self.selected_expression(root);

        let base = expression
            .and_then(|expression| expression_access(self.storage, expression))
            .or_else(|| self.hidden_access(root))
            .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

        if let DependencyRequirementKind::BorrowCapabilityActive(kind) = requirement {
            let capability = self
                .borrow_capability(expression, base, kind)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            return Ok(BoundDependencySubject::BorrowCapability(capability));
        }

        let access = projected_access(self.storage, base, subject.projections()).unwrap_or(base);

        Ok(BoundDependencySubject::StorageAccess(access))
    }

    fn resolve_guard(
        &mut self,
        guard: &DependencyGuard,
    ) -> Result<BoundDependencyGuard, Self::Error> {
        match guard {
            DependencyGuard::NullablePresent(subject) => {
                let access = self.resolve_guard_access(subject)?;

                Ok(BoundDependencyGuard::NullablePresent(access))
            }
            DependencyGuard::ActiveUnionVariant { subject, variant } => {
                let access = self.resolve_guard_access(subject)?;

                Ok(BoundDependencyGuard::ActiveUnionVariant {
                    access,
                    variant: *variant,
                })
            }
        }
    }
}

pub(super) fn expression_access(
    storage: &StoragePlan,
    expression: BoundExpressionId,
) -> Option<StorageAccessId> {
    storage
        .expression_plans(expression)
        .next()
        .map(bray_bound_tree::StorageAccessPlan::access)
        .or_else(|| identity_access(storage, StorageIdentity::Temporary(expression)))
}

pub(super) fn identity_access(
    storage: &StoragePlan,
    identity: StorageIdentity,
) -> Option<StorageAccessId> {
    let identity = storage
        .identity_entries()
        .find_map(|(id, candidate)| (candidate == identity).then_some(id))?;

    storage.access_entries().find_map(|(id, _)| {
        (storage.root_identity(id) == Some(identity)
            && storage
                .resolved_projections(id)
                .is_some_and(<[_]>::is_empty))
        .then_some(id)
    })
}

fn projected_access(
    storage: &StoragePlan,
    base: StorageAccessId,
    projections: &[DependencyProjection],
) -> Option<StorageAccessId> {
    let root = storage.root_identity(base)?;
    let base_projections = storage.resolved_projections(base)?;

    storage.access_entries().find_map(|(id, _)| {
        let candidate = storage.resolved_projections(id)?;

        (storage.root_identity(id) == Some(root)
            && candidate.len() == base_projections.len() + projections.len()
            && candidate.starts_with(base_projections)
            && projections
                .iter()
                .zip(&candidate[base_projections.len()..])
                .all(|(expected, actual)| projection_matches(*expected, *actual)))
        .then_some(id)
    })
}

fn projection_matches(expected: DependencyProjection, actual: StorageProjection) -> bool {
    match (expected, actual) {
        (DependencyProjection::ProductField(expected), StorageProjection::ProductField(actual)) => {
            expected == actual
        }
        (DependencyProjection::TupleElement(expected), StorageProjection::TupleElement(actual)) => {
            expected == actual
        }
        (
            DependencyProjection::UnionPayloadField(expected),
            StorageProjection::ActiveUnionPayloadField { field: actual, .. },
        ) => expected == actual,
        (DependencyProjection::NullableValue, StorageProjection::NullableValue)
        | (DependencyProjection::OwnedTarget, StorageProjection::OwnedTarget) => true,
        (DependencyProjection::Element(_), _)
        | (_, StorageProjection::ElementFromStart(_))
        | (_, StorageProjection::ElementFromEnd(_))
        | (_, StorageProjection::Element(_))
        | (_, StorageProjection::SliceRange { .. }) => false,
        _ => false,
    }
}
