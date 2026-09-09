use std::sync::Arc;

use bray_base::shared_slice;
pub use bray_bound_tree::CallableParameterDefaultTemplate;
use bray_bound_tree::{
    BoundExpressionId, CallableDeclarationTemplate, DeclaredValueTypeTerm, SelectionKind,
};
use bray_symbols::{
    CallableContractTemplate, CallableDefinitionId, CallableSignatureTemplate,
    GenericArgumentTemplate, GenericDeclarationTemplate, PredicateDefinitionSymbolId,
    PredicateSignatureTemplate, SymbolKey,
};

/// Why an exact semantic candidate request has no candidate surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CandidateAbsence {
    /// Name binding did not produce a usable callee identity.
    UnresolvedReference,
    /// Every declaration reached through an overload arm was unusable.
    EmptyOverload,
    /// Required declaration semantics are not available from the symbol's origin.
    UnavailableDeclarationSemantics,
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

/// One declared callable considered before substitution and applicability checking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableDeclarationCandidateTemplate {
    declaration: CallableDeclarationTemplate,
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
            declaration: CallableDeclarationTemplate::new(
                key,
                definition,
                signature,
                contract,
                generic,
                generic_arguments,
            ),
            state,
        }
    }

    /// Creates a candidate from a declaration template and participation state.
    pub fn from_declaration(
        declaration: CallableDeclarationTemplate,
        state: CallableCandidateTemplateState,
    ) -> Self {
        Self { declaration, state }
    }

    /// Supplies declaration-owned parameter defaults in parameter order.
    pub fn with_defaults(
        mut self,
        defaults: impl IntoIterator<Item = CallableParameterDefaultTemplate>,
    ) -> Self {
        self.declaration = self.declaration.with_defaults(defaults);

        self
    }

    /// Supplies the selected subject for declaration-local `Self` references.
    pub fn with_contextual_self(
        mut self,
        context: bray_symbols::SelfTypeContext,
        replacement: bray_symbols::TypeExpressionTemplate,
    ) -> Self {
        self.declaration = self.declaration.with_contextual_self(context, replacement);

        self
    }

    /// Returns the candidate's stable semantic key.
    pub fn key(&self) -> &SymbolKey {
        self.declaration.key()
    }

    /// Returns the exact callable declaration.
    pub fn definition(&self) -> CallableDefinitionId {
        self.declaration.definition()
    }

    /// Returns the complete unevaluated callable signature.
    pub fn signature(&self) -> &CallableSignatureTemplate {
        self.declaration.signature()
    }

    /// Returns the selected subject replacing declaration-local `Self` during instantiation.
    pub(crate) fn contextual_self(
        &self,
    ) -> Option<&(
        bray_symbols::SelfTypeContext,
        bray_symbols::TypeExpressionTemplate,
    )> {
        self.declaration.contextual_self()
    }

    /// Returns source or imported contract clauses retained for semantic checking.
    pub fn contract(&self) -> &CallableContractTemplate {
        self.declaration.contract()
    }

    /// Returns generic parameters and constraint templates.
    pub fn generic(&self) -> &GenericDeclarationTemplate {
        self.declaration.generic()
    }

    /// Returns explicit generic arguments bound for this candidate.
    pub fn generic_arguments(&self) -> &[GenericArgumentTemplate] {
        self.declaration.generic_arguments()
    }

    /// Returns parameter defaults in declaration order.
    pub fn defaults(&self) -> &[CallableParameterDefaultTemplate] {
        self.declaration.defaults()
    }

    /// Returns the participation state known before applicability checking.
    pub fn state(&self) -> CallableCandidateTemplateState {
        self.state
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
    /// A callable declaration with an exact signature.
    Declaration(CallableDeclarationCandidateTemplate),
    /// A compile-time predicate with an exact signature.
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
            CandidateAbsence::UnavailableDeclarationSemantics,
        );

        assert_eq!(absent.candidates(), None);

        assert_eq!(
            absent.absence(),
            Some(CandidateAbsence::UnavailableDeclarationSemantics)
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
