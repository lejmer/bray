use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{
    BoundExpressionId, BoundPatternId, DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
};
use bray_symbols::{ConstantTermId, ConstantValueId, TypeExpressionTemplate, TypeId};

use crate::CheckerPatternInputFailure;

/// The selected element type supplied to one iteration pattern.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IterationPatternType {
    pattern: BoundPatternId,
    element_type: TypeId,
    is_recovered: bool,
}

impl IterationPatternType {
    /// Creates one iteration-pattern type input.
    pub const fn new(pattern: BoundPatternId, element_type: TypeId, is_recovered: bool) -> Self {
        Self {
            pattern,
            element_type,
            is_recovered,
        }
    }

    /// Returns the exact iteration pattern.
    pub const fn pattern(self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the selected iteration element type.
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// Returns whether iteration selection recovered.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// A checked constant value selected by one path pattern.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatternConstantEvidence {
    pattern: BoundPatternId,
    ty: TypeId,
    term: ConstantTermId,
}

impl PatternConstantEvidence {
    /// Creates exact constant evidence for one pattern occurrence.
    pub const fn new(pattern: BoundPatternId, ty: TypeId, term: ConstantTermId) -> Self {
        Self { pattern, ty, term }
    }

    /// Returns the exact path pattern.
    pub const fn pattern(self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the checked constant type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns the checked open or closed constant term.
    pub const fn term(self) -> ConstantTermId {
        self.term
    }
}

/// A checked constant value produced by one match guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuardConstantEvidence {
    guard: BoundExpressionId,
    value: ConstantValueId,
}

impl GuardConstantEvidence {
    /// Creates exact constant evidence for one guard expression.
    pub const fn new(guard: BoundExpressionId, value: ConstantValueId) -> Self {
        Self { guard, value }
    }

    /// Returns the exact guard expression.
    pub const fn guard(self) -> BoundExpressionId {
        self.guard
    }

    /// Returns the checked constant value.
    pub const fn value(self) -> ConstantValueId {
        self.value
    }
}

/// Contextual types unavailable from the bound unit alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternCheckInput {
    iteration_patterns: Arc<[IterationPatternType]>,
    declared_patterns: BTreeMap<BoundPatternId, TypeExpressionTemplate>,
    constant_patterns: BTreeMap<BoundPatternId, PatternConstantEvidence>,
    constant_guards: BTreeMap<BoundExpressionId, ConstantValueId>,
    failure: Option<CheckerPatternInputFailure>,
}

impl Default for PatternCheckInput {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternCheckInput {
    /// Creates an empty pattern-checking input.
    pub fn new() -> Self {
        Self {
            iteration_patterns: Arc::from([]),
            declared_patterns: BTreeMap::new(),
            constant_patterns: BTreeMap::new(),
            constant_guards: BTreeMap::new(),
            failure: None,
        }
    }

    /// Returns this input with selected iteration element types.
    pub fn with_iteration_patterns(
        mut self,
        patterns: impl IntoIterator<Item = IterationPatternType>,
    ) -> Self {
        self.iteration_patterns = shared_slice(patterns);

        self
    }

    /// Returns this input with source-declared pattern types.
    pub fn with_declared_pattern_types(mut self, declared: &DeclaredValueTypeTemplates) -> Self {
        for evidence in declared.evidence() {
            let DeclaredValueTypeTerm::Pattern(pattern) = evidence.term() else {
                continue;
            };

            // The check input owns templates independently of the borrowed declaration query.
            let template = evidence.template().clone();

            if self
                .declared_patterns
                .insert(pattern, template)
                .is_some_and(|current| current != *evidence.template())
            {
                self.failure.get_or_insert(
                    CheckerPatternInputFailure::ConflictingDeclaredPattern { pattern },
                );
            }
        }

        self
    }

    /// Returns this input with evaluated path-pattern constants.
    pub fn with_constant_patterns(
        mut self,
        patterns: impl IntoIterator<Item = PatternConstantEvidence>,
    ) -> Self {
        for evidence in patterns {
            if self
                .constant_patterns
                .insert(evidence.pattern(), evidence)
                .is_some_and(|current| current != evidence)
            {
                self.failure.get_or_insert(
                    CheckerPatternInputFailure::ConflictingConstantPattern {
                        pattern: evidence.pattern(),
                    },
                );
            }
        }

        self
    }

    /// Returns this input with evaluated match guards.
    pub fn with_constant_guards(
        mut self,
        guards: impl IntoIterator<Item = GuardConstantEvidence>,
    ) -> Self {
        for evidence in guards {
            if self
                .constant_guards
                .insert(evidence.guard(), evidence.value())
                .is_some_and(|current| current != evidence.value())
            {
                self.failure
                    .get_or_insert(CheckerPatternInputFailure::ConflictingGuard {
                        expression: evidence.guard(),
                    });
            }
        }

        self
    }

    pub(super) fn iteration_patterns(&self) -> &[IterationPatternType] {
        &self.iteration_patterns
    }

    pub(super) fn declared_patterns(&self) -> &BTreeMap<BoundPatternId, TypeExpressionTemplate> {
        &self.declared_patterns
    }

    pub(super) fn constant_patterns(&self) -> &BTreeMap<BoundPatternId, PatternConstantEvidence> {
        &self.constant_patterns
    }

    pub(super) fn constant_guards(&self) -> &BTreeMap<BoundExpressionId, ConstantValueId> {
        &self.constant_guards
    }

    pub(super) const fn failure(&self) -> Option<CheckerPatternInputFailure> {
        self.failure
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitId;
    use bray_symbols::{ConstantValueData, ConstantValueKind};

    use super::{GuardConstantEvidence, PatternCheckInput};
    use crate::CheckerPatternInputFailure;
    use crate::test_support::{
        error_type, expression_unit, integer_literal_expression, push_expression, semantic_values,
    };

    #[test]
    fn conflicting_constant_evidence_is_rejected_deterministically() {
        let values = semantic_values();
        let ty = error_type();

        let (_, expressions) = expression_unit(BoundUnitId::new(1), |tree, origin| {
            vec![push_expression(
                tree,
                integer_literal_expression(origin, None),
            )]
        });

        let [guard] = expressions.as_slice() else {
            panic!("test unit must contain one expression");
        };

        let first_value = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Boolean(false),
            ))
            .unwrap_or_else(|error| panic!("first test constant must intern: {error:?}"));

        let second_value = values
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(true)))
            .unwrap_or_else(|error| panic!("second test constant must intern: {error:?}"));

        let input = PatternCheckInput::new().with_constant_guards([
            GuardConstantEvidence::new(*guard, first_value),
            GuardConstantEvidence::new(*guard, second_value),
        ]);

        assert_eq!(
            input.failure(),
            Some(CheckerPatternInputFailure::ConflictingGuard { expression: *guard })
        );
    }
}
