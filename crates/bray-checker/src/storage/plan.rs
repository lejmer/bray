use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundCallableBodyKind, BoundExpressionId, BoundPatternId,
    CheckedExpressionTypes, CheckedPatternFacts, CheckedSemanticSelections, ExpressionTypeResult,
    SelectedIterationSource, StorageAccessId, StorageAccessPurpose, StorageBinding,
    StorageBindingTarget, StorageIdentity, StoragePlan, StoragePlanBuilder,
};
use bray_symbols::{AnySymbolId, CallableSymbolId, PredicateDefinitionSymbolId};

use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitRoot,
    CheckerUnitView, SemanticUnitContext,
};

pub(crate) fn plan_storage<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    patterns: &CheckedPatternFacts,
    selections: &CheckedSemanticSelections,
    iterations: &[SelectedIterationSource],
) -> CheckerOutcome<StoragePlan>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let unit = request.unit().unit();
    let kind = request.unit().key().kind();

    if types.unit() != unit
        || types.kind() != kind
        || patterns.unit() != unit
        || patterns.kind() != kind
        || selections.unit() != unit
        || selections.kind() != kind
        || iterations
            .iter()
            .any(|selection| selection.expression().unit() != unit)
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidStoragePlan,
        );
    }

    let mut planner = match Planner::new(request, types, patterns, selections, iterations) {
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
    pub(super) types: &'view CheckedExpressionTypes,
    pub(super) patterns: &'view CheckedPatternFacts,
    pub(super) selections: &'view CheckedSemanticSelections,
    pub(super) iterations: BTreeMap<BoundExpressionId, &'view SelectedIterationSource>,
    pub(super) builder: Option<StoragePlanBuilder>,
    pub(super) expression_accesses: BTreeMap<BoundExpressionId, StorageAccessId>,
    pub(super) planned_blocks: BTreeSet<BoundBlockId>,
    pub(super) planned_patterns: BTreeSet<BoundPatternId>,
    pub(super) conservative_pattern_bindings: BTreeSet<bray_symbols::LocalBindingSymbolId>,
    pub(super) result_storage: bray_bound_tree::StorageIdentityId,
    pub(super) receiver_storage: Option<bray_bound_tree::StorageIdentityId>,
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

impl<C> Planner<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn new<'view>(
        request: CheckerUnitView<'view, C>,
        types: &'view CheckedExpressionTypes,
        patterns: &'view CheckedPatternFacts,
        selections: &'view CheckedSemanticSelections,
        iterations: &'view [SelectedIterationSource],
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
            types,
            patterns,
            selections,
            iterations: iterations
                .iter()
                .map(|selection| (selection.expression(), selection))
                .collect(),
            builder: Some(builder),
            expression_accesses: BTreeMap::new(),
            planned_blocks: BTreeSet::new(),
            planned_patterns: BTreeSet::new(),
            conservative_pattern_bindings: BTreeSet::new(),
            result_storage,
            receiver_storage: None,
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
                    self.bind_identity(
                        StorageBindingTarget::AnonymousParameter(*parameter),
                        StorageIdentity::AnonymousParameter(*parameter),
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

        let result_storage = self.result_storage;

        for result in results {
            self.builder_mut()?
                .bind(
                    StorageBindingTarget::PostconditionResult(result),
                    StorageBinding::Identity(result_storage),
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
                    self.bind_identity(
                        StorageBindingTarget::Parameter(parameter),
                        StorageIdentity::Parameter(parameter),
                    )?;
                }

                if let Some(receiver) = receiver {
                    self.bind_identity(
                        StorageBindingTarget::Receiver(receiver),
                        StorageIdentity::Receiver(receiver),
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
                    self.bind_identity(
                        StorageBindingTarget::PredicateParameter(parameter),
                        StorageIdentity::PredicateParameter(parameter),
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

pub(super) fn invalid_node(id: impl Into<bray_bound_tree::AnyBoundNodeId>) -> PlanError {
    CheckerInfrastructureError::InvalidBoundNode { node: id.into() }.into()
}

pub(super) const fn iteration_purpose(
    mode: bray_bound_tree::IterationSourceMode,
) -> StorageAccessPurpose {
    match mode {
        bray_bound_tree::IterationSourceMode::Shared => {
            StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Shared)
        }
        bray_bound_tree::IterationSourceMode::Mutable => {
            StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Mutable)
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
