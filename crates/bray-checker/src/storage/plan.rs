use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BorrowCapabilityOrigin, BoundBlockId, BoundBlockItem, BoundCallableBodyKind, BoundExpressionId,
    BoundPatternId, BoundReferenceTarget, CheckedExpressionTypes, CheckedPatternFacts,
    CheckedSemanticSelections, DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
    ExpressionTypeResult, PlannedBorrowCapability, SelectedIterationSource, SemanticSelection,
    StorageAccess, StorageAccessId, StorageAccessPurpose, StorageAccessRoot, StorageBinding,
    StorageBindingTarget, StorageIdentity, StoragePlan, StoragePlanBuilder,
};
use bray_symbols::{
    AnySymbolId, BorrowKind, CallableSignatureFact, CallableSymbolId, PredicateDefinitionSymbolId,
    ReceiverMode, SymbolFactRequest, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::{
    CheckerFactError, CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerUnitRoot, CheckerUnitView, SemanticUnitContext,
    resolve_type_expression_template,
};

#[derive(Clone, Copy)]
struct EntryStorage {
    ty: TypeId,
    borrow: Option<(BorrowKind, TypeId)>,
}

pub(crate) fn plan_storage<C>(
    request: CheckerUnitView<'_, C>,
    declared_types: &DeclaredValueTypeTemplates,
    types: &CheckedExpressionTypes,
    patterns: &CheckedPatternFacts,
    selections: &CheckedSemanticSelections,
) -> CheckerOutcome<StoragePlan>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let unit = request.unit().unit();
    let kind = request.unit().key().kind();

    if declared_types.unit() != unit
        || declared_types.kind() != kind
        || types.unit() != unit
        || types.kind() != kind
        || patterns.unit() != unit
        || patterns.kind() != kind
        || selections.unit() != unit
        || selections.kind() != kind
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidStoragePlan,
        );
    }

    let mut planner = match Planner::new(
        request,
        declared_types,
        types,
        patterns,
        selections,
        match receiver_entry(request) {
            Ok(receiver) => receiver,
            Err(CheckerFactError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(CheckerFactError::Infrastructure(error)) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
        },
    ) {
        Ok(planner) => planner,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    match planner.plan() {
        Ok(plan) => CheckerOutcome::without_diagnostics(plan),
        Err(PlanError::Cancelled) => CheckerOutcome::Cancelled,
        Err(PlanError::Infrastructure(error)) => CheckerOutcome::InfrastructureFailure(error),
    }
}

pub(super) struct Planner<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) request: CheckerUnitView<'view, C>,
    pub(super) declared_types: &'view DeclaredValueTypeTemplates,
    pub(super) types: &'view CheckedExpressionTypes,
    pub(super) patterns: &'view CheckedPatternFacts,
    pub(super) selections: &'view CheckedSemanticSelections,
    pub(super) iterations: BTreeMap<BoundExpressionId, &'view SelectedIterationSource>,
    pub(super) builder: Option<StoragePlanBuilder>,
    pub(super) expression_accesses: BTreeMap<BoundExpressionId, StorageAccessId>,
    pub(super) planned_blocks: BTreeSet<BoundBlockId>,
    pub(super) planned_patterns: BTreeSet<BoundPatternId>,
    pub(super) alternative_pattern_bindings:
        BTreeMap<bray_symbols::LocalBindingSymbolId, Vec<Vec<StorageAccessId>>>,
    pub(super) result_storage: bray_bound_tree::StorageIdentityId,
    pub(super) receiver_storage: Option<bray_bound_tree::StorageIdentityId>,
    receiver_entry: Option<(
        bray_symbols::ReceiverParameterSymbolId,
        Option<(BorrowKind, TypeId)>,
    )>,
}

pub(super) enum PlanError {
    Cancelled,
    Infrastructure(CheckerInfrastructureError),
}

impl From<CheckerInfrastructureError> for PlanError {
    fn from(error: CheckerInfrastructureError) -> Self {
        Self::Infrastructure(error)
    }
}

impl From<CheckerFactError> for PlanError {
    fn from(error: CheckerFactError) -> Self {
        match error {
            CheckerFactError::Cancelled => Self::Cancelled,
            CheckerFactError::Infrastructure(error) => Self::Infrastructure(error),
        }
    }
}

impl<C> Planner<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn new<'view>(
        request: CheckerUnitView<'view, C>,
        declared_types: &'view DeclaredValueTypeTemplates,
        types: &'view CheckedExpressionTypes,
        patterns: &'view CheckedPatternFacts,
        selections: &'view CheckedSemanticSelections,
        receiver_entry: Option<(
            bray_symbols::ReceiverParameterSymbolId,
            Option<(BorrowKind, TypeId)>,
        )>,
    ) -> Result<Planner<'view, C>, CheckerInfrastructureError> {
        let unit = request.unit().unit();
        let root = request.unit().root().into();
        let mut builder = StoragePlanBuilder::new(unit, request.unit().key().kind());

        let result_storage = builder
            .push_identity(StorageIdentity::Result(root))
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        builder
            .bind(
                StorageBindingTarget::Result,
                StorageBinding::Identity(result_storage),
            )
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        Ok(Planner {
            request,
            declared_types,
            types,
            patterns,
            selections,
            iterations: selections
                .entries()
                .iter()
                .filter_map(|entry| match entry.selection() {
                    SemanticSelection::Iteration(selection) => {
                        Some((selection.expression(), selection))
                    }
                    SemanticSelection::Reference(_)
                    | SemanticSelection::Call(_)
                    | SemanticSelection::Operation(_)
                    | SemanticSelection::Propagation(_) => None,
                })
                .collect(),
            builder: Some(builder),
            expression_accesses: BTreeMap::new(),
            planned_blocks: BTreeSet::new(),
            planned_patterns: BTreeSet::new(),
            alternative_pattern_bindings: BTreeMap::new(),
            result_storage,
            receiver_storage: None,
            receiver_entry,
        })
    }

    fn plan(&mut self) -> Result<StoragePlan, PlanError> {
        self.install_entry_storage()?;

        match self.request.root() {
            CheckerUnitRoot::CallableBody(id) => {
                let body = self
                    .request
                    .view()
                    .callable_body(id)
                    .ok_or_else(|| invalid_node(id))?;

                let block = match body.kind() {
                    BoundCallableBodyKind::Block(block) => Some(block),
                    BoundCallableBodyKind::Error(error) => error.body(),
                };

                if let Some(block) = block {
                    self.plan_block(block)?;
                }
            }
            CheckerUnitRoot::Expression(expression) => {
                self.plan_expression(expression, Some(StorageAccessPurpose::Read))?;
            }
            CheckerUnitRoot::ExpressionSequence(block) => self.plan_block(block)?,
        }

        self.install_recovered_local_storage()?;

        let Some(builder) = self.builder.take() else {
            return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
        };

        Ok(builder.finish())
    }

    fn install_entry_storage(&mut self) -> Result<(), PlanError> {
        match self.request.semantic_context() {
            SemanticUnitContext::AnonymousCallable(context) => {
                for parameter in context.parameters() {
                    let target = StorageBindingTarget::AnonymousParameter(*parameter);

                    let reference = BoundReferenceTarget::Local(
                        bray_symbols::AnyLocalSymbolId::from(*parameter),
                    );

                    self.bind_entry(
                        target,
                        StorageIdentity::AnonymousParameter(*parameter),
                        self.entry_storage(reference)?,
                    )?;
                }
            }
            SemanticUnitContext::CallableBody(context)
            | SemanticUnitContext::RuntimeDefault(context)
            | SemanticUnitContext::ConstantTemplate(context)
            | SemanticUnitContext::EmbeddedConstant(context)
            | SemanticUnitContext::PredicateDefinition(context)
            | SemanticUnitContext::Constraint(context)
            | SemanticUnitContext::TargetGate(context) => {
                self.install_declared_inputs(context.declaration())?;
            }
            SemanticUnitContext::ContractClause(context) => {
                self.install_declared_inputs(context.declaration().declaration())?;
            }
        }

        let results = self
            .request
            .unit()
            .local_symbols()
            .postcondition_results()
            .iter()
            .map(|result| result.id())
            .collect::<Vec<_>>();

        for result in results {
            let storage = self
                .builder_mut()?
                .push_identity(StorageIdentity::PostconditionResult(result))
                .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

            self.builder_mut()?
                .bind(
                    StorageBindingTarget::PostconditionResult(result),
                    StorageBinding::Identity(storage),
                )
                .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;
        }

        Ok(())
    }

    fn install_declared_inputs(&mut self, mut symbol: AnySymbolId) -> Result<(), PlanError> {
        loop {
            if let Some(callable) = CallableSymbolId::try_from_any(symbol) {
                let Some((parameters, receiver)) = self
                    .request
                    .symbols()
                    .callable_parameters_and_receiver(callable)
                else {
                    return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                };

                let parameters = parameters.to_vec();

                for parameter in parameters {
                    let target = StorageBindingTarget::Parameter(parameter);

                    self.bind_entry(
                        target,
                        StorageIdentity::Parameter(parameter),
                        self.entry_storage(BoundReferenceTarget::Surface(parameter.into()))?,
                    )?;
                }

                if let Some(receiver) = receiver {
                    let borrow = self
                        .receiver_entry
                        .filter(|(parameter, _)| parameter == &receiver)
                        .map(|(_, borrow)| borrow)
                        .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                    let entry = match borrow {
                        Some((kind, fallback_target)) => {
                            let target = self
                                .entry_storage(BoundReferenceTarget::Surface(receiver.into()))?
                                .map(|entry| match entry.borrow {
                                    Some((_, target)) => target,
                                    None => entry.ty,
                                })
                                .unwrap_or(fallback_target);

                            let ty = self
                                .request
                                .semantic_values()
                                .intern_type(TypeData::Borrow { kind, target })
                                .map_err(|_| {
                                    CheckerInfrastructureError::SemanticValueUnavailable
                                })?;

                            Some(EntryStorage {
                                ty,
                                borrow: Some((kind, target)),
                            })
                        }
                        None => {
                            self.entry_storage(BoundReferenceTarget::Surface(receiver.into()))?
                        }
                    };

                    self.bind_entry(
                        StorageBindingTarget::Receiver(receiver),
                        StorageIdentity::Receiver(receiver),
                        entry,
                    )?;
                }

                return Ok(());
            }

            if let Some(predicate) = PredicateDefinitionSymbolId::try_from_any(symbol) {
                let Some(parameters) = self
                    .request
                    .symbols()
                    .predicate_definition_parameters(predicate)
                else {
                    return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                };

                let parameters = parameters.to_vec();

                for parameter in parameters {
                    let target = StorageBindingTarget::PredicateParameter(parameter);

                    self.bind_entry(
                        target,
                        StorageIdentity::PredicateParameter(parameter),
                        self.entry_storage(BoundReferenceTarget::Surface(parameter.into()))?,
                    )?;
                }

                return Ok(());
            }

            let Some(owner) = self.request.symbols().containing_symbol(symbol) else {
                return Ok(());
            };

            symbol = owner;
        }
    }

    pub(super) fn bind_identity(
        &mut self,
        target: StorageBindingTarget,
        identity: StorageIdentity,
    ) -> Result<(), PlanError> {
        if self.builder()?.binding(target).is_some() {
            return Ok(());
        }

        let storage = self
            .builder_mut()?
            .push_identity(identity)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        if matches!(target, StorageBindingTarget::Receiver(_)) {
            self.receiver_storage = Some(storage);
        }

        self.builder_mut()?
            .bind(target, StorageBinding::Identity(storage))
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        Ok(())
    }

    fn bind_entry(
        &mut self,
        target: StorageBindingTarget,
        identity: StorageIdentity,
        entry: Option<EntryStorage>,
    ) -> Result<(), PlanError> {
        if self.builder()?.binding(target).is_some() {
            return Ok(());
        }

        let storage = self
            .builder_mut()?
            .push_identity(identity)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        if let Some(entry) = entry {
            self.builder_mut()?
                .set_identity_type(storage, entry.ty)
                .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;
        }

        let Some((kind, reached_type)) = entry.and_then(|entry| entry.borrow) else {
            self.builder_mut()?
                .bind(target, StorageBinding::Identity(storage))
                .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

            return Ok(());
        };

        let source = bray_bound_tree::BoundSourceAnchor::new(
            self.request.unit().key().source().syntax(),
            self.request.unit().key().source().source_version(),
        );

        let borrowed = StorageAccess::new(
            StorageAccessRoot::Storage(storage),
            [],
            reached_type,
            source,
            false,
        );

        let borrowed = self
            .builder_mut()?
            .push_access(borrowed)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        let capability = PlannedBorrowCapability::new(
            BorrowCapabilityOrigin::Entry(target),
            kind,
            borrowed,
            None,
            source,
            false,
        );

        let capability = self
            .builder_mut()?
            .push_borrow_capability(capability)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        let access = StorageAccess::new(
            StorageAccessRoot::Borrow(capability),
            [],
            reached_type,
            source,
            false,
        );

        let access = self
            .builder_mut()?
            .push_access(access)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        self.builder_mut()?
            .bind(target, StorageBinding::Access(access))
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan.into())
    }

    fn entry_storage(
        &self,
        target: BoundReferenceTarget,
    ) -> Result<Option<EntryStorage>, PlanError> {
        let Some(template) = self
            .declared_types
            .evidence()
            .iter()
            .find(|evidence| evidence.term() == DeclaredValueTypeTerm::Value(target))
            .map(|evidence| evidence.template())
        else {
            return Ok(None);
        };

        match template {
            TypeExpressionTemplate::Resolved(ty) => {
                let data = self
                    .request
                    .semantic_values()
                    .type_data(*ty)
                    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

                let borrow = match data.as_ref() {
                    TypeData::Borrow { kind, target } => Some((*kind, *target)),
                    _ => None,
                };

                Ok(Some(EntryStorage { ty: *ty, borrow }))
            }
            TypeExpressionTemplate::Borrow { kind, target } => {
                let terms = self.request.checked_constant_terms(target)?;

                let reached_type = match resolve_type_expression_template(
                    self.request.semantic_values(),
                    target,
                    terms.value(),
                )? {
                    Some(ty) => ty,
                    None => self
                        .request
                        .semantic_values()
                        .intern_type(TypeData::Error)
                        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?,
                };

                let ty = self
                    .request
                    .semantic_values()
                    .intern_type(TypeData::Borrow {
                        kind: *kind,
                        target: reached_type,
                    })
                    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

                Ok(Some(EntryStorage {
                    ty,
                    borrow: Some((*kind, reached_type)),
                }))
            }
            TypeExpressionTemplate::Named { .. }
            | TypeExpressionTemplate::TypeValuedMemberProjection { .. }
            | TypeExpressionTemplate::Tuple(_)
            | TypeExpressionTemplate::Array { .. }
            | TypeExpressionTemplate::Slice(_)
            | TypeExpressionTemplate::Nullable(_)
            | TypeExpressionTemplate::TraitView(_)
            | TypeExpressionTemplate::OwnedIndirection { .. }
            | TypeExpressionTemplate::Callable(_) => Ok(None),
        }
    }

    fn install_recovered_local_storage(&mut self) -> Result<(), PlanError> {
        let root = self.request.unit().root().into();

        let bindings = self
            .request
            .unit()
            .local_symbols()
            .bindings()
            .iter()
            .map(|binding| binding.id())
            .collect::<Vec<_>>();

        for binding in bindings {
            let target = StorageBindingTarget::Local(binding);

            if self.builder()?.binding(target).is_none() {
                self.bind_identity(target, StorageIdentity::LocalOwned(root))?;
            }
        }

        Ok(())
    }

    pub(super) fn plan_block(&mut self, id: BoundBlockId) -> Result<(), PlanError> {
        self.check_cancellation()?;

        if !self.planned_blocks.insert(id) {
            return Ok(());
        }

        // The immutable block is cloned so recursive planning can mutably advance task-local
        // state.
        let block = self
            .request
            .view()
            .block(id)
            .ok_or_else(|| invalid_node(id))?
            .clone();

        for item in block.items() {
            match item {
                BoundBlockItem::LocalBinding(binding) => {
                    let subject = self.plan_expression(binding.initializer(), None)?;

                    self.plan_pattern(binding.pattern(), binding.initializer(), subject)?;
                }
                BoundBlockItem::LocalConstant(constant) => {
                    self.plan_expression(constant.initializer(), Some(StorageAccessPurpose::Read))?;
                }
                BoundBlockItem::Expression(expression) => {
                    self.plan_expression(*expression, Some(StorageAccessPurpose::Read))?;
                }
            }
        }

        Ok(())
    }

    pub(super) fn expression_type(
        &self,
        expression: BoundExpressionId,
    ) -> Result<ExpressionTypeResult, PlanError> {
        self.types
            .expression(expression)
            .ok_or_else(|| CheckerInfrastructureError::InvalidStoragePlan.into())
    }

    pub(super) fn record_purpose(
        &mut self,
        expression: BoundExpressionId,
        purpose: Option<StorageAccessPurpose>,
        access: StorageAccessId,
    ) -> Result<(), PlanError> {
        let Some(purpose) = purpose else {
            return Ok(());
        };

        self.builder_mut()?
            .plan_access(expression, purpose, access)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan.into())
    }

    pub(super) fn builder(&self) -> Result<&StoragePlanBuilder, PlanError> {
        self.builder
            .as_ref()
            .ok_or_else(|| CheckerInfrastructureError::InvalidStoragePlan.into())
    }

    pub(super) fn builder_mut(&mut self) -> Result<&mut StoragePlanBuilder, PlanError> {
        self.builder
            .as_mut()
            .ok_or_else(|| CheckerInfrastructureError::InvalidStoragePlan.into())
    }

    pub(super) fn check_cancellation(&self) -> Result<(), PlanError> {
        if self.request.is_cancelled() {
            Err(PlanError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[expect(
    clippy::type_complexity,
    reason = "the tuple directly represents the receiver identity and optional borrow capability"
)]
fn receiver_entry<C>(
    request: CheckerUnitView<'_, C>,
) -> Result<
    Option<(
        bray_symbols::ReceiverParameterSymbolId,
        Option<(BorrowKind, TypeId)>,
    )>,
    CheckerFactError,
>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    let Some(callable) = request.containing_callable() else {
        return Ok(None);
    };

    let signature =
        request.symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))?;

    Ok(signature.value().receiver().map(|receiver| {
        let borrow = match receiver.mode() {
            ReceiverMode::Shared => Some((BorrowKind::Shared, receiver.ty())),
            ReceiverMode::Mutable => Some((BorrowKind::Mutable, receiver.ty())),
            ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => None,
        };

        (receiver.parameter(), borrow)
    }))
}

pub(super) fn invalid_node(id: impl Into<bray_bound_tree::AnyBoundNodeId>) -> PlanError {
    CheckerInfrastructureError::InvalidBoundNode { node: id.into() }.into()
}

pub(super) const fn iteration_purpose(
    mode: bray_bound_tree::IterationSourceMode,
) -> StorageAccessPurpose {
    match mode {
        bray_bound_tree::IterationSourceMode::Shared => {
            StorageAccessPurpose::Borrow(BorrowKind::Shared)
        }
        bray_bound_tree::IterationSourceMode::Mutable => {
            StorageAccessPurpose::Borrow(BorrowKind::Mutable)
        }
        bray_bound_tree::IterationSourceMode::Move => StorageAccessPurpose::Move,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::StoragePlan;

    #[test]
    fn storage_plans_are_shareable() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<StoragePlan>();
    }
}
