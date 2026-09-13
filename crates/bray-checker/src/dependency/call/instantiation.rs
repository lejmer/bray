use bray_bound_tree::{
    BoundDependencyGuard, BoundDependencySubject, BoundExpressionId,
    DependencyContractInstantiationContext, SelectedArgument, SelectedCall, StorageAccessId,
    StorageIdentity, StoragePlan, StorageProjection,
};
use bray_symbols::{
    DependencyGuard, DependencyProjection, DependencyRequirementKind, DependencySubject,
    DependencySubjectRoot, ReceiverMode, SymbolOrdinal, TypeData,
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
    deferred: bool,
    result_values: Option<&'check crate::dependency::ValueInputs>,
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
            deferred: false,
            result_values: None,
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
            deferred: false,
            result_values: None,
        }
    }

    pub(super) fn set_result_values(&mut self, values: &'check crate::dependency::ValueInputs) {
        self.result_values = Some(values);
    }

    pub(super) const fn begin_deferred_execution(&mut self) {
        self.deferred = true;
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
            | BoundDependencySubject::ProductStatic(_)
            | BoundDependencySubject::ExactThreadStatic(_)
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
            DependencySubjectRoot::Result => Some(expression),
            _ => crate::dependency::result_argument(call, root),
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
            | DependencySubjectRoot::ImplementationWitness(_)
            | DependencySubjectRoot::ProductStatic(_)
            | DependencySubjectRoot::ExactThreadStatic(_) => None,
        }
    }

    fn deferred_frame_access(
        &self,
        root: DependencySubjectRoot,
    ) -> Result<Option<StorageAccessId>, CheckerInfrastructureError> {
        if !self.deferred {
            return Ok(None);
        }

        let CallInstantiationInput::Selected { expression, call } = self.input else {
            return Ok(None);
        };

        let transferred = match root {
            DependencySubjectRoot::Receiver => call.receiver().is_some_and(|receiver| {
                matches!(
                    receiver.mode(),
                    ReceiverMode::Consuming | ReceiverMode::ConsumingMutable
                )
            }),
            DependencySubjectRoot::Parameter(ordinal) => {
                let argument = call.arguments().iter().find(|argument| match argument {
                    SelectedArgument::Explicit {
                        ordinal: actual, ..
                    }
                    | SelectedArgument::Default {
                        ordinal: actual, ..
                    } => SymbolOrdinal::new(*actual) == ordinal,
                });

                match argument {
                    Some(SelectedArgument::Explicit { conversion, .. }) => !matches!(
                        self.request
                            .semantic_values()
                            .type_data(conversion.target_type())
                            .map_err(CheckerInfrastructureError::SemanticValueStore)?
                            .as_ref(),
                        TypeData::Borrow { .. }
                    ),
                    Some(SelectedArgument::Default { .. }) => true,
                    None => false,
                }
            }
            DependencySubjectRoot::Result
            | DependencySubjectRoot::ScopedCapability(_)
            | DependencySubjectRoot::ImplementationWitness(_)
            | DependencySubjectRoot::ProductStatic(_)
            | DependencySubjectRoot::ExactThreadStatic(_) => false,
        };

        Ok(transferred
            .then(|| expression_access(self.storage, expression))
            .flatten())
    }

    fn borrow_capability(
        &self,
        expression: Option<BoundExpressionId>,
        access: StorageAccessId,
        kind: bray_symbols::BorrowKind,
    ) -> Option<bray_bound_tree::BorrowCapabilityId> {
        if let Some((id, _)) = self
            .storage
            .borrow_capability_entries()
            .find(|(_, capability)| {
                capability.kind() == kind
                    && expression
                        .is_some_and(|expression| capability.expression() == Some(expression))
            })
        {
            return Some(id);
        }

        if let Some(capability) = self
            .storage
            .access(access)
            .and_then(|access| access.root().borrow_capability())
            && self
                .storage
                .borrow_capability(capability)
                .is_some_and(|capability| capability.kind() == kind)
        {
            return Some(capability);
        }

        self.storage
            .borrow_capability_entries()
            .find_map(|(id, capability)| {
                (capability.kind() == kind
                    && (self.storage.relationship(capability.access(), access)
                        == bray_bound_tree::StorageRelationship::Identical
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

        match subject.subject_root() {
            DependencySubjectRoot::ProductStatic(id) => {
                return Ok(BoundDependencySubject::ProductStatic(id));
            }
            DependencySubjectRoot::ExactThreadStatic(id) => {
                return Ok(BoundDependencySubject::ExactThreadStatic(id));
            }
            _ => {}
        }

        let root = subject.subject_root();
        let mut expression = self.selected_expression(root);
        let mut projections = subject.projections();

        if requirement == DependencyRequirementKind::ValueDependencies
            && let Some((values, source)) = self.result_values.zip(expression)
        {
            let (projected, remaining) = values.project(source, projections);

            expression = Some(projected);
            projections = remaining;
        }

        if requirement == DependencyRequirementKind::ValueDependencies
            && projections.is_empty()
            && let Some((capability, _)) =
                self.storage
                    .borrow_capability_entries()
                    .find(|(_, capability)| {
                        expression
                            .is_some_and(|expression| capability.expression() == Some(expression))
                    })
        {
            return Ok(BoundDependencySubject::BorrowCapability(capability));
        }

        let frame_access = self.deferred_frame_access(root)?;

        let base = frame_access
            .or_else(|| {
                expression.and_then(|expression| expression_access(self.storage, expression))
            })
            .or_else(|| self.hidden_access(root))
            .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

        if let DependencyRequirementKind::BorrowCapabilityActive(kind) = requirement {
            let capability = self
                .borrow_capability(expression, base, kind)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            return Ok(BoundDependencySubject::BorrowCapability(capability));
        }

        let access = if frame_access.is_some() {
            base
        } else {
            projected_access(self.storage, base, projections).unwrap_or(base)
        };

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

    storage.root_access(identity)
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
