use bray_bound_tree::{
    BoundDependencyGuard, BoundDependencySubject, BoundExpressionId,
    DependencyContractInstantiationContext, SelectedArgument, SelectedCall, StorageAccessId,
    StorageIdentity, StoragePlan,
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
        expression: Option<BoundExpressionId>,
    },
    Inputs {
        inputs: &'check [(
            DependencySubjectRoot,
            StorageAccessId,
            Option<BoundExpressionId>,
        )],
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
        expression: Option<BoundExpressionId>,
    ) -> Self {
        Self {
            request,
            storage,
            input: CallInstantiationInput::Hidden {
                input,
                result,
                expression,
            },
            deferred: false,
            result_values: None,
        }
    }

    pub(super) fn hidden_inputs(
        request: CheckerUnitView<'check, C>,
        storage: &'check StoragePlan,
        inputs: &'check [(
            DependencySubjectRoot,
            StorageAccessId,
            Option<BoundExpressionId>,
        )],
        result: Option<StorageAccessId>,
    ) -> Self {
        Self {
            request,
            storage,
            input: CallInstantiationInput::Inputs { inputs, result },
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
    ) -> CheckerQueryResult<StorageAccessId, C::UpstreamError> {
        match self.resolve_subject(subject, DependencyRequirementKind::StorageAlive)? {
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
        let (expression, call) = match &self.input {
            CallInstantiationInput::Selected { expression, call } => (*expression, *call),
            CallInstantiationInput::Hidden {
                input, expression, ..
            } => {
                return (root == input.0).then_some(*expression).flatten();
            }
            CallInstantiationInput::Inputs { inputs, .. } => {
                return inputs.iter().find_map(|(actual, _, expression)| {
                    (*actual == root).then_some(*expression).flatten()
                });
            }
        };

        match root {
            DependencySubjectRoot::Result => Some(expression),
            _ if call.evaluates_defaults() => None,
            _ => crate::dependency::result_argument(call.arguments(), call.receiver(), root),
        }
    }

    fn input_borrow(
        &self,
        root: DependencySubjectRoot,
    ) -> Option<bray_bound_tree::BorrowCapabilityId> {
        let (expression, kind) = match &self.input {
            CallInstantiationInput::Inputs { inputs, .. } => {
                return inputs.iter().find_map(|(actual, access, _)| {
                    (*actual == root)
                        .then(|| self.storage.access(*access)?.root().borrow_capability())
                        .flatten()
                });
            }
            CallInstantiationInput::Selected { call, .. }
                if call.evaluates_defaults() => return None,
            CallInstantiationInput::Selected { call, .. } => match root {
                DependencySubjectRoot::Receiver => {
                    let receiver = call.receiver()?;

                    let kind = match receiver.mode() {
                        ReceiverMode::Shared => bray_symbols::BorrowKind::Shared,
                        ReceiverMode::Mutable => bray_symbols::BorrowKind::Mutable,
                        ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => return None,
                    };

                    (receiver.expression(), kind)
                }
                DependencySubjectRoot::Parameter(ordinal) => {
                    call.arguments()
                        .iter()
                        .find_map(|argument| match argument {
                            SelectedArgument::Explicit {
                                expression,
                                ordinal: actual,
                                reborrow: Some(kind),
                                ..
                            } if *actual == ordinal.raw() => Some((*expression, *kind)),
                            _ => None,
                        })?
                }
                _ => return None,
            },
            CallInstantiationInput::Hidden { .. } => return None,
        };

        self.storage
            .node_plans((expression).into())
            .find(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Borrow(kind))
            .and_then(|plan| self.storage.borrow_for_plan(plan))
    }

    fn receiver_access(&self, root: DependencySubjectRoot) -> Option<StorageAccessId> {
        if root != DependencySubjectRoot::Receiver {
            return None;
        }

        let CallInstantiationInput::Selected { call, .. } = &self.input else {
            return None;
        };

        let receiver = call.receiver()?;

        let kind = match receiver.mode() {
            ReceiverMode::Shared => bray_symbols::BorrowKind::Shared,
            ReceiverMode::Mutable => bray_symbols::BorrowKind::Mutable,
            _ => return None,
        };

        self.storage
            .node_plans(receiver.expression().into())
            .find(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Borrow(kind))
            .map(bray_bound_tree::StorageAccessPlan::access)
    }

    fn hidden_access(&self, root: DependencySubjectRoot) -> Option<StorageAccessId> {
        match &self.input {
            CallInstantiationInput::Hidden { input, result, .. } => {
                if root == input.0 {
                    Some(input.1)
                } else if root == DependencySubjectRoot::Result {
                    *result
                } else {
                    None
                }
            }
            CallInstantiationInput::Inputs { inputs, result } => {
                if root == DependencySubjectRoot::Result {
                    *result
                } else {
                    inputs
                        .iter()
                        .find_map(|(actual, access, _)| (*actual == root).then_some(*access))
                }
            }
            CallInstantiationInput::Selected { expression, call }
                if call.evaluates_defaults() =>
            {
                call.input_ordinal(root).map(|ordinal| {
                    self.storage.invocation_input(*expression, ordinal)
                        .expect("selected formal input has its staged storage")
                })
            }
            CallInstantiationInput::Selected { .. } => None,
        }
    }

    fn deferred_frame_access(&self, root: DependencySubjectRoot) -> Option<StorageAccessId> {
        if !self.deferred {
            return None;
        }

        let CallInstantiationInput::Selected { expression, call } = &self.input else {
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
            .then(|| expression_access(self.storage, *expression))
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
                        .is_some_and(|expression| capability.occurrence() == Some(expression.into()))
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
                            .is_some_and(|expression| capability.occurrence() == Some(expression.into()))))
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
            && subject.projections().is_empty()
            && let Some(capability) = self.input_borrow(root)
        {
            return Ok(BoundDependencySubject::BorrowCapability(capability));
        }

        if requirement == DependencyRequirementKind::ValueDependencies
            && projections.is_empty()
            && let Some((capability, _)) =
                self.storage
                    .borrow_capability_entries()
                    .find(|(_, capability)| {
                        expression
                            .is_some_and(|expression| capability.occurrence() == Some(expression.into()))
                    })
        {
            return Ok(BoundDependencySubject::BorrowCapability(capability));
        }

        let staged = match &self.input {
            CallInstantiationInput::Selected { call, .. }
                if call.evaluates_defaults() =>
            {
                self.hidden_access(root)
            }
            CallInstantiationInput::Inputs { .. } => self.hidden_access(root).filter(|access|
                matches!(self.storage.root_identity(*access).and_then(|identity| self.storage.identity(identity)),
                    Some(StorageIdentity::InvocationInput { .. }))),
            _ => None,
        };

        // A staged borrow is a checked value, not the original source loan.
        // Retaining its dependencies follows the loans of its current contents.
        if staged.is_some() && matches!(requirement,
            DependencyRequirementKind::BorrowCapabilityActive(_))
        {
            return Ok(BoundDependencySubject::StorageAccess(staged.expect("staged input exists")));
        }

        let frame_access = self.deferred_frame_access(root);

        let base = frame_access.or(staged)
            .or_else(|| (requirement != DependencyRequirementKind::ValueDependencies)
                .then(|| self.receiver_access(root).or_else(|| self.hidden_access(root))).flatten())
            .or_else(|| {
                expression.and_then(|expression| expression_access(self.storage, expression))
            })
            .or_else(|| self.hidden_access(root)).unwrap_or_else(|| panic!("resolve_subject requires dependency storage access, root: {root:?}, expression: {expression:?}, requirement: {requirement:?}"));

        if let DependencyRequirementKind::BorrowCapabilityActive(kind) = requirement {
            let capability = self
                .borrow_capability(expression, base, kind).unwrap_or_else(|| panic!("resolve_subject requires planned borrow capability, subject: {subject:?}, expression: {expression:?}, requirement: {requirement:?}"));

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
        .node_plans((expression).into())
        .find(|plan| !matches!(storage.root_identity(plan.access()).and_then(|root| storage.identity(root)),
            Some(StorageIdentity::InvocationInput { .. })))
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
    let Some(projected) = crate::storage::resolve_storage_projections(request, projections)? else {
        return Ok(None);
    };

    Ok(storage.projected_access(base, &projected))
}
