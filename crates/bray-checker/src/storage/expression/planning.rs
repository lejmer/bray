use bray_bound_tree::{
    BoundCallableTarget, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundPatternMode, BoundReferenceTarget, BoundStructuredExpressionKind, IndexTarget,
    OperatorTarget, SelectedOperation, SemanticSelection, StorageAccessId, StorageAccessPurpose,
    StorageIdentity, StorageProjection,
};
use bray_symbols::{BorrowKind, ReceiverMode, TypeData};

use super::super::plan::{PlanError, Planner, invalid_node, iteration_purpose};
use crate::{CheckerInfrastructureError, CheckerRequestContext};

impl<C> Planner<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(in crate::storage) fn plan_expression(
        &mut self,
        id: BoundExpressionId,
        purpose: Option<StorageAccessPurpose>,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        self.check_cancellation()?;

        if let Some(access) = self.expression_accesses.get(&id).copied() {
            self.record_purpose(id, purpose, access)?;

            return Ok(access);
        }

        // The immutable node is cloned so recursive planning can mutably advance task-local state.
        let expression = self
            .request
            .view()
            .expression(id)
            .ok_or_else(|| invalid_node(id))?
            .clone();

        let access = match &expression {
            BoundExpression::Name(name) => self.reference_access(id, name.target())?,
            BoundExpression::PatternReference(reference) => {
                let target = match self.selections.expression(id) {
                    Some(SemanticSelection::Reference(target)) => *target,
                    Some(_) | None => BoundReferenceTarget::Local(reference.binding().into()),
                };

                self.reference_access(id, target)?
            }
            BoundExpression::Assignment(assignment) => {
                self.plan_assignment(id, assignment.operator(), assignment.operands())?
            }
            BoundExpression::Unary(expression) => self.plan_operator(id, expression.operands())?,
            BoundExpression::Binary(expression) => self.plan_operator(id, expression.operands())?,
            BoundExpression::MemberAccess(member) => {
                if self.is_compile_time_qualifier(member.receiver())? {
                    self.temporary_access(id)?
                } else {
                    let receiver = self.plan_expression(member.receiver(), None)?;
                    let projection = self.member_projection(id, member.selector())?;
                    let access = self.member_access(id, receiver, projection)?;

                    self.record_purpose(id, Some(StorageAccessPurpose::Member), access)?;

                    access
                }
            }
            BoundExpression::TraitQualifiedMember(member) => {
                let receiver = self.plan_expression(member.receiver(), None)?;
                let projection = self.selected_member_projection(id)?;
                let access = self.member_access(id, receiver, projection)?;

                self.record_purpose(id, Some(StorageAccessPurpose::Member), access)?;

                access
            }
            BoundExpression::Structured(structured) => {
                if matches!(
                    structured.kind(),
                    BoundStructuredExpressionKind::PatternTest
                        | BoundStructuredExpressionKind::PatternBinding
                ) {
                    let [subject] = structured.operands() else {
                        return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                    };

                    let access =
                        self.plan_expression(*subject, Some(StorageAccessPurpose::Read))?;

                    for pattern in structured.patterns() {
                        self.plan_pattern(*pattern, *subject, access)?;
                    }
                }

                self.plan_structured(id, structured.kind(), structured.operands())?
            }
            BoundExpression::StructConstruction(construction) => {
                for field in construction.fields() {
                    self.plan_expression(
                        field.expression(),
                        Some(StorageAccessPurpose::ValueTransfer),
                    )?;
                }

                self.temporary_access(id)?
            }
            BoundExpression::Call(call) => self.plan_call(
                id,
                call.callee(),
                call.arguments()
                    .iter()
                    .map(bray_bound_tree::BoundArgument::expression),
            )?,
            BoundExpression::ErrorCall(call) => self.plan_call(
                id,
                call.callee(),
                call.arguments()
                    .iter()
                    .map(bray_bound_tree::BoundArgument::expression),
            )?,
            BoundExpression::For(expression) => {
                let source = self.plan_expression(expression.source(), None)?;

                let mode = self
                    .iterations
                    .get(&id)
                    .map(|selection| selection.mode())
                    .unwrap_or_else(|| expression.source_mode());

                self.record_purpose(expression.source(), Some(iteration_purpose(mode)), source)?;

                let element = self
                    .plan_iteration_storage(id)?
                    .map_or(source, |(_, element)| element);

                self.plan_pattern(expression.pattern(), id, element)?;
                self.plan_block(expression.body())?;

                if let Some(else_body) = expression.else_body() {
                    self.plan_block(else_body)?;
                }

                self.temporary_access(id)?
            }
            BoundExpression::Generator(expression) => {
                let source = self.plan_expression(expression.source(), None)?;

                let mode = self
                    .iterations
                    .get(&id)
                    .map(|selection| selection.mode())
                    .unwrap_or_else(|| expression.source_mode());

                self.record_purpose(expression.source(), Some(iteration_purpose(mode)), source)?;

                let element = self
                    .plan_iteration_storage(id)?
                    .map_or(source, |(_, element)| element);

                self.plan_pattern(expression.pattern(), id, element)?;
                self.plan_block(expression.body())?;

                self.temporary_access(id)?
            }
            BoundExpression::Match(expression) => {
                let subject = self.plan_expression(expression.subject(), None)?;

                let subject_purpose = match expression.arms().first() {
                    Some(arm) => {
                        let pattern = self
                            .request
                            .view()
                            .pattern(arm.pattern())
                            .ok_or_else(|| invalid_node(arm.pattern()))?;

                        match pattern.mode() {
                            BoundPatternMode::MatchConsume | BoundPatternMode::MatchObserve => {
                                StorageAccessPurpose::Read
                            }
                            BoundPatternMode::Declaration | BoundPatternMode::Assignment => {
                                return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                            }
                        }
                    }
                    None => StorageAccessPurpose::Read,
                };

                self.record_purpose(expression.subject(), Some(subject_purpose), subject)?;

                for arm in expression.arms() {
                    self.plan_pattern(arm.pattern(), expression.subject(), subject)?;

                    if let Some(guard) = arm.guard() {
                        self.plan_expression(guard, Some(StorageAccessPurpose::Read))?;
                    }

                    self.plan_block(arm.body())?;
                }

                self.temporary_access(id)?
            }
            BoundExpression::ControlTransfer(transfer) => {
                if let Some(operand) = transfer.operand() {
                    self.plan_expression(operand, Some(StorageAccessPurpose::ValueTransfer))?;
                }

                if transfer.kind() == BoundControlTransferKind::Return {
                    let access = self.result_access(id, transfer.operand())?;

                    self.record_purpose(id, Some(StorageAccessPurpose::Initialize), access)?;

                    access
                } else {
                    self.temporary_access(id)?
                }
            }
            _ => {
                for child in expression.child_expressions() {
                    self.plan_expression(child, Some(StorageAccessPurpose::Read))?;
                }

                for block in expression.child_blocks() {
                    self.plan_block(block)?;
                }

                self.temporary_access(id)?
            }
        };

        self.expression_accesses.insert(id, access);
        self.record_purpose(id, purpose, access)?;

        Ok(access)
    }

    fn is_compile_time_qualifier(
        &self,
        expression: BoundExpressionId,
    ) -> Result<bool, PlanError<C::UpstreamError>> {
        let expression = self
            .request
            .view()
            .expression(expression)
            .ok_or_else(|| invalid_node(expression))?;

        Ok(matches!(
            expression,
            BoundExpression::Name(name) if name.target().is_compile_time_qualifier()
        ))
    }

    fn plan_call(
        &mut self,
        id: BoundExpressionId,
        callee: BoundExpressionId,
        arguments: impl IntoIterator<Item = BoundExpressionId>,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let semantic_selection = self.selections.expression(id);

        let selection = semantic_selection.and_then(|selection| {
            let SemanticSelection::Call(call) = selection else {
                return None;
            };

            Some(call)
        });

        let direct = selection
            .is_some_and(|call| matches!(call.target(), BoundCallableTarget::Declaration(_)))
            || matches!(
                semantic_selection,
                Some(SemanticSelection::Operation(
                    SelectedOperation::Construction(_)
                ))
            );

        if !direct {
            self.plan_expression(callee, Some(StorageAccessPurpose::Read))?;
        }

        if let Some(receiver) = selection.and_then(bray_bound_tree::SelectedCall::receiver) {
            self.plan_expression(
                receiver.expression(),
                Some(Self::call_receiver_purpose(receiver.mode())),
            )?;
        }

        for argument in arguments {
            self.plan_expression(argument, Some(StorageAccessPurpose::ValueTransfer))?;
        }

        self.temporary_access(id)
    }

    const fn call_receiver_purpose(mode: ReceiverMode) -> StorageAccessPurpose {
        match mode {
            ReceiverMode::Shared => StorageAccessPurpose::Borrow(BorrowKind::Shared),
            ReceiverMode::Mutable => StorageAccessPurpose::Borrow(BorrowKind::Mutable),
            ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => {
                StorageAccessPurpose::ValueTransfer
            }
        }
    }

    fn plan_operator(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let purpose = self.operator_operand_purpose(id);

        for operand in operands {
            self.plan_expression(*operand, Some(purpose))?;
        }

        self.temporary_access(id)
    }

    fn plan_assignment(
        &mut self,
        id: BoundExpressionId,
        operator: bray_bound_tree::BoundAssignmentOperator,
        operands: &[BoundExpressionId],
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let Some((destination, values)) = operands.split_first() else {
            return self.recovery_access(id);
        };

        let destination_access = self.plan_expression(*destination, None)?;

        let destination_access =
            self.assignment_destination_access(*destination, destination_access)?;

        if operator.binary_operator().is_some() {
            let purpose = self.operator_operand_purpose(id);

            self.record_purpose(*destination, Some(purpose), destination_access)?;
        }

        self.record_purpose(
            *destination,
            Some(StorageAccessPurpose::Write),
            destination_access,
        )?;

        self.record_purpose(
            id,
            Some(StorageAccessPurpose::Assignment),
            destination_access,
        )?;

        let value_purpose = if operator.binary_operator().is_some() {
            self.operator_operand_purpose(id)
        } else {
            StorageAccessPurpose::ValueTransfer
        };

        for value in values {
            self.plan_expression(*value, Some(value_purpose))?;
        }

        self.temporary_access(id)
    }

    fn operator_operand_purpose(&self, id: BoundExpressionId) -> StorageAccessPurpose {
        let target = self
            .selections
            .expression(id)
            .and_then(|selection| match selection {
                SemanticSelection::Operation(operation) => operation.operator_target(),
                _ => None,
            });

        match target {
            Some(OperatorTarget::Trait { .. } | OperatorTarget::TraitConstraint { .. }) => {
                StorageAccessPurpose::Borrow(BorrowKind::Shared)
            }
            Some(OperatorTarget::BuiltIn(_)) | None => StorageAccessPurpose::Read,
        }
    }

    fn assignment_destination_access(
        &mut self,
        destination: BoundExpressionId,
        access: StorageAccessId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let destination_type = self.expression_type(destination)?;

        let data = self
            .request
            .semantic_values()
            .type_data(destination_type.ty())
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        match data.as_ref() {
            TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target,
            } => self.access_with_reached_type(destination, access, *target),
            _ => Ok(access),
        }
    }

    fn plan_structured(
        &mut self,
        id: BoundExpressionId,
        kind: BoundStructuredExpressionKind,
        operands: &[BoundExpressionId],
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        match kind {
            BoundStructuredExpressionKind::TrustBoundary => {
                let Some(operand) = operands.first().copied() else {
                    return self.recovery_access(id);
                };

                let access = self.plan_expression(operand, None)?;

                for operand in &operands[1..] {
                    self.plan_expression(*operand, Some(StorageAccessPurpose::Read))?;
                }

                Ok(access)
            }
            BoundStructuredExpressionKind::Borrow => {
                let Some(operand) = operands.first().copied() else {
                    return self.recovery_access(id);
                };

                let operand_access = self.plan_expression(operand, None)?;

                let borrow_kind = self
                    .request
                    .view()
                    .expression(id)
                    .and_then(|expression| match expression {
                        BoundExpression::Structured(expression) => expression.borrow_kind(),
                        _ => None,
                    })
                    .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                let access = self.borrow_access(id, operand_access, borrow_kind)?;

                self.record_purpose(id, Some(StorageAccessPurpose::Borrow(borrow_kind)), access)?;

                for operand in &operands[1..] {
                    self.plan_expression(*operand, Some(StorageAccessPurpose::Read))?;
                }

                Ok(access)
            }
            BoundStructuredExpressionKind::ElementIndex
            | BoundStructuredExpressionKind::SliceIndex
            | BoundStructuredExpressionKind::NullablePropagation => {
                self.plan_structured_projection(id, kind, operands)
            }
            BoundStructuredExpressionKind::BooleanAllFold
            | BoundStructuredExpressionKind::BooleanAnyFold => {
                let Some(source) = operands.first().copied() else {
                    return self.recovery_access(id);
                };

                let access = self.plan_expression(source, None)?;

                self.record_purpose(
                    source,
                    Some(iteration_purpose(
                        bray_bound_tree::IterationSourceMode::Shared,
                    )),
                    access,
                )?;

                self.plan_iteration_storage(id)?;

                self.temporary_access(id)
            }
            BoundStructuredExpressionKind::Tuple
            | BoundStructuredExpressionKind::Array
            | BoundStructuredExpressionKind::RepeatedArray
            | BoundStructuredExpressionKind::TypeFormConstruction => {
                for operand in operands {
                    self.plan_expression(*operand, Some(StorageAccessPurpose::ValueTransfer))?;
                }

                self.temporary_access(id)
            }
            _ => {
                for operand in operands {
                    self.plan_expression(*operand, Some(StorageAccessPurpose::Read))?;
                }

                // The immutable node is cloned so recursive planning can mutably advance
                // task-local state.
                let expression = self
                    .request
                    .view()
                    .expression(id)
                    .ok_or_else(|| invalid_node(id))?
                    .clone();

                for block in expression.child_blocks() {
                    self.plan_block(block)?;
                }

                self.temporary_access(id)
            }
        }
    }

    fn plan_iteration_storage(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<Option<(StorageAccessId, StorageAccessId)>, PlanError<C::UpstreamError>> {
        let Some(selection) = self.iterations.get(&expression) else {
            return Ok(None);
        };

        let cursor_type = selection.cursor_type();
        let element_type = selection.element_type();

        let cursor = self.iteration_access(
            expression,
            StorageIdentity::IterationCursor(expression),
            cursor_type,
        )?;

        let element = self.iteration_access(
            expression,
            StorageIdentity::IterationElement(expression),
            element_type,
        )?;

        self.record_purpose(expression, Some(StorageAccessPurpose::Initialize), cursor)?;
        self.record_purpose(expression, Some(StorageAccessPurpose::Initialize), element)?;

        Ok(Some((cursor, element)))
    }

    fn plan_structured_projection(
        &mut self,
        id: BoundExpressionId,
        kind: BoundStructuredExpressionKind,
        operands: &[BoundExpressionId],
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let Some(receiver) = operands.first().copied() else {
            return self.recovery_access(id);
        };

        let target = match kind {
            BoundStructuredExpressionKind::ElementIndex
            | BoundStructuredExpressionKind::SliceIndex => self.selected_index_target(id)?,
            _ => None,
        };

        let custom_borrow_kind = target.and_then(IndexTarget::custom_borrow_kind);
        let custom = custom_borrow_kind.is_some();

        let receiver_purpose = match kind {
            BoundStructuredExpressionKind::NullablePropagation => Some(StorageAccessPurpose::Read),
            _ => custom_borrow_kind.map(StorageAccessPurpose::Borrow),
        };

        let receiver_access = self.plan_expression(receiver, receiver_purpose)?;

        for selector in &operands[1..] {
            let purpose = match (kind, custom) {
                (BoundStructuredExpressionKind::ElementIndex, true) => {
                    StorageAccessPurpose::Borrow(BorrowKind::Shared)
                }
                (BoundStructuredExpressionKind::SliceIndex, true) => {
                    StorageAccessPurpose::ValueTransfer
                }
                _ => StorageAccessPurpose::Read,
            };

            self.plan_expression(*selector, Some(purpose))?;
        }

        let (projection, purpose) = match kind {
            BoundStructuredExpressionKind::ElementIndex => {
                match target {
                    Some(
                        IndexTarget::ArrayElement
                        | IndexTarget::SliceElement
                        | IndexTarget::Custom { .. }
                        | IndexTarget::TraitConstraint { .. },
                    ) => {}
                    Some(IndexTarget::ArraySlice | IndexTarget::Slice) => {
                        return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                    }
                    None => {
                        let access = self.conservative_subject_access(id, receiver_access)?;

                        self.record_purpose(id, Some(StorageAccessPurpose::Index), access)?;

                        return Ok(access);
                    }
                }

                let Some(selector) = operands.get(1).copied() else {
                    return self.recovery_access(id);
                };

                (
                    StorageProjection::Element(selector),
                    StorageAccessPurpose::Index,
                )
            }
            BoundStructuredExpressionKind::SliceIndex => {
                match target {
                    Some(
                        IndexTarget::ArraySlice
                        | IndexTarget::Slice
                        | IndexTarget::Custom { .. }
                        | IndexTarget::TraitConstraint { .. },
                    ) => {}
                    Some(IndexTarget::ArrayElement | IndexTarget::SliceElement) => {
                        return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                    }
                    None => {
                        let access = self.conservative_subject_access(id, receiver_access)?;

                        self.record_purpose(id, Some(StorageAccessPurpose::Slice), access)?;

                        return Ok(access);
                    }
                }

                let bounds = self
                    .request
                    .view()
                    .expression(id)
                    .and_then(|expression| match expression {
                        BoundExpression::Structured(expression) => expression.slice_bounds(),
                        _ => None,
                    })
                    .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                (
                    StorageProjection::SliceRange {
                        start: bounds.lower(),
                        end: bounds.upper(),
                    },
                    StorageAccessPurpose::Slice,
                )
            }
            BoundStructuredExpressionKind::NullablePropagation => (
                StorageProjection::NullableValue,
                StorageAccessPurpose::Projection,
            ),
            _ => return Err(CheckerInfrastructureError::InvalidStoragePlan.into()),
        };

        let access = match custom_borrow_kind {
            Some(kind) => self.custom_index_access(id, receiver_access, kind)?,
            None => self.project_access(id, receiver_access, Some(projection))?,
        };

        if let Some(kind) = custom_borrow_kind {
            self.record_purpose(id, Some(StorageAccessPurpose::Borrow(kind)), access)?;
        }

        self.record_purpose(id, Some(purpose), access)?;

        Ok(access)
    }

    fn selected_index_target(
        &self,
        expression: BoundExpressionId,
    ) -> Result<Option<IndexTarget>, PlanError<C::UpstreamError>> {
        match self.selections.expression(expression) {
            Some(SemanticSelection::Operation(SelectedOperation::Index { target, .. })) => {
                Ok(Some(*target))
            }
            Some(_) => Err(CheckerInfrastructureError::InvalidStoragePlan.into()),
            None => {
                let bound = self
                    .request
                    .view()
                    .expression(expression)
                    .ok_or_else(|| invalid_node(expression))?;

                let BoundExpression::Structured(structured) = bound else {
                    return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                };

                if structured.is_recovered() {
                    return Ok(None);
                }

                let Some(receiver) = structured.operands().first().copied() else {
                    return Ok(None);
                };

                let receiver = self
                    .types
                    .expression(receiver)
                    .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                if receiver.is_recovered() {
                    return Ok(None);
                }

                let data = self
                    .request
                    .semantic_values()
                    .type_data(receiver.ty())
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                Ok(match (structured.kind(), data.as_ref()) {
                    (
                        BoundStructuredExpressionKind::ElementIndex,
                        bray_symbols::TypeData::Array { .. },
                    ) => Some(IndexTarget::ArrayElement),
                    (
                        BoundStructuredExpressionKind::ElementIndex,
                        bray_symbols::TypeData::Slice(_),
                    ) => Some(IndexTarget::SliceElement),
                    (
                        BoundStructuredExpressionKind::SliceIndex,
                        bray_symbols::TypeData::Array { .. },
                    ) => Some(IndexTarget::ArraySlice),
                    (
                        BoundStructuredExpressionKind::SliceIndex,
                        bray_symbols::TypeData::Slice(_),
                    ) => Some(IndexTarget::Slice),
                    _ => None,
                })
            }
        }
    }
}
