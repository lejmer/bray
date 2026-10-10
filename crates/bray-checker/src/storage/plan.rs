use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BorrowCapabilityOrigin, BoundBlockId, BoundBlockItem, BoundCallableBodyKind, BoundExpressionId,
    BoundPatternId, BoundReferenceTarget, CheckedExpressionTypes, CheckedPatterns,
    CheckedSemanticSelections, DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
    ExpressionTypeResult, PlannedBorrowCapability, SelectedIterationSource, SemanticSelection,
    StorageAccess, StorageAccessId, StorageAccessPurpose, StorageAccessRoot, StorageBinding,
    StorageBindingTarget, StorageIdentity, StoragePlan, StoragePlanBuilder,
};
use bray_symbols::{
    AnySymbolId, BorrowKind, CallableSignatureQuery, CallableSymbolId, PredicateDefinitionSymbolId,
    ReceiverMode, SymbolQueryRequest, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::{
    CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider,
    CheckerUnitRoot, CheckerUnitView, SemanticUnitContext, resolve_type_expression_template,
};

#[derive(Clone, Copy)]
struct EntryStorage {
    ty: TypeId,
    borrow: Option<(BorrowKind, TypeId)>,
}

struct DeclaredCallableEntry {
    parameter_types: BTreeMap<bray_symbols::CallableParameterSymbolId, TypeExpressionTemplate>,
    receiver: Option<(
        bray_symbols::ReceiverParameterSymbolId,
        Option<(BorrowKind, TypeId)>,
    )>,
}

pub(crate) fn plan_storage<C>(
    request: CheckerUnitView<'_, C>,
    declared_types: &DeclaredValueTypeTemplates,
    types: &CheckedExpressionTypes,
    patterns: &CheckedPatterns,
    selections: &CheckedSemanticSelections,
) -> CheckerOutcome<StoragePlan, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
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
        panic!(
            "Storage-planning inputs or constructed records violate the requested unit contract. in plan_storage"
        );
    }

    let callable_entry = match declared_callable_entry(request) {
        Ok(entry) => entry,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let mut planner = Planner::new(
        request,
        declared_types,
        types,
        patterns,
        selections,
        callable_entry,
    );

    match planner.plan() {
        Ok(plan) => CheckerOutcome::complete(plan, planner.diagnostics),
        Err(PlanError::Cancelled) => CheckerOutcome::Cancelled,
        Err(PlanError::Upstream(error)) => CheckerOutcome::UpstreamFailure(error),
    }
}

pub(super) struct Planner<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) request: CheckerUnitView<'view, C>,
    pub(super) declared_types: &'view DeclaredValueTypeTemplates,
    pub(super) types: &'view CheckedExpressionTypes,
    pub(super) patterns: &'view CheckedPatterns,
    pub(super) selections: &'view CheckedSemanticSelections,
    pub(super) iterations: BTreeMap<BoundExpressionId, &'view SelectedIterationSource>,
    pub(super) builder: Option<StoragePlanBuilder>,
    pub(super) diagnostics: bray_diagnostics::DiagnosticBag,
    pub(super) expression_accesses: BTreeMap<BoundExpressionId, StorageAccessId>,
    pub(super) borrowed_values: BTreeMap<bray_bound_tree::StorageIdentityId, StorageAccessId>,
    pub(super) planned_blocks: BTreeSet<BoundBlockId>,
    pub(super) planned_patterns: BTreeSet<BoundPatternId>,
    pub(super) alternative_pattern_bindings:
        BTreeMap<bray_symbols::LocalBindingSymbolId, Vec<Vec<StorageAccessId>>>,
    pub(super) result_storage: bray_bound_tree::StorageIdentityId,
    receiver_entry: Option<(
        bray_symbols::ReceiverParameterSymbolId,
        Option<(BorrowKind, TypeId)>,
    )>,
    parameter_type_templates:
        BTreeMap<bray_symbols::CallableParameterSymbolId, TypeExpressionTemplate>,
}

pub(super) enum PlanError<Upstream = std::convert::Infallible> {
    Cancelled,
    Upstream(Upstream),
}

impl<C> Planner<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn receiver_parameter(&self) -> Option<bray_symbols::ReceiverParameterSymbolId> {
        self.receiver_entry.map(|(receiver, _)| receiver)
    }
}

impl<Upstream> From<CheckerQueryError<Upstream>> for PlanError<Upstream> {
    fn from(error: CheckerQueryError<Upstream>) -> Self {
        match error {
            CheckerQueryError::Cancelled => Self::Cancelled,
            CheckerQueryError::Upstream(error) => Self::Upstream(error),
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
        patterns: &'view CheckedPatterns,
        selections: &'view CheckedSemanticSelections,
        callable_entry: DeclaredCallableEntry,
    ) -> Planner<'view, C> {
        let unit = request.unit().unit();
        let root = request.unit().root().into();
        let mut builder = StoragePlanBuilder::new(unit, request.unit().key().kind());

        let result_storage = builder
            .push_identity(StorageIdentity::Result(root))
            .unwrap_or_else(|error| {
                panic!(
                    "The storage-plan builder rejected one exact relationship. in new: {error:?}"
                )
            });

        builder
            .bind(
                StorageBindingTarget::Result,
                StorageBinding::Identity(result_storage),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "The storage-plan builder rejected one exact relationship. in new: {error:?}"
                )
            });

        Planner {
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
                    | SemanticSelection::CallableReference(_)
                    | SemanticSelection::StaticReference(_)
                    | SemanticSelection::Call(_)
                    | SemanticSelection::Predicate(_)
                    | SemanticSelection::Operation(_)
                    | SemanticSelection::ScopedUse(_)
                    | SemanticSelection::Propagation(_) => None,
                })
                .collect(),
            builder: Some(builder),
            diagnostics: bray_diagnostics::DiagnosticBag::new(),
            expression_accesses: BTreeMap::new(),
            borrowed_values: BTreeMap::new(),
            planned_blocks: BTreeSet::new(),
            planned_patterns: BTreeSet::new(),
            alternative_pattern_bindings: BTreeMap::new(),
            result_storage,
            receiver_entry: callable_entry.receiver,
            parameter_type_templates: callable_entry.parameter_types,
        }
    }

    fn plan(&mut self) -> Result<StoragePlan, PlanError<C::UpstreamError>> {
        self.install_entry_storage()?;

        match self.request.root() {
            CheckerUnitRoot::CallableBody(id) => {
                let body = self
                    .request
                    .view()
                    .callable_body(id)
                    .unwrap_or_else(|| missing_node(id));

                let block = match body.kind() {
                    BoundCallableBodyKind::Block(block) => Some(block),
                    BoundCallableBodyKind::Error(error) => error.body(),
                };

                if let Some(block) = block {
                    self.plan_block(block)?;
                }
            }
            CheckerUnitRoot::Expression(expression) => {
                let purpose = if self.request.unit().key().kind()
                    == bray_bound_tree::BoundUnitKind::RuntimeDefault
                {
                    StorageAccessPurpose::ValueTransfer
                } else {
                    StorageAccessPurpose::Read
                };

                self.plan_expression(expression, Some(purpose))?;
            }
            CheckerUnitRoot::ExpressionSequence(block) => self.plan_block(block)?,
        }

        self.install_recovered_local_storage()?;

        let Some(builder) = self.builder.take() else {
            panic!(
                "Storage-planning inputs or constructed records violate the requested unit contract. in plan"
            );
        };

        Ok(builder.finish())
    }

    fn install_entry_storage(&mut self) -> Result<(), PlanError<C::UpstreamError>> {
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
                .builder_mut()
                .push_identity(StorageIdentity::PostconditionResult(result)).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in install_entry_storage: {error:?}"));

            self.builder_mut()
                .bind(
                    StorageBindingTarget::PostconditionResult(result),
                    StorageBinding::Identity(storage),
                ).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in install_entry_storage: {error:?}"));
        }

        Ok(())
    }

    fn install_declared_inputs(
        &mut self,
        mut symbol: AnySymbolId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        loop {
            if let Some(callable) = CallableSymbolId::try_from_any(symbol) {
                let Some((parameters, receiver)) = self
                    .request
                    .symbols()
                    .callable_parameters_and_receiver(callable)
                else {
                    panic!(
                        "Storage-planning inputs or constructed records violate the requested unit contract. in install_declared_inputs"
                    );
                };

                let parameters = parameters.to_vec();

                for parameter in parameters {
                    let entry = if let Some(entry) =
                        self.entry_storage(BoundReferenceTarget::Surface(parameter.into()))?
                    {
                        Some(entry)
                    } else {
                        let template = self
                            .parameter_type_templates
                            .get(&parameter).unwrap_or_else(|| panic!("install_declared_inputs requires entry parameter type, symbol: {symbol:?}, parameter: {parameter:?}"));

                        self.entry_storage_from_template(template)?
                    };

                    let target = StorageBindingTarget::Parameter(parameter);

                    self.bind_entry(target, StorageIdentity::Parameter(parameter), entry)?;
                }

                if let Some(receiver) = receiver {
                    let borrow = self
                        .receiver_entry
                        .filter(|(parameter, _)| parameter == &receiver)
                        .map(|(_, borrow)| borrow).unwrap_or_else(|| panic!("install_declared_inputs requires entry receiver borrow, symbol: {symbol:?}"));

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
                                .intern_type(TypeData::Borrow { kind, target }).unwrap_or_else(|error| panic!("install_declared_inputs must satisfy its checked construction contract: {error:?}"));

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
                    panic!(
                        "Storage-planning inputs or constructed records violate the requested unit contract. in install_declared_inputs"
                    );
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
        ty: Option<TypeId>,
    ) -> Result<Option<bray_bound_tree::StorageIdentityId>, PlanError<C::UpstreamError>> {
        if self.builder().binding(target).is_some() {
            return Ok(None);
        }

        let storage = self
            .builder_mut()
            .push_identity(identity).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_identity: {error:?}"));

        if let Some(ty) = ty {
            self.builder_mut()
                .set_identity_type(storage, ty).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_identity: {error:?}"));
        }

        self.builder_mut()
            .bind(target, StorageBinding::Identity(storage)).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_identity: {error:?}"));

        Ok(Some(storage))
    }

    fn bind_entry(
        &mut self,
        target: StorageBindingTarget,
        identity: StorageIdentity,
        entry: Option<EntryStorage>,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        if self.builder().binding(target).is_some() {
            return Ok(());
        }

        let storage = self
            .builder_mut()
            .push_identity(identity).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_entry: {error:?}"));

        if let Some(entry) = entry {
            self.builder_mut()
                .set_identity_type(storage, entry.ty).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_entry: {error:?}"));
        }

        let source = bray_bound_tree::BoundSourceAnchor::new(
            self.request.unit().key().source().syntax(),
            self.request.unit().key().source().source_version(),
        );

        let root_access = if let Some(entry) = entry {
            let reached_type = entry
                .borrow
                .map(|(_, reached_type)| reached_type)
                .unwrap_or(entry.ty);

            let access = StorageAccess::new(
                StorageAccessRoot::Storage(storage),
                [],
                reached_type,
                source,
                false,
            );

            Some(
                self.builder_mut()
                    .push_access(access).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_entry: {error:?}")),
            )
        } else {
            None
        };

        let Some((kind, reached_type)) = entry.and_then(|entry| entry.borrow) else {
            self.builder_mut()
                .bind(target, StorageBinding::Identity(storage)).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_entry: {error:?}"));

            return Ok(());
        };

        let borrowed = root_access.unwrap_or_else(|| panic!("bind_entry requires borrowed entry root access, target: {target:?}, identity: {identity:?}"));

        let capability = PlannedBorrowCapability::new(
            BorrowCapabilityOrigin::Entry(target),
            kind,
            borrowed,
            None,
            source,
            false,
        );

        let capability = self
            .builder_mut()
            .push_borrow_capability(capability).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_entry: {error:?}"));

        let access = StorageAccess::new(
            StorageAccessRoot::Borrow(capability),
            [],
            reached_type,
            source,
            false,
        );

        let access = self
            .builder_mut()
            .push_access(access).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_entry: {error:?}"));

        self.builder_mut()
            .bind(target, StorageBinding::Access(access))
            .unwrap_or_else(|error| {
                panic!("bind_entry must satisfy its checked construction contract: {error:?}")
            });

        Ok(())
    }

    fn entry_storage(
        &self,
        target: BoundReferenceTarget,
    ) -> Result<Option<EntryStorage>, PlanError<C::UpstreamError>> {
        let Some(template) = self
            .declared_types
            .evidence()
            .iter()
            .find(|evidence| evidence.term() == DeclaredValueTypeTerm::Value(target))
            .map(|evidence| evidence.template())
        else {
            return Ok(None);
        };

        self.entry_storage_from_template(template)
    }

    fn entry_storage_from_template(
        &self,
        template: &TypeExpressionTemplate,
    ) -> Result<Option<EntryStorage>, PlanError<C::UpstreamError>> {
        let ty = match template {
            TypeExpressionTemplate::Resolved(ty) => *ty,
            _ => {
                let terms = self.request.checked_constant_terms(template)?;

                let Some(ty) = resolve_type_expression_template(
                    self.request.semantic_values(),
                    template,
                    terms.value(),
                ) else {
                    return Ok(None);
                };

                ty
            }
        };

        let data = self.request.semantic_values().type_data(ty);

        let borrow = match data.as_ref() {
            TypeData::Borrow { kind, target } => Some((*kind, *target)),
            _ => None,
        };

        Ok(Some(EntryStorage { ty, borrow }))
    }

    fn install_recovered_local_storage(&mut self) -> Result<(), PlanError<C::UpstreamError>> {
        let root = self.request.unit().root().into();

        let bindings = self
            .patterns
            .binding_types()
            .iter()
            .map(|binding| binding.binding())
            .collect::<Vec<_>>();

        for binding in bindings {
            let target = StorageBindingTarget::Local(binding);

            if self.builder().binding(target).is_none() {
                let _ = self.bind_identity(target, StorageIdentity::LocalOwned(root), None)?;
            }
        }

        Ok(())
    }

    pub(super) fn plan_block(
        &mut self,
        id: BoundBlockId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
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
            .unwrap_or_else(|| missing_node(id))
            .clone();

        for item in block.items() {
            match item {
                BoundBlockItem::LocalBinding(binding) => {
                    let subject = self.plan_expression(binding.initializer(), None)?;

                    self.record_purpose(
                        binding.initializer(),
                        Some(StorageAccessPurpose::Projection),
                        subject,
                    )?;

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

    pub(super) fn expression_type(&self, expression: BoundExpressionId) -> ExpressionTypeResult {
        self.types.expression(expression).unwrap_or_else(|| {
            panic!("storage planning requires a checked expression type for {expression:?}")
        })
    }

    pub(super) fn builder(&self) -> &StoragePlanBuilder {
        self.builder
            .as_ref()
            .expect("storage-plan builder must remain present until the plan is finished")
    }

    pub(super) fn builder_mut(&mut self) -> &mut StoragePlanBuilder {
        self.builder
            .as_mut()
            .expect("storage-plan builder must remain present until the plan is finished")
    }

    pub(super) fn check_cancellation(&self) -> Result<(), PlanError<C::UpstreamError>> {
        if self.request.is_cancelled() {
            Err(PlanError::Cancelled)
        } else {
            Ok(())
        }
    }
}

fn declared_callable_entry<C>(
    request: CheckerUnitView<'_, C>,
) -> Result<DeclaredCallableEntry, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let Some(callable) = request.containing_callable() else {
        return Ok(DeclaredCallableEntry {
            parameter_types: BTreeMap::new(),
            receiver: None,
        });
    };

    let signature = request
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))?;

    let parameter_types = signature
        .value()
        .parameter_type_templates(request.semantic_values())
        .unwrap_or_else(|error| {
            panic!("checked callable signature must match its storage plan: {error:?}")
        });

    if signature.value().parameters().len() != parameter_types.len() {
        panic!(
            "Storage-planning inputs or constructed records violate the requested unit contract. in declared_callable_entry"
        );
    }

    let parameter_types = signature
        .value()
        .parameters()
        .iter()
        .copied()
        .zip(parameter_types)
        .collect();

    let receiver = signature.value().receiver().map(|receiver| {
        let borrow = match receiver.mode() {
            ReceiverMode::Shared => Some((BorrowKind::Shared, receiver.ty())),
            ReceiverMode::Mutable => Some((BorrowKind::Mutable, receiver.ty())),
            ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => None,
        };

        (receiver.parameter(), borrow)
    });

    Ok(DeclaredCallableEntry {
        parameter_types,
        receiver,
    })
}

pub(super) fn missing_node(id: impl Into<bray_bound_tree::AnyBoundNodeId>) -> ! {
    panic!(
        "storage node {:?} must belong to the committed tree and checked inputs",
        id.into()
    );
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
