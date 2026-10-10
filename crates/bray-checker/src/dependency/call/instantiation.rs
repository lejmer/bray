use bray_bound_tree::{
    BoundDependencyGuard, BoundDependencySubject, BoundExpressionId,
    DependencyContractInstantiationContext, SelectedArgument, SelectedCall, StorageAccessId,
    StorageIdentity, StoragePlan, StorageProjection,
};
use bray_symbols::{
    DependencyGuard, DependencyProjection, DependencyRequirementKind, DependencySubject,
    DependencySubjectRoot, ReceiverMode, SymbolOrdinal,
};

use crate::{CheckerQueryError, CheckerQueryResult, CheckerRequestContext, CheckerUnitView};

enum CallInstantiationInput<'check> {
    Selected {
        expression: BoundExpressionId,
        call: &'check SelectedCall,
    },
    Hidden {
        input: (DependencySubjectRoot, StorageAccessId),
        result: Option<StorageAccessId>,
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
        input: (DependencySubjectRoot, StorageAccessId),
        result: Option<StorageAccessId>,
    ) -> Self {
        Self {
            request,
            storage,
            input: CallInstantiationInput::Hidden { input, result },
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

    fn resolve_guard_access(&mut self, subject: &DependencySubject) -> CheckerQueryResult<StorageAccessId, C::UpstreamError> {
        match self
            .resolve_subject(subject, DependencyRequirementKind::StorageAlive)?
        {
            BoundDependencySubject::StorageAccess(access) => Ok(access),
            BoundDependencySubject::Storage(_)
            | BoundDependencySubject::BorrowCapability(_)
            | BoundDependencySubject::ScopedCapability(_)
            | BoundDependencySubject::ImplementationWitness(_)
            | BoundDependencySubject::ProductStatic(_)
            | BoundDependencySubject::ExactThreadStatic(_)
            | BoundDependencySubject::LifecycleObligation(_) => {
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in resolve_guard_access"
                )
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
        let CallInstantiationInput::Hidden { input, result } = self.input else {
            return None;
        };

        if root == input.0 {
            Some(input.1)
        } else if root == DependencySubjectRoot::Result {
            result
        } else {
            None
        }
    }

    fn deferred_frame_access(&self, root: DependencySubjectRoot) -> Option<StorageAccessId> {
        if !self.deferred {
            return None;
        }

        let CallInstantiationInput::Selected { expression, call } = self.input else {
            return None;
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
                    Some(SelectedArgument::Explicit { reborrow, .. }) => reborrow.is_none(),
                    Some(SelectedArgument::Default { .. }) => true,
                    None => false,
                }
            }
            DependencySubjectRoot::Result
            | DependencySubjectRoot::EvaluationStorage
            | DependencySubjectRoot::ScopedCapability(_)
            | DependencySubjectRoot::ImplementationWitness(_)
            | DependencySubjectRoot::ProductStatic(_)
            | DependencySubjectRoot::ExactThreadStatic(_) => false,
        };

        transferred
            .then(|| expression_access(self.storage, expression))
            .flatten()
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
    type Error = CheckerQueryError<C::UpstreamError>;

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

        let frame_access = self.deferred_frame_access(root);

        let base = frame_access
            .or_else(|| {
                expression.and_then(|expression| expression_access(self.storage, expression))
            })
            .or_else(|| self.hidden_access(root)).unwrap_or_else(|| panic!("resolve_subject requires dependency storage access, root: {root:?}, expression: {expression:?}, requirement: {requirement:?}"));

        if let DependencyRequirementKind::BorrowCapabilityActive(kind) = requirement {
            let capability = self
                .borrow_capability(expression, base, kind).unwrap_or_else(|| panic!("resolve_subject requires planned borrow capability, root: {root:?}, expression: {expression:?}, requirement: {requirement:?}"));

            return Ok(BoundDependencySubject::BorrowCapability(capability));
        }

        let access = if frame_access.is_some() {
            base
        } else {
            projected_access(self.request, self.storage, base, projections)?.unwrap_or(base)
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

fn projected_access<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    base: StorageAccessId,
    projections: &[DependencyProjection],
) -> CheckerQueryResult<Option<StorageAccessId>, C::UpstreamError> {
    let mut projected = Vec::with_capacity(projections.len());

    for projection in projections {
        projected.push(match *projection {
            DependencyProjection::ProductField(field) => StorageProjection::ProductField(field),
            DependencyProjection::TupleElement(index) => StorageProjection::TupleElement(index),
            DependencyProjection::UnionPayloadField(field) => {
                let variant = request.union_payload_field(field)?
                    .unwrap_or_else(|| panic!("dependency projection requires union payload field {field:?}, base: {base:?}, unit: {:?}", request.unit().unit()))
                    .variant();

                StorageProjection::ActiveUnionPayloadField { variant, field }
            }
            DependencyProjection::NullableValue => StorageProjection::NullableValue,
            DependencyProjection::OwnedTarget => StorageProjection::OwnedTarget,
            DependencyProjection::Element(_) => return Ok(None),
        });
    }

    Ok(storage.projected_access(base, &projected))
}
