use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundCallableBodyKind, BoundExpression, BoundExpressionId,
    BoundMemberSelector, BoundPatternId, BoundReferenceTarget, BoundStructuredExpression,
    BoundStructuredExpressionKind, CheckedStorageFactsBuildError, CheckedStorageFactsBuilder,
    StorageAccess, StorageAccessId, StorageAccessOccurrence, StorageAccessRoot, StorageIdentity,
    StorageIdentityId, StorageParameter, StorageProjection, StorageReferent, SurfaceStorageSymbol,
};
use bray_symbols::{AnyLocalSymbolId, LocalBindingSymbolId, SymbolOrdinal, TypeData, TypeId};

use crate::{CheckerOutcome, StorageCheckResult, UnitCheckRequest, UnitCheckRoot};

/// A failure that prevents checked storage facts from being established.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageCheckError {
    /// The canonical recovery type could not be obtained from the semantic value store.
    SemanticValueStore,
    /// Durable storage facts did not satisfy their checked-unit contract.
    Construction(CheckedStorageFactsBuildError),
}

pub(crate) fn check_storage(
    request: UnitCheckRequest<'_>,
) -> Result<CheckerOutcome<StorageCheckResult>, StorageCheckError> {
    if request.is_cancelled() {
        return Ok(CheckerOutcome::Cancelled);
    }

    let error_type = request
        .semantic_values()
        .intern_type(TypeData::Error)
        .map_err(|_| StorageCheckError::SemanticValueStore)?;

    let mut analysis = StorageAnalysis::new(request, error_type);

    match analysis.check_root() {
        Ok(()) => {}
        Err(StorageAnalysisError::Cancelled) => return Ok(CheckerOutcome::Cancelled),
        Err(StorageAnalysisError::Construction(error)) => {
            return Err(StorageCheckError::Construction(error));
        }
    }

    let facts = analysis
        .builder
        .finish(request.view(), request.root().node())
        .map_err(StorageCheckError::Construction)?;

    Ok(CheckerOutcome::without_diagnostics(
        StorageCheckResult::new(facts),
    ))
}

struct StorageAnalysis<'view> {
    request: UnitCheckRequest<'view>,
    builder: CheckedStorageFactsBuilder,
    error_type: TypeId,
    expression_accesses: BTreeMap<BoundExpressionId, StorageAccessId>,
    local_storage: BTreeMap<LocalBindingSymbolId, StorageIdentityId>,
    parameter_storage: BTreeMap<StorageParameter, StorageIdentityId>,
    surface_storage: BTreeMap<SurfaceStorageSymbol, StorageIdentityId>,
    checked_blocks: BTreeSet<BoundBlockId>,
    checked_patterns: BTreeSet<BoundPatternId>,
}

enum StorageAnalysisError {
    Cancelled,
    Construction(CheckedStorageFactsBuildError),
}

impl From<CheckedStorageFactsBuildError> for StorageAnalysisError {
    fn from(error: CheckedStorageFactsBuildError) -> Self {
        Self::Construction(error)
    }
}

impl<'view> StorageAnalysis<'view> {
    fn new(request: UnitCheckRequest<'view>, error_type: TypeId) -> Self {
        Self {
            request,
            builder: CheckedStorageFactsBuilder::new(request.view().unit(), request.view().kind()),
            error_type,
            expression_accesses: BTreeMap::new(),
            local_storage: BTreeMap::new(),
            parameter_storage: BTreeMap::new(),
            surface_storage: BTreeMap::new(),
            checked_blocks: BTreeSet::new(),
            checked_patterns: BTreeSet::new(),
        }
    }

    fn check_root(&mut self) -> Result<(), StorageAnalysisError> {
        match self.request.root() {
            UnitCheckRoot::CallableBody(body) => {
                let body =
                    self.request.view().callable_body(body).ok_or(
                        CheckedStorageFactsBuildError::MissingBoundNode { node: body.into() },
                    )?;

                let block = match body.kind() {
                    BoundCallableBodyKind::Block(block) => Some(block),
                    BoundCallableBodyKind::Error(error) => error.body(),
                };

                if let Some(block) = block {
                    self.check_block(block)?;
                }
            }
            UnitCheckRoot::Expression(expression) => {
                self.check_expression(expression)?;
            }
            UnitCheckRoot::ExpressionSequence(block) => self.check_block(block)?,
        }

        Ok(())
    }

    fn check_block(&mut self, id: BoundBlockId) -> Result<(), StorageAnalysisError> {
        self.observe_cancellation()?;

        if !self.checked_blocks.insert(id) {
            return Ok(());
        }

        let block = self
            .request
            .view()
            .block(id)
            .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node: id.into() })?;

        for item in block.items() {
            match item {
                BoundBlockItem::LocalBinding(binding) => {
                    self.check_expression(binding.initializer())?;
                    self.check_pattern(binding.pattern())?;
                }
                BoundBlockItem::LocalConstant(constant) => {
                    self.check_expression(constant.initializer())?;
                }
                BoundBlockItem::Expression(expression) => {
                    self.check_expression(*expression)?;
                }
            }
        }

        Ok(())
    }

    fn check_pattern(&mut self, id: BoundPatternId) -> Result<(), StorageAnalysisError> {
        self.observe_cancellation()?;

        if !self.checked_patterns.insert(id) {
            return Ok(());
        }

        let pattern = self
            .request
            .view()
            .pattern(id)
            .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node: id.into() })?;

        for binding in pattern.bindings() {
            if self.local_storage.contains_key(binding) {
                continue;
            }

            let storage = self
                .builder
                .push_identity(StorageIdentity::LocalOwned(id.into()))?;

            self.builder
                .record_local_storage(*binding, StorageReferent::Identity(storage))?;

            let access = self.builder.push_access(StorageAccess::new(
                StorageAccessRoot::Storage(storage),
                [],
                pattern.input_type(),
                pattern.origin().source_anchor(),
                pattern.is_recovered(),
            ))?;

            self.builder
                .record_occurrence_access(StorageAccessOccurrence::Binding(*binding), access)?;
            self.local_storage.insert(*binding, storage);
        }

        for child in pattern.children() {
            self.check_pattern(*child)?;
        }

        Ok(())
    }

    fn check_expression(
        &mut self,
        id: BoundExpressionId,
    ) -> Result<Option<StorageAccessId>, StorageAnalysisError> {
        self.observe_cancellation()?;

        if let Some(access) = self.expression_accesses.get(&id).copied() {
            return Ok(Some(access));
        }

        let expression = self
            .request
            .view()
            .expression(id)
            .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node: id.into() })?;

        match expression {
            BoundExpression::Name(name) => self.check_name(id, name.target()),
            BoundExpression::Assignment(assignment) => {
                let mut operands = assignment.operands().iter().copied();
                let destination = match operands.next() {
                    Some(destination) => {
                        let access = match self.check_expression(destination)? {
                            Some(access) => access,
                            None => self.recovery_access(destination)?,
                        };

                        Some((destination, access))
                    }
                    None => None,
                };

                for operand in operands {
                    self.check_expression(operand)?;
                }

                if let Some((destination, access)) = destination {
                    self.builder.record_occurrence_access(
                        StorageAccessOccurrence::Write(destination),
                        access,
                    )?;
                    self.builder.record_occurrence_access(
                        StorageAccessOccurrence::Assignment(id),
                        access,
                    )?;
                }

                Ok(None)
            }
            BoundExpression::MemberAccess(member) => {
                let receiver = self.check_expression(member.receiver())?;
                let projection = match member.selector() {
                    Some(BoundMemberSelector::TupleElement(index)) => {
                        Some(StorageProjection::TupleElement(SymbolOrdinal::new(*index)))
                    }
                    Some(BoundMemberSelector::Name(_)) | None => None,
                };

                self.check_projection(id, expression, receiver, projection)
            }
            BoundExpression::Structured(structured) if projection_kind(structured.kind()) => {
                self.check_structured_projection(id, expression, structured)
            }
            _ => {
                for child in expression.child_expressions() {
                    self.check_expression(child)?;
                }

                for pattern in expression.child_patterns() {
                    self.check_pattern(pattern)?;
                }

                for block in expression.child_blocks() {
                    self.check_block(block)?;
                }

                Ok(None)
            }
        }
    }

    fn check_name(
        &mut self,
        id: BoundExpressionId,
        target: BoundReferenceTarget,
    ) -> Result<Option<StorageAccessId>, StorageAnalysisError> {
        let storage = match target {
            BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)) => {
                self.local_storage.get(&binding).copied()
            }
            BoundReferenceTarget::Local(local) => {
                let Some(parameter) = StorageParameter::from_local_symbol(local) else {
                    return Ok(None);
                };

                Some(self.parameter_identity(parameter)?)
            }
            BoundReferenceTarget::Surface(surface) => {
                if let Some(parameter) = StorageParameter::from_surface_symbol(surface) {
                    Some(self.parameter_identity(parameter)?)
                } else if let Some(surface) = SurfaceStorageSymbol::from_symbol(surface) {
                    Some(self.surface_identity(surface)?)
                } else {
                    return Ok(None);
                }
            }
        };

        let access = match storage {
            Some(storage) => self.direct_access(id, storage)?,
            None => self.recovery_access(id)?,
        };

        self.builder
            .record_occurrence_access(StorageAccessOccurrence::Read(id), access)?;
        self.expression_accesses.insert(id, access);

        Ok(Some(access))
    }

    fn check_structured_projection(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundExpression,
        structured: &BoundStructuredExpression,
    ) -> Result<Option<StorageAccessId>, StorageAnalysisError> {
        let mut operands = structured.operands().iter().copied();
        let receiver = match operands.next() {
            Some(receiver) => self.check_expression(receiver)?,
            None => None,
        };
        let selectors = operands.collect::<Vec<_>>();

        for selector in &selectors {
            self.check_expression(*selector)?;
        }

        let projection = match structured.kind() {
            BoundStructuredExpressionKind::ElementIndex => {
                selectors.first().copied().map(StorageProjection::Element)
            }
            BoundStructuredExpressionKind::SliceIndex => Some(StorageProjection::SliceRange {
                start: selectors.first().copied(),
                end: selectors.get(1).copied(),
            }),
            BoundStructuredExpressionKind::NullablePropagation => {
                Some(StorageProjection::NullableValue)
            }
            _ => None,
        };

        self.check_projection(id, expression, receiver, projection)
    }

    fn check_projection(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundExpression,
        receiver: Option<StorageAccessId>,
        projection: Option<StorageProjection>,
    ) -> Result<Option<StorageAccessId>, StorageAnalysisError> {
        let access = match (receiver, projection) {
            (Some(receiver), Some(projection)) => {
                let Some(receiver) = self.builder.access(receiver) else {
                    return Err(CheckedStorageFactsBuildError::MissingAccess.into());
                };

                let mut projections = receiver.projections().to_vec();
                projections.push(projection);

                self.builder.push_access(StorageAccess::new(
                    receiver.root(),
                    projections,
                    expression.ty().unwrap_or(self.error_type),
                    expression.origin().source_anchor(),
                    expression.is_recovered() || expression.ty().is_none(),
                ))?
            }
            _ => self.recovery_access(id)?,
        };

        self.builder
            .record_occurrence_access(StorageAccessOccurrence::Projection(id), access)?;
        self.expression_accesses.insert(id, access);

        Ok(Some(access))
    }

    fn parameter_identity(
        &mut self,
        parameter: StorageParameter,
    ) -> Result<StorageIdentityId, StorageAnalysisError> {
        if let Some(storage) = self.parameter_storage.get(&parameter).copied() {
            return Ok(storage);
        }

        let identity = match parameter {
            StorageParameter::Callable(parameter) => StorageIdentity::Parameter(parameter),
            StorageParameter::Receiver(receiver) => StorageIdentity::Receiver(receiver),
            StorageParameter::Anonymous(parameter) => {
                StorageIdentity::AnonymousParameter(parameter)
            }
        };
        let storage = self.builder.push_identity(identity)?;

        self.builder.record_parameter_storage(parameter, storage)?;
        self.parameter_storage.insert(parameter, storage);

        Ok(storage)
    }

    fn surface_identity(
        &mut self,
        surface: SurfaceStorageSymbol,
    ) -> Result<StorageIdentityId, StorageAnalysisError> {
        if let Some(storage) = self.surface_storage.get(&surface).copied() {
            return Ok(storage);
        }

        let storage = self
            .builder
            .push_identity(StorageIdentity::Surface(surface))?;

        self.builder
            .record_surface_storage(surface, StorageReferent::Identity(storage))?;
        self.surface_storage.insert(surface, storage);

        Ok(storage)
    }

    fn direct_access(
        &mut self,
        id: BoundExpressionId,
        storage: StorageIdentityId,
    ) -> Result<StorageAccessId, StorageAnalysisError> {
        let expression = self
            .request
            .view()
            .expression(id)
            .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node: id.into() })?;

        Ok(self.builder.push_access(StorageAccess::new(
            StorageAccessRoot::Storage(storage),
            [],
            expression.ty().unwrap_or(self.error_type),
            expression.origin().source_anchor(),
            expression.is_recovered() || expression.ty().is_none(),
        ))?)
    }

    fn recovery_access(
        &mut self,
        id: BoundExpressionId,
    ) -> Result<StorageAccessId, StorageAnalysisError> {
        let expression = self
            .request
            .view()
            .expression(id)
            .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node: id.into() })?;
        let source = expression.origin().source_anchor();
        let storage = self.builder.push_identity(StorageIdentity::Error(source))?;

        Ok(self.builder.push_access(StorageAccess::new(
            StorageAccessRoot::Recovery(storage),
            [],
            expression.ty().unwrap_or(self.error_type),
            source,
            true,
        ))?)
    }

    fn observe_cancellation(&self) -> Result<(), StorageAnalysisError> {
        if self.request.is_cancelled() {
            Err(StorageAnalysisError::Cancelled)
        } else {
            Ok(())
        }
    }
}

const fn projection_kind(kind: BoundStructuredExpressionKind) -> bool {
    matches!(
        kind,
        BoundStructuredExpressionKind::ElementIndex
            | BoundStructuredExpressionKind::SliceIndex
            | BoundStructuredExpressionKind::NullablePropagation
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundAssignmentExpression, BoundBlock, BoundBlockItem, BoundCallableBody,
        BoundErrorExpression, BoundExpression, BoundNameExpression, BoundNodeOrigin, BoundOperator,
        BoundReferenceTarget, BoundTree, BoundTreeBuilder, BoundUnitId, StorageAccessOccurrence,
        StorageParameter,
    };
    use bray_symbols::{CallableParameterSymbolId, SymbolId};

    use crate::test_support::{
        available_compiler_known_symbols, callable_key, error_type, semantic_values,
    };
    use crate::{
        CheckerOutcome, DefaultStorageChecker, StorageChecker, UnitCheckRequest, UnitCheckRoot,
    };

    #[test]
    fn storage_checking_publishes_parameter_accesses_with_owned_diagnostics() {
        let unit = BoundUnitId::new(11);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let (tree, root, expression) = parameter_tree(unit, parameter);
        let key = callable_key();
        let cancellation = || false;
        let request = request(tree.view(&key), root, &cancellation);

        let outcome = match DefaultStorageChecker.check_storage(request) {
            Ok(outcome) => outcome,
            Err(error) => panic!("storage checking must complete: {error:?}"),
        };
        let CheckerOutcome::Complete(result) = outcome else {
            panic!("uncancelled storage checking must publish");
        };

        assert!(result.diagnostics().is_empty());
        assert_eq!(result.value().unit(), unit);

        let facts = result.value().clone().into_facts();

        assert!(
            facts
                .parameter_storage(StorageParameter::Callable(parameter))
                .is_some()
        );
        assert!(
            facts
                .occurrence_access(StorageAccessOccurrence::Read(expression))
                .is_some()
        );
    }

    #[test]
    fn storage_checking_discards_partial_state_after_cancellation() {
        let unit = BoundUnitId::new(12);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let (tree, root, _) = parameter_tree(unit, parameter);
        let key = callable_key();
        let cancellation = || true;
        let request = request(tree.view(&key), root, &cancellation);

        let outcome = DefaultStorageChecker.check_storage(request);

        assert_eq!(outcome, Ok(CheckerOutcome::Cancelled));
    }

    #[test]
    fn storage_checking_recovers_assignment_facts_for_an_invalid_destination() {
        let unit = BoundUnitId::new(13);
        let (tree, root, destination, assignment) = recovered_assignment_tree(unit);
        let key = callable_key();
        let cancellation = || false;
        let request = request(tree.view(&key), root, &cancellation);

        let outcome = match DefaultStorageChecker.check_storage(request) {
            Ok(outcome) => outcome,
            Err(error) => panic!("recovered assignment checking must complete: {error:?}"),
        };
        let CheckerOutcome::Complete(result) = outcome else {
            panic!("recovered assignment checking must publish");
        };
        let facts = result.value().clone().into_facts();
        let Some(access) = facts.occurrence_access(StorageAccessOccurrence::Write(destination))
        else {
            panic!("the recovered destination must retain its write access");
        };

        assert_eq!(
            facts.occurrence_access(StorageAccessOccurrence::Assignment(assignment)),
            Some(access)
        );
        assert!(
            facts
                .access(access)
                .is_some_and(|access| access.is_recovered())
        );
    }

    fn request<'view>(
        view: bray_bound_tree::BoundUnitView<'view>,
        root: bray_bound_tree::BoundCallableBodyId,
        cancellation: &'view dyn bray_base::Cancellation,
    ) -> UnitCheckRequest<'view> {
        match UnitCheckRequest::new(
            view,
            UnitCheckRoot::CallableBody(root),
            semantic_values(),
            available_compiler_known_symbols(),
            cancellation,
        ) {
            Ok(request) => request,
            Err(error) => panic!("matching storage test request must validate: {error:?}"),
        }
    }

    fn parameter_tree(
        unit: BoundUnitId,
        parameter: CallableParameterSymbolId,
    ) -> (
        BoundTree,
        bray_bound_tree::BoundCallableBodyId,
        bray_bound_tree::BoundExpressionId,
    ) {
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut builder = BoundTreeBuilder::new(unit);
        let name = BoundExpression::Name(BoundNameExpression::new(
            origin,
            BoundReferenceTarget::Surface(parameter.into()),
            Some(error_type()),
            false,
        ));
        let expression = match builder.push_expression(name) {
            Ok(expression) => expression,
            Err(error) => panic!("parameter expression must fit: {error:?}"),
        };
        let block = match builder.push_block(BoundBlock::new(
            origin,
            [BoundBlockItem::Expression(expression)],
            false,
        )) {
            Ok(block) => block,
            Err(error) => panic!("parameter block must fit: {error:?}"),
        };
        let root = match builder.push_callable_body(BoundCallableBody::block(origin, block)) {
            Ok(root) => root,
            Err(error) => panic!("parameter callable body must fit: {error:?}"),
        };

        (builder.finish(), root, expression)
    }

    fn recovered_assignment_tree(
        unit: BoundUnitId,
    ) -> (
        BoundTree,
        bray_bound_tree::BoundCallableBodyId,
        bray_bound_tree::BoundExpressionId,
        bray_bound_tree::BoundExpressionId,
    ) {
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut builder = BoundTreeBuilder::new(unit);
        let destination = match builder.push_expression(BoundExpression::Error(
            BoundErrorExpression::new(origin, error_type()),
        )) {
            Ok(destination) => destination,
            Err(error) => panic!("assignment destination must fit: {error:?}"),
        };
        let assignment = match builder.push_expression(BoundExpression::Assignment(
            BoundAssignmentExpression::new(
                origin,
                BoundOperator::Assign,
                [destination],
                Some(error_type()),
                true,
            ),
        )) {
            Ok(assignment) => assignment,
            Err(error) => panic!("assignment expression must fit: {error:?}"),
        };
        let block = match builder.push_block(BoundBlock::new(
            origin,
            [BoundBlockItem::Expression(assignment)],
            true,
        )) {
            Ok(block) => block,
            Err(error) => panic!("assignment block must fit: {error:?}"),
        };
        let root = match builder.push_callable_body(BoundCallableBody::block(origin, block)) {
            Ok(root) => root,
            Err(error) => panic!("assignment callable body must fit: {error:?}"),
        };

        (builder.finish(), root, destination, assignment)
    }
}
