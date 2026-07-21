use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{BoundExpressionId, DeclaredValueTypeTerm, SelectionKind};
use bray_symbols::{
    CallableDefinitionId, CallableParameterSymbolId, CallableSignatureTemplate,
    GenericDeclarationTemplate, SymbolKey, TypeExpressionTemplate, UnevaluatedDefaultTemplate,
};

/// Why an exact semantic candidate request has no candidate surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CandidateAbsence {
    /// Name binding did not produce a usable callee identity.
    UnresolvedReference,
    /// The referenced value has no declared type template at this occurrence.
    MissingValueType,
    /// The referenced value's declared type is not callable.
    NonCallableValue,
    /// Every declaration reached through an overload arm was unusable.
    EmptyOverload,
}

/// Participation state known before type compatibility and target checks.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableCandidateTemplateState {
    /// The declaration is visible and contains no known recovery.
    Visible,
    /// The declaration exists but is not visible from the requesting context.
    Inaccessible,
    /// Prior syntax or name recovery contributed to this candidate.
    Recovered,
}

/// One declaration parameter and its unevaluated default surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableParameterDefaultTemplate {
    parameter: CallableParameterSymbolId,
    value: UnevaluatedDefaultTemplate,
}

impl CallableParameterDefaultTemplate {
    /// Creates one declaration parameter default entry.
    pub const fn new(
        parameter: CallableParameterSymbolId,
        value: UnevaluatedDefaultTemplate,
    ) -> Self {
        Self { parameter, value }
    }

    /// Returns the declaration parameter.
    pub const fn parameter(self) -> CallableParameterSymbolId {
        self.parameter
    }

    /// Returns the unevaluated default expression surface.
    pub const fn value(self) -> UnevaluatedDefaultTemplate {
        self.value
    }
}

/// One declared callable considered before substitution and applicability checking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableDeclarationCandidateTemplate {
    key: SymbolKey,
    definition: CallableDefinitionId,
    signature: CallableSignatureTemplate,
    generic: GenericDeclarationTemplate,
    defaults: Arc<[CallableParameterDefaultTemplate]>,
    state: CallableCandidateTemplateState,
}

impl CallableDeclarationCandidateTemplate {
    /// Creates a declared callable candidate with complete unevaluated signature inputs.
    pub fn new(
        key: SymbolKey,
        definition: CallableDefinitionId,
        signature: CallableSignatureTemplate,
        generic: GenericDeclarationTemplate,
        defaults: impl IntoIterator<Item = CallableParameterDefaultTemplate>,
        state: CallableCandidateTemplateState,
    ) -> Self {
        Self {
            key,
            definition,
            signature,
            generic,
            defaults: shared_slice(defaults),
            state,
        }
    }

    /// Returns the candidate's stable semantic key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the exact callable declaration.
    pub const fn definition(&self) -> CallableDefinitionId {
        self.definition
    }

    /// Returns the complete unevaluated callable signature.
    pub const fn signature(&self) -> &CallableSignatureTemplate {
        &self.signature
    }

    /// Returns generic parameters and constraint templates.
    pub const fn generic(&self) -> &GenericDeclarationTemplate {
        &self.generic
    }

    /// Returns parameter defaults in declaration order.
    pub fn defaults(&self) -> &[CallableParameterDefaultTemplate] {
        &self.defaults
    }

    /// Returns the participation state known before applicability checking.
    pub const fn state(&self) -> CallableCandidateTemplateState {
        self.state
    }
}

/// One callable value considered through its declared type template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableValueCandidateTemplate {
    value: DeclaredValueTypeTerm,
    callable_type: TypeExpressionTemplate,
    state: CallableCandidateTemplateState,
}

impl CallableValueCandidateTemplate {
    /// Creates a callable value candidate from exact declared type evidence.
    pub const fn new(
        value: DeclaredValueTypeTerm,
        callable_type: TypeExpressionTemplate,
        state: CallableCandidateTemplateState,
    ) -> Self {
        Self {
            value,
            callable_type,
            state,
        }
    }

    /// Returns the exact referenced value.
    pub const fn value(&self) -> DeclaredValueTypeTerm {
        self.value
    }

    /// Returns the value's callable type template.
    pub const fn callable_type(&self) -> &TypeExpressionTemplate {
        &self.callable_type
    }

    /// Returns the participation state known before applicability checking.
    pub const fn state(&self) -> CallableCandidateTemplateState {
        self.state
    }
}

/// One binder-enumerated callable candidate surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableCandidateTemplate {
    /// A callable declaration with exact signature facts.
    Declaration(CallableDeclarationCandidateTemplate),
    /// A callable value with exact declared type evidence.
    Value(CallableValueCandidateTemplate),
}

/// Candidate surfaces for one exact call expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallableCandidateTemplates {
    /// One or more candidates in deterministic semantic order.
    Present {
        /// The call expression that requested candidates.
        expression: BoundExpressionId,
        /// Candidate surfaces in deterministic semantic order.
        candidates: Arc<[CallableCandidateTemplate]>,
    },
    /// No truthful candidate surface was available.
    Absent {
        /// The call expression that requested candidates.
        expression: BoundExpressionId,
        /// Why candidate production could not provide a candidate.
        reason: CandidateAbsence,
    },
}

impl CallableCandidateTemplates {
    /// Creates a present candidate set when at least one candidate is supplied.
    pub fn present(
        expression: BoundExpressionId,
        candidates: impl IntoIterator<Item = CallableCandidateTemplate>,
    ) -> Option<Self> {
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();

        candidates.sort_unstable();
        candidates.dedup();

        (!candidates.is_empty()).then(|| Self::Present {
            expression,
            candidates: candidates.into(),
        })
    }

    /// Creates an explicit absence for one call expression.
    pub const fn absent(expression: BoundExpressionId, reason: CandidateAbsence) -> Self {
        Self::Absent { expression, reason }
    }

    /// Returns the call expression that requested these candidates.
    pub const fn expression(&self) -> BoundExpressionId {
        match self {
            Self::Present { expression, .. } | Self::Absent { expression, .. } => *expression,
        }
    }

    /// Returns the candidates when this set is present.
    pub fn candidates(&self) -> Option<&[CallableCandidateTemplate]> {
        match self {
            Self::Present { candidates, .. } => Some(candidates),
            Self::Absent { .. } => None,
        }
    }

    /// Returns the typed absence when no candidates were produced.
    pub const fn absence(&self) -> Option<CandidateAbsence> {
        match self {
            Self::Present { .. } => None,
            Self::Absent { reason, .. } => Some(*reason),
        }
    }
}

/// Binder-owned candidate production outcome for one exact expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpressionCandidateSet {
    /// The expression does not perform semantic candidate selection.
    NotApplicable(BoundExpressionId),
    /// The expression is an ordinary callable application.
    Callable(CallableCandidateTemplates),
    /// The expression selects a category not covered by the current candidate producer.
    Unsupported {
        /// The expression that requested candidates.
        expression: BoundExpressionId,
        /// The semantic selection category requiring another candidate provider.
        kind: SelectionKind,
    },
}

impl ExpressionCandidateSet {
    /// Returns the expression that requested candidate production.
    pub const fn expression(&self) -> BoundExpressionId {
        match self {
            Self::NotApplicable(expression) | Self::Unsupported { expression, .. } => *expression,
            Self::Callable(candidates) => candidates.expression(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CallableCandidateTemplate, CallableCandidateTemplateState, CallableCandidateTemplates,
        CallableValueCandidateTemplate, CandidateAbsence, ExpressionCandidateSet,
    };
    use crate::test_support::{callable_key, error_type, push_expression};
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundExpressionId, BoundNodeOrigin,
        BoundReferenceTarget, BoundTreeBuilder, BoundUnitId, DeclaredValueTypeTerm, SelectionKind,
    };
    use bray_symbols::{AnySymbolId, FunctionSymbolId, SymbolId, TypeExpressionTemplate};

    #[test]
    fn candidate_template_outcomes_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CallableCandidateTemplate>();
        assert_send_sync::<CallableCandidateTemplates>();
        assert_send_sync::<ExpressionCandidateSet>();
    }

    #[test]
    fn callable_candidate_sets_are_nonempty_and_deterministic_when_present() {
        let expression = expression_id();
        let first = value_candidate(2);
        let second = value_candidate(1);

        let Some(candidates) =
            CallableCandidateTemplates::present(expression, [first.clone(), second.clone(), first])
        else {
            panic!("nonempty candidates must produce a present set");
        };

        assert_eq!(candidates.expression(), expression);
        assert_eq!(
            candidates.candidates(),
            Some([second, value_candidate(2)].as_slice())
        );
        assert_eq!(candidates.absence(), None);
    }

    #[test]
    fn empty_and_unsupported_results_are_explicit() {
        let expression = expression_id();

        assert!(CallableCandidateTemplates::present(expression, []).is_none());

        let absent =
            CallableCandidateTemplates::absent(expression, CandidateAbsence::MissingValueType);

        assert_eq!(absent.candidates(), None);
        assert_eq!(absent.absence(), Some(CandidateAbsence::MissingValueType));

        let unsupported = ExpressionCandidateSet::Unsupported {
            expression,
            kind: SelectionKind::Operator,
        };

        assert_eq!(unsupported.expression(), expression);
    }

    fn value_candidate(symbol: u32) -> CallableCandidateTemplate {
        let target = BoundReferenceTarget::Surface(AnySymbolId::from(
            FunctionSymbolId::from_symbol_id(SymbolId::new(symbol)),
        ));

        CallableCandidateTemplate::Value(CallableValueCandidateTemplate::new(
            DeclaredValueTypeTerm::Value(target),
            TypeExpressionTemplate::Resolved(error_type()),
            CallableCandidateTemplateState::Visible,
        ))
    }

    fn expression_id() -> BoundExpressionId {
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(3));

        push_expression(
            &mut tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
        )
    }
}
