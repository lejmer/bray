use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{BoundExpressionId, DeclaredValueTypeTerm, SelectionKind};
use bray_symbols::{
    CallableContractTemplate, CallableDefinitionId, CallableParameterDefaultProviderSymbolId,
    CallableParameterSymbolId, CallableSignatureTemplate, GenericArgumentTemplate,
    GenericDeclarationTemplate, PredicateDefinitionSymbolId, PredicateSignatureTemplate, SymbolKey,
    UnevaluatedDefaultTemplate,
};

/// Why an exact semantic candidate request has no candidate surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CandidateAbsence {
    /// Name binding did not produce a usable callee identity.
    UnresolvedReference,
    /// Every declaration reached through an overload arm was unusable.
    EmptyOverload,
    /// Required declaration facts are not available from the symbol's origin.
    UnavailableDeclarationFacts,
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
    provider: Option<CallableParameterDefaultProviderSymbolId>,
}

impl CallableParameterDefaultTemplate {
    /// Creates one declaration parameter default entry.
    pub const fn new(
        parameter: CallableParameterSymbolId,
        value: UnevaluatedDefaultTemplate,
        provider: Option<CallableParameterDefaultProviderSymbolId>,
    ) -> Self {
        Self {
            parameter,
            value,
            provider,
        }
    }

    /// Returns the declaration parameter.
    pub const fn parameter(self) -> CallableParameterSymbolId {
        self.parameter
    }

    /// Returns the unevaluated default expression surface.
    pub const fn value(self) -> UnevaluatedDefaultTemplate {
        self.value
    }

    /// Returns the declaration-owned provider for a present runtime default.
    pub const fn provider(self) -> Option<CallableParameterDefaultProviderSymbolId> {
        self.provider
    }
}

/// One declared callable considered before substitution and applicability checking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableDeclarationCandidateTemplate {
    data: Arc<CallableDeclarationCandidateTemplateData>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CallableDeclarationCandidateTemplateData {
    key: SymbolKey,
    definition: CallableDefinitionId,
    signature: CallableSignatureTemplate,
    contract: CallableContractTemplate,
    generic: GenericDeclarationTemplate,
    generic_arguments: Arc<[GenericArgumentTemplate]>,
    defaults: Arc<[CallableParameterDefaultTemplate]>,
    state: CallableCandidateTemplateState,
}

impl CallableDeclarationCandidateTemplate {
    /// Creates a declared callable candidate with complete unevaluated signature inputs.
    pub fn new(
        key: SymbolKey,
        definition: CallableDefinitionId,
        signature: CallableSignatureTemplate,
        contract: CallableContractTemplate,
        generic: GenericDeclarationTemplate,
        generic_arguments: impl IntoIterator<Item = GenericArgumentTemplate>,
        state: CallableCandidateTemplateState,
    ) -> Self {
        Self {
            data: Arc::new(CallableDeclarationCandidateTemplateData {
                key,
                definition,
                signature,
                contract,
                generic,
                generic_arguments: shared_slice(generic_arguments),
                defaults: Arc::new([]),
                state,
            }),
        }
    }

    /// Supplies declaration-owned parameter defaults in parameter order.
    pub fn with_defaults(
        mut self,
        defaults: impl IntoIterator<Item = CallableParameterDefaultTemplate>,
    ) -> Self {
        Arc::make_mut(&mut self.data).defaults = shared_slice(defaults);

        self
    }

    /// Returns the candidate's stable semantic key.
    pub fn key(&self) -> &SymbolKey {
        &self.data.key
    }

    /// Returns the exact callable declaration.
    pub fn definition(&self) -> CallableDefinitionId {
        self.data.definition
    }

    /// Returns the complete unevaluated callable signature.
    pub fn signature(&self) -> &CallableSignatureTemplate {
        &self.data.signature
    }

    /// Returns source or imported contract clauses retained for semantic checking.
    pub fn contract(&self) -> &CallableContractTemplate {
        &self.data.contract
    }

    /// Returns generic parameters and constraint templates.
    pub fn generic(&self) -> &GenericDeclarationTemplate {
        &self.data.generic
    }

    /// Returns explicit generic arguments bound for this candidate.
    pub fn generic_arguments(&self) -> &[GenericArgumentTemplate] {
        &self.data.generic_arguments
    }

    /// Returns parameter defaults in declaration order.
    pub fn defaults(&self) -> &[CallableParameterDefaultTemplate] {
        &self.data.defaults
    }

    /// Returns the participation state known before applicability checking.
    pub fn state(&self) -> CallableCandidateTemplateState {
        self.data.state
    }
}

/// One predicate considered before substitution and applicability checking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PredicateCandidateTemplate {
    data: Arc<PredicateCandidateTemplateData>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PredicateCandidateTemplateData {
    key: SymbolKey,
    definition: PredicateDefinitionSymbolId,
    signature: PredicateSignatureTemplate,
    generic: GenericDeclarationTemplate,
    generic_arguments: Arc<[GenericArgumentTemplate]>,
    state: CallableCandidateTemplateState,
}

impl PredicateCandidateTemplate {
    /// Creates a predicate candidate with its complete unevaluated signature inputs.
    pub fn new(
        key: SymbolKey,
        definition: PredicateDefinitionSymbolId,
        signature: PredicateSignatureTemplate,
        generic: GenericDeclarationTemplate,
        generic_arguments: impl IntoIterator<Item = GenericArgumentTemplate>,
        state: CallableCandidateTemplateState,
    ) -> Self {
        Self {
            data: Arc::new(PredicateCandidateTemplateData {
                key,
                definition,
                signature,
                generic,
                generic_arguments: shared_slice(generic_arguments),
                state,
            }),
        }
    }

    /// Returns the candidate's stable semantic key.
    pub fn key(&self) -> &SymbolKey {
        &self.data.key
    }

    /// Returns the exact predicate declaration.
    pub fn definition(&self) -> PredicateDefinitionSymbolId {
        self.data.definition
    }

    /// Returns the complete unevaluated predicate signature.
    pub fn signature(&self) -> &PredicateSignatureTemplate {
        &self.data.signature
    }

    /// Returns generic parameters and constraint templates.
    pub fn generic(&self) -> &GenericDeclarationTemplate {
        &self.data.generic
    }

    /// Returns explicit generic arguments bound for this candidate.
    pub fn generic_arguments(&self) -> &[GenericArgumentTemplate] {
        &self.data.generic_arguments
    }

    /// Returns the participation state known before applicability checking.
    pub fn state(&self) -> CallableCandidateTemplateState {
        self.data.state
    }
}

/// One value retained for callable classification by the type-selection fixed point.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableValueCandidateTemplate {
    value: DeclaredValueTypeTerm,
    state: CallableCandidateTemplateState,
}

impl CallableValueCandidateTemplate {
    /// Creates a callable value candidate from one exact semantic value term.
    pub const fn new(value: DeclaredValueTypeTerm, state: CallableCandidateTemplateState) -> Self {
        Self { value, state }
    }

    /// Returns the exact referenced value.
    pub const fn value(&self) -> DeclaredValueTypeTerm {
        self.value
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
    /// A compile-time predicate with exact signature facts.
    Predicate(PredicateCandidateTemplate),
    /// A value retained for callable classification by the type-selection fixed point.
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

/// Binder-associated source inputs for one non-call semantic selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationCandidateSource {
    expression: BoundExpressionId,
    kind: SelectionKind,
    operands: Arc<[BoundExpressionId]>,
}

impl OperationCandidateSource {
    /// Creates one operation source with operands in semantic evaluation order.
    pub fn new(
        expression: BoundExpressionId,
        kind: SelectionKind,
        operands: impl IntoIterator<Item = BoundExpressionId>,
    ) -> Self {
        Self {
            expression,
            kind,
            operands: shared_slice(operands),
        }
    }

    /// Returns the expression that owns semantic selection.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the exact semantic selection category.
    pub const fn kind(&self) -> SelectionKind {
        self.kind
    }

    /// Returns operands in semantic evaluation order.
    pub fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }
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
    /// The expression requests a non-call semantic operation.
    Operation(OperationCandidateSource),
}

impl ExpressionCandidateSet {
    /// Returns the expression that requested candidate production.
    pub const fn expression(&self) -> BoundExpressionId {
        match self {
            Self::NotApplicable(expression) => *expression,
            Self::Callable(candidates) => candidates.expression(),
            Self::Operation(source) => source.expression(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CallableCandidateTemplate, CallableCandidateTemplateState, CallableCandidateTemplates,
        CallableValueCandidateTemplate, CandidateAbsence, ExpressionCandidateSet,
        OperationCandidateSource,
    };
    use crate::test_support::{callable_key, error_type, push_expression};
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundExpressionId, BoundNodeOrigin,
        BoundReferenceTarget, BoundTreeBuilder, BoundUnitId, DeclaredValueTypeTerm, SelectionKind,
    };
    use bray_symbols::{AnySymbolId, FunctionSymbolId, SymbolId};

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
    fn empty_and_operation_results_are_explicit() {
        let expression = expression_id();

        assert!(CallableCandidateTemplates::present(expression, []).is_none());

        let absent = CallableCandidateTemplates::absent(
            expression,
            CandidateAbsence::UnavailableDeclarationFacts,
        );

        assert_eq!(absent.candidates(), None);

        assert_eq!(
            absent.absence(),
            Some(CandidateAbsence::UnavailableDeclarationFacts)
        );

        let operation = ExpressionCandidateSet::Operation(OperationCandidateSource::new(
            expression,
            SelectionKind::Operator,
            [expression],
        ));

        assert_eq!(operation.expression(), expression);
    }

    fn value_candidate(symbol: u32) -> CallableCandidateTemplate {
        let target = BoundReferenceTarget::Surface(AnySymbolId::from(
            FunctionSymbolId::from_symbol_id(SymbolId::new(symbol)),
        ));

        CallableCandidateTemplate::Value(CallableValueCandidateTemplate::new(
            DeclaredValueTypeTerm::Value(target),
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
