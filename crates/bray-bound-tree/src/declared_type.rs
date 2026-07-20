use std::sync::Arc;

use bray_symbols::TypeExpressionTemplate;

use crate::{BoundExpressionId, BoundPatternId, BoundReferenceTarget, BoundUnitId, BoundUnitKind};

/// One semantic subject participating in declared value-type evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclaredValueTypeTerm {
    /// One source-correlated expression occurrence.
    Expression(BoundExpressionId),
    /// One source-correlated pattern occurrence.
    Pattern(BoundPatternId),
    /// One local or compilation-wide value identity.
    Value(BoundReferenceTarget),
}

/// A source-declared type template attached to one semantic subject.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclaredValueTypeEvidence {
    term: DeclaredValueTypeTerm,
    template: TypeExpressionTemplate,
}

impl DeclaredValueTypeEvidence {
    /// Creates source-declared type evidence without resolving embedded constants.
    pub const fn new(term: DeclaredValueTypeTerm, template: TypeExpressionTemplate) -> Self {
        Self { term, template }
    }

    /// Returns the semantic subject constrained by this declaration.
    pub const fn term(&self) -> DeclaredValueTypeTerm {
        self.term
    }

    /// Returns the source type-expression template.
    pub const fn template(&self) -> &TypeExpressionTemplate {
        &self.template
    }
}

/// The source relationship establishing one declared value-type equality.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclaredValueTypeConstraintKind {
    /// A declaration initializer has the declared subject's type.
    Initializer,
    /// A pattern binding receives the type selected for its pattern occurrence.
    PatternBinding,
    /// A name expression has the type of the exact value it references.
    DefinitionUse,
}

/// An equality between two subjects whose types are determined together.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclaredValueTypeConstraint {
    kind: DeclaredValueTypeConstraintKind,
    left: DeclaredValueTypeTerm,
    right: DeclaredValueTypeTerm,
}

impl DeclaredValueTypeConstraint {
    /// Creates one source-correlated value-type equality.
    pub const fn new(
        kind: DeclaredValueTypeConstraintKind,
        left: DeclaredValueTypeTerm,
        right: DeclaredValueTypeTerm,
    ) -> Self {
        Self { kind, left, right }
    }

    /// Returns the source relationship that established this equality.
    pub const fn kind(self) -> DeclaredValueTypeConstraintKind {
        self.kind
    }

    /// Returns the first equality subject.
    pub const fn left(self) -> DeclaredValueTypeTerm {
        self.left
    }

    /// Returns the second equality subject.
    pub const fn right(self) -> DeclaredValueTypeTerm {
        self.right
    }
}

/// Immutable declared value-type inputs for one bound semantic unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredValueTypeTemplates {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    evidence: Arc<[DeclaredValueTypeEvidence]>,
    constraints: Arc<[DeclaredValueTypeConstraint]>,
    callable_result: Option<TypeExpressionTemplate>,
}

impl DeclaredValueTypeTemplates {
    /// Creates a fact in canonical term and constraint order.
    pub fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        evidence: impl IntoIterator<Item = DeclaredValueTypeEvidence>,
        constraints: impl IntoIterator<Item = DeclaredValueTypeConstraint>,
        callable_result: Option<TypeExpressionTemplate>,
    ) -> Self {
        let mut evidence = evidence.into_iter().collect::<Vec<_>>();
        evidence.sort_unstable();
        evidence.dedup();

        let mut constraints = constraints.into_iter().collect::<Vec<_>>();
        constraints.sort_unstable();
        constraints.dedup();

        Self {
            unit,
            kind,
            evidence: evidence.into(),
            constraints: constraints.into(),
            callable_result,
        }
    }

    /// Returns the exact bound unit described by this fact.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns source-declared templates in canonical subject order.
    pub fn evidence(&self) -> &[DeclaredValueTypeEvidence] {
        &self.evidence
    }

    /// Returns source value-type equalities in canonical order.
    pub fn constraints(&self) -> &[DeclaredValueTypeConstraint] {
        &self.constraints
    }

    /// Returns the callable result expectation active for this unit, when any.
    pub const fn callable_result(&self) -> Option<&TypeExpressionTemplate> {
        self.callable_result.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::TypeExpressionTemplate;

    use super::{
        DeclaredValueTypeConstraint, DeclaredValueTypeConstraintKind, DeclaredValueTypeEvidence,
        DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
    };
    use crate::test_support::error_type;
    use crate::{BoundExpressionId, BoundPatternId, BoundUnitId, BoundUnitKind};

    #[test]
    fn declared_value_type_facts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DeclaredValueTypeTerm>();
        assert_send_sync::<DeclaredValueTypeEvidence>();
        assert_send_sync::<DeclaredValueTypeConstraint>();
        assert_send_sync::<DeclaredValueTypeTemplates>();
    }

    #[test]
    fn declared_value_type_facts_canonicalize_and_deduplicate_entries() {
        let unit = BoundUnitId::new(3);
        let expression = DeclaredValueTypeTerm::Expression(BoundExpressionId::from_slot(unit, 1));
        let pattern = DeclaredValueTypeTerm::Pattern(BoundPatternId::from_slot(unit, 0));
        let template = TypeExpressionTemplate::Resolved(error_type());
        let evidence = DeclaredValueTypeEvidence::new(pattern, template.clone());

        let constraint = DeclaredValueTypeConstraint::new(
            DeclaredValueTypeConstraintKind::Initializer,
            expression,
            pattern,
        );

        let facts = DeclaredValueTypeTemplates::new(
            unit,
            BoundUnitKind::CallableBody,
            [evidence.clone(), evidence.clone()],
            [constraint, constraint],
            Some(template.clone()),
        );

        assert_eq!(facts.evidence(), &[evidence]);
        assert_eq!(facts.constraints(), &[constraint]);
        assert_eq!(facts.callable_result(), Some(&template));
    }
}
