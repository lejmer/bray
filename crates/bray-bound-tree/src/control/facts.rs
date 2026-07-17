use std::sync::Arc;

use crate::{BoundExpressionId, BoundPatternId, BoundUnitId, BoundUnitKind, BoundUnitView};

use super::validation::{
    reject_duplicate_block_results, reject_duplicate_control_transfers,
    reject_duplicate_for_iterations, reject_duplicate_matches, reject_duplicate_patterns,
    validate_block_result, validate_control_transfer, validate_for_iteration, validate_match,
    validate_pattern,
};
use super::{
    CheckedBlockResult, CheckedControlTransfer, CheckedForIterationFacts, CheckedMatchFacts,
    CheckedPatternFacts, ControlCompletion,
};

/// A contract violation while publishing lowering-facing checked control facts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckedControlFlowFactsBuildError {
    /// The supplied bound-unit view does not match the builder's unit.
    ForeignUnitView {
        /// The unit owned by the builder.
        expected: BoundUnitId,
        /// The unit owned by the supplied view.
        actual: BoundUnitId,
    },
    /// A transfer fact references another unit.
    ForeignControlTransfer(BoundExpressionId),
    /// A block-result fact references another unit.
    ForeignBlockResult(crate::BoundBlockId),
    /// A match fact references another unit.
    ForeignMatch(BoundExpressionId),
    /// A pattern fact references another unit.
    ForeignPattern(BoundPatternId),
    /// A for-iteration fact references another unit.
    ForeignForIteration(BoundExpressionId),
    /// More than one transfer fact names the same expression.
    DuplicateControlTransfer(BoundExpressionId),
    /// More than one result fact names the same block.
    DuplicateBlockResult(crate::BoundBlockId),
    /// More than one coverage fact names the same match expression.
    DuplicateMatch(BoundExpressionId),
    /// More than one semantic fact names the same pattern.
    DuplicatePattern(BoundPatternId),
    /// More than one protocol fact names the same for expression.
    DuplicateForIteration(BoundExpressionId),
    /// A transfer fact does not name a control-transfer expression.
    InvalidControlTransfer(BoundExpressionId),
    /// A transfer target does not match the source transfer category.
    InvalidControlTransferTarget(BoundExpressionId),
    /// A block-result fact names a missing block or invalid owner.
    InvalidBlockResult(crate::BoundBlockId),
    /// A coverage fact does not name a match expression.
    InvalidMatch(BoundExpressionId),
    /// A pattern fact names a missing pattern or invalid child projection list.
    InvalidPattern(BoundPatternId),
    /// A protocol fact does not name a for expression.
    InvalidForIteration(BoundExpressionId),
}

/// Local mutable construction state for immutable checked control facts.
#[derive(Debug)]
pub struct CheckedControlFlowFactsBuilder {
    unit: BoundUnitId,
    control_transfers: Vec<CheckedControlTransfer>,
    block_results: Vec<CheckedBlockResult>,
    matches: Vec<CheckedMatchFacts>,
    patterns: Vec<CheckedPatternFacts>,
    for_iterations: Vec<CheckedForIterationFacts>,
}

impl CheckedControlFlowFactsBuilder {
    /// Starts fact construction for one exact checked semantic unit.
    pub const fn new(unit: BoundUnitId) -> Self {
        Self {
            unit,
            control_transfers: Vec::new(),
            block_results: Vec::new(),
            matches: Vec::new(),
            patterns: Vec::new(),
            for_iterations: Vec::new(),
        }
    }

    /// Records one exact control-transfer target.
    pub fn push_control_transfer(&mut self, fact: CheckedControlTransfer) {
        self.control_transfers.push(fact);
    }

    /// Records one exact block-result role.
    pub fn push_block_result(&mut self, fact: CheckedBlockResult) {
        self.block_results.push(fact);
    }

    /// Records one exact match coverage decision.
    pub fn push_match(&mut self, fact: CheckedMatchFacts) {
        self.matches.push(fact);
    }

    /// Records one exact pattern decision.
    pub fn push_pattern(&mut self, fact: CheckedPatternFacts) {
        self.patterns.push(fact);
    }

    /// Records one exact selected or recovered for-iteration protocol.
    pub fn push_for_iteration(&mut self, fact: CheckedForIterationFacts) {
        self.for_iterations.push(fact);
    }

    /// Validates, canonicalizes, and freezes facts for the supplied bound-unit view.
    pub fn finish(
        mut self,
        view: BoundUnitView<'_>,
        completion: ControlCompletion,
    ) -> Result<CheckedControlFlowFacts, CheckedControlFlowFactsBuildError> {
        if view.unit() != self.unit {
            return Err(CheckedControlFlowFactsBuildError::ForeignUnitView {
                expected: self.unit,
                actual: view.unit(),
            });
        }

        self.validate_records(view)?;
        self.sort_and_reject_duplicates()?;

        Ok(CheckedControlFlowFacts {
            unit: self.unit,
            kind: view.kind(),
            completion,
            control_transfers: self.control_transfers.into(),
            block_results: self.block_results.into(),
            matches: self.matches.into(),
            patterns: self.patterns.into(),
            for_iterations: self.for_iterations.into(),
        })
    }

    fn validate_records(
        &self,
        view: BoundUnitView<'_>,
    ) -> Result<(), CheckedControlFlowFactsBuildError> {
        for fact in &self.control_transfers {
            validate_control_transfer(self.unit, view, *fact)?;
        }

        for fact in &self.block_results {
            validate_block_result(self.unit, view, *fact)?;
        }

        for fact in &self.matches {
            validate_match(self.unit, view, *fact)?;
        }

        for fact in &self.patterns {
            validate_pattern(self.unit, view, fact)?;
        }

        for fact in &self.for_iterations {
            validate_for_iteration(self.unit, view, fact)?;
        }

        Ok(())
    }

    fn sort_and_reject_duplicates(&mut self) -> Result<(), CheckedControlFlowFactsBuildError> {
        self.control_transfers.sort_by_key(|fact| fact.expression());
        reject_duplicate_control_transfers(&self.control_transfers)?;

        self.block_results.sort_by_key(|fact| fact.block());
        reject_duplicate_block_results(&self.block_results)?;

        self.matches.sort_by_key(|fact| fact.expression());
        reject_duplicate_matches(&self.matches)?;

        self.patterns.sort_by_key(CheckedPatternFacts::pattern);
        reject_duplicate_patterns(&self.patterns)?;

        self.for_iterations
            .sort_by_key(CheckedForIterationFacts::expression);
        reject_duplicate_for_iterations(&self.for_iterations)
    }
}

/// Durable lowering-facing control, pattern, and iteration facts for one exact checked unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedControlFlowFacts {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    completion: ControlCompletion,
    control_transfers: Arc<[CheckedControlTransfer]>,
    block_results: Arc<[CheckedBlockResult]>,
    matches: Arc<[CheckedMatchFacts]>,
    patterns: Arc<[CheckedPatternFacts]>,
    for_iterations: Arc<[CheckedForIterationFacts]>,
}

impl CheckedControlFlowFacts {
    /// Returns the exact bound unit these facts describe.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns the unit's checked completion categories.
    pub const fn completion(&self) -> ControlCompletion {
        self.completion
    }

    /// Returns whether conservative recovery affected control-flow checking.
    pub fn is_recovered(&self) -> bool {
        self.completion
            .contains(super::ControlCompletionKind::Recovered)
            || self
                .control_transfers
                .iter()
                .any(|fact| fact.target() == super::CheckedControlTransferTarget::Recovered)
            || self
                .block_results
                .iter()
                .any(|fact| fact.role() == super::CheckedBlockResultRole::Recovered)
            || self
                .matches
                .iter()
                .any(|fact| fact.exhaustiveness() == super::MatchExhaustiveness::Recovered)
            || self.patterns.iter().any(|fact| {
                matches!(
                    fact.resolution(),
                    super::CheckedPatternResolution::Recovered
                )
            })
            || self.for_iterations.iter().any(|fact| {
                matches!(
                    fact.resolution(),
                    super::CheckedForIterationResolution::Recovered
                )
            })
    }

    /// Returns transfer facts in bound-expression identity order.
    pub fn control_transfers(&self) -> &[CheckedControlTransfer] {
        &self.control_transfers
    }

    /// Returns the target fact for one exact control-transfer expression.
    pub fn control_transfer(
        &self,
        expression: BoundExpressionId,
    ) -> Option<CheckedControlTransfer> {
        find_by_key(&self.control_transfers, expression, |fact| {
            fact.expression()
        })
        .copied()
    }

    /// Returns block-result facts in bound-block identity order.
    pub fn block_results(&self) -> &[CheckedBlockResult] {
        &self.block_results
    }

    /// Returns the result role for one exact source-shaped block.
    pub fn block_result(&self, block: crate::BoundBlockId) -> Option<CheckedBlockResult> {
        find_by_key(&self.block_results, block, |fact| fact.block()).copied()
    }

    /// Returns match coverage facts in bound-expression identity order.
    pub fn matches(&self) -> &[CheckedMatchFacts] {
        &self.matches
    }

    /// Returns the coverage fact for one exact match expression.
    pub fn match_facts(&self, expression: BoundExpressionId) -> Option<CheckedMatchFacts> {
        find_by_key(&self.matches, expression, |fact| fact.expression()).copied()
    }

    /// Returns pattern facts in bound-pattern identity order.
    pub fn patterns(&self) -> &[CheckedPatternFacts] {
        &self.patterns
    }

    /// Returns semantic facts for one exact pattern.
    pub fn pattern(&self, pattern: BoundPatternId) -> Option<&CheckedPatternFacts> {
        find_by_key(&self.patterns, pattern, CheckedPatternFacts::pattern)
    }

    /// Returns for-iteration facts in bound-expression identity order.
    pub fn for_iterations(&self) -> &[CheckedForIterationFacts] {
        &self.for_iterations
    }

    /// Returns selected protocol facts for one exact for expression.
    pub fn for_iteration(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&CheckedForIterationFacts> {
        find_by_key(
            &self.for_iterations,
            expression,
            CheckedForIterationFacts::expression,
        )
    }
}

fn find_by_key<T, K: Ord>(items: &[T], key: K, item_key: impl Fn(&T) -> K) -> Option<&T> {
    items
        .binary_search_by_key(&key, item_key)
        .ok()
        .and_then(|index| items.get(index))
}

#[cfg(test)]
mod tests {
    use crate::test_support::{error_type, source_anchor, symbol_key};
    use crate::{
        BoundBlock, BoundBlockItem, BoundCallableBody, BoundControlTransferExpression,
        BoundControlTransferKind, BoundExpression, BoundNodeOrigin, BoundPattern, BoundPatternKind,
        BoundPatternMode, BoundTreeBuilder, BoundUnitId, BoundUnitKey, CheckedBlockResult,
        CheckedBlockResultRole, CheckedControlFlowFactsBuildError, CheckedControlFlowFactsBuilder,
        CheckedControlTransfer, CheckedControlTransferTarget, CheckedPatternFacts,
        ControlCompletion, PatternTest,
    };
    use bray_symbols::SymbolKind;

    #[test]
    fn publication_canonicalizes_exact_transfer_and_block_facts() {
        let unit = BoundUnitId::new(7);
        let source = source_anchor();
        let origin = BoundNodeOrigin::source(source);
        let mut tree = BoundTreeBuilder::new(unit);

        let first = push_return(&mut tree, origin);
        let second = push_return(&mut tree, origin);
        let block = push_block(&mut tree, origin, [first, second]);
        let body = push_callable(&mut tree, origin, block);
        let key = callable_key(source);
        let tree = tree.finish();

        let mut facts = CheckedControlFlowFactsBuilder::new(unit);

        facts.push_control_transfer(CheckedControlTransfer::new(
            second,
            CheckedControlTransferTarget::Callable(body),
        ));

        facts.push_control_transfer(CheckedControlTransfer::new(
            first,
            CheckedControlTransferTarget::Callable(body),
        ));

        facts.push_block_result(CheckedBlockResult::new(
            block,
            CheckedBlockResultRole::CallableBody(body),
        ));

        let facts = match facts.finish(tree.view(&key), ControlCompletion::default()) {
            Ok(facts) => facts,
            Err(error) => panic!("valid control facts must publish: {error:?}"),
        };

        assert_eq!(
            facts.control_transfers(),
            &[
                CheckedControlTransfer::new(first, CheckedControlTransferTarget::Callable(body)),
                CheckedControlTransfer::new(second, CheckedControlTransferTarget::Callable(body)),
            ]
        );

        assert_eq!(
            facts.block_result(block).map(|fact| fact.role()),
            Some(CheckedBlockResultRole::CallableBody(body))
        );
    }

    #[test]
    fn publication_rejects_foreign_records_and_incomplete_child_projections() {
        let unit = BoundUnitId::new(8);
        let source = source_anchor();
        let origin = BoundNodeOrigin::source(source);
        let mut tree = BoundTreeBuilder::new(unit);
        let child = push_binding_pattern(&mut tree, origin);

        let parent = match tree.push_pattern(BoundPattern::new(
            origin,
            error_type(),
            BoundPatternMode::Match,
            BoundPatternKind::Grouped,
            [child],
            [],
        )) {
            Ok(pattern) => pattern,
            Err(error) => panic!("test parent pattern must fit: {error:?}"),
        };

        let block = push_block(&mut tree, origin, []);
        let _body = push_callable(&mut tree, origin, block);
        let key = callable_key(source);
        let tree = tree.finish();

        let mut missing_projection = CheckedControlFlowFactsBuilder::new(unit);

        missing_projection.push_pattern(CheckedPatternFacts::resolved(
            parent,
            true,
            PatternTest::Always,
            [],
        ));

        assert_eq!(
            missing_projection.finish(tree.view(&key), ControlCompletion::default()),
            Err(CheckedControlFlowFactsBuildError::InvalidPattern(parent))
        );

        let mut recovered_pattern = CheckedControlFlowFactsBuilder::new(unit);
        recovered_pattern.push_pattern(CheckedPatternFacts::recovered(parent));
        let recovered_pattern =
            match recovered_pattern.finish(tree.view(&key), ControlCompletion::default()) {
                Ok(facts) => facts,
                Err(error) => panic!("recovered pattern facts must validate: {error:?}"),
            };

        assert!(recovered_pattern.is_recovered());

        let foreign = crate::BoundExpressionId::from_slot(BoundUnitId::new(9), 0);
        let mut foreign_fact = CheckedControlFlowFactsBuilder::new(unit);

        foreign_fact.push_control_transfer(CheckedControlTransfer::new(
            foreign,
            CheckedControlTransferTarget::Recovered,
        ));

        assert_eq!(
            foreign_fact.finish(tree.view(&key), ControlCompletion::default()),
            Err(CheckedControlFlowFactsBuildError::ForeignControlTransfer(
                foreign
            ))
        );
    }

    #[test]
    fn publication_rejects_a_block_role_owned_by_another_block() {
        let unit = BoundUnitId::new(10);
        let source = source_anchor();
        let origin = BoundNodeOrigin::source(source);
        let mut tree = BoundTreeBuilder::new(unit);
        let body_block = push_block(&mut tree, origin, []);
        let unrelated_block = push_block(&mut tree, origin, []);
        let body = push_callable(&mut tree, origin, body_block);
        let key = callable_key(source);
        let tree = tree.finish();
        let mut facts = CheckedControlFlowFactsBuilder::new(unit);

        facts.push_block_result(CheckedBlockResult::new(
            unrelated_block,
            CheckedBlockResultRole::CallableBody(body),
        ));

        assert_eq!(
            facts.finish(tree.view(&key), ControlCompletion::default()),
            Err(CheckedControlFlowFactsBuildError::InvalidBlockResult(
                unrelated_block
            ))
        );

        assert_eq!(
            CheckedControlFlowFactsBuilder::new(BoundUnitId::new(12))
                .finish(tree.view(&key), ControlCompletion::default()),
            Err(CheckedControlFlowFactsBuildError::ForeignUnitView {
                expected: BoundUnitId::new(12),
                actual: unit,
            })
        );
    }

    fn callable_key(source: crate::BoundSourceAnchor) -> BoundUnitKey {
        let owner = symbol_key(SymbolKind::Function, 0);

        match BoundUnitKey::callable_body(owner, source) {
            Some(key) => key,
            None => panic!("function symbols must support callable-body units"),
        }
    }

    fn push_return(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> crate::BoundExpressionId {
        let expression = BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
            origin,
            BoundControlTransferKind::Return,
            None,
            None,
            Some(error_type()),
            false,
        ));

        match tree.push_expression(expression) {
            Ok(expression) => expression,
            Err(error) => panic!("test return expression must fit: {error:?}"),
        }
    }

    fn push_binding_pattern(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> crate::BoundPatternId {
        let pattern = BoundPattern::new(
            origin,
            error_type(),
            BoundPatternMode::Match,
            BoundPatternKind::Binding,
            [],
            [],
        );

        match tree.push_pattern(pattern) {
            Ok(pattern) => pattern,
            Err(error) => panic!("test binding pattern must fit: {error:?}"),
        }
    }

    fn push_block(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        expressions: impl IntoIterator<Item = crate::BoundExpressionId>,
    ) -> crate::BoundBlockId {
        let items = expressions.into_iter().map(BoundBlockItem::Expression);

        match tree.push_block(BoundBlock::new(origin, items, false)) {
            Ok(block) => block,
            Err(error) => panic!("test block must fit: {error:?}"),
        }
    }

    fn push_callable(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        block: crate::BoundBlockId,
    ) -> crate::BoundCallableBodyId {
        match tree.push_callable_body(BoundCallableBody::block(origin, block)) {
            Ok(body) => body,
            Err(error) => panic!("test callable body must fit: {error:?}"),
        }
    }
}
