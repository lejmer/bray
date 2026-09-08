use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;

use crate::{CallableContractSet, CallableSymbolId, SymbolOrdinal};

use super::DeclarationExpressionTemplate;

/// The declaration clause that owns one predicate expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclarationPredicateClauseKind {
    /// A callable invocation precondition.
    Requires,
    /// A callable successful-completion guarantee.
    Ensures,
    /// An execution-entry condition guarding nested guarantees.
    Guard,
    /// A callable static constraint.
    Static,
}

/// One callable contract expression retained for later checking.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableContractExpressionTemplate {
    ordinal: SymbolOrdinal,
    kind: DeclarationPredicateClauseKind,
    unit_syntax: SyntaxAnchor,
    expression: DeclarationExpressionTemplate,
    guard: Option<SymbolOrdinal>,
}

impl CallableContractExpressionTemplate {
    /// Creates one callable contract expression in declaration order.
    pub const fn new(
        ordinal: SymbolOrdinal,
        kind: DeclarationPredicateClauseKind,
        unit_syntax: SyntaxAnchor,
        expression: DeclarationExpressionTemplate,
    ) -> Self {
        Self {
            ordinal,
            kind,
            unit_syntax,
            expression,
            guard: None,
        }
    }

    /// Associates this expression with its immediate enclosing execution-entry guard.
    pub const fn with_guard(mut self, guard: Option<SymbolOrdinal>) -> Self {
        self.guard = guard;

        self
    }

    /// Returns the enclosing guard's expression ordinal, if any.
    pub const fn guard(self) -> Option<SymbolOrdinal> {
        self.guard
    }

    /// Returns the expression's stable declaration-order position.
    pub const fn ordinal(self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the clause semantics attached to the expression.
    pub const fn kind(self) -> DeclarationPredicateClauseKind {
        self.kind
    }

    /// Returns the contract clause that forms the bound semantic unit.
    pub const fn unit_syntax(self) -> SyntaxAnchor {
        self.unit_syntax
    }

    /// Returns the exact source expression retained for checking.
    pub const fn expression(self) -> DeclarationExpressionTemplate {
        self.expression
    }
}

/// One declared trusted capability path retained for later resolution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclarationCapabilityTemplate {
    ordinal: SymbolOrdinal,
    syntax: SyntaxAnchor,
}

impl DeclarationCapabilityTemplate {
    /// Creates one capability path in declaration order.
    pub const fn new(ordinal: SymbolOrdinal, syntax: SyntaxAnchor) -> Self {
        Self { ordinal, syntax }
    }

    /// Returns the capability's stable declaration-order position.
    pub const fn ordinal(self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the exact capability path syntax.
    pub const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }
}

/// Source-backed callable contract clauses.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceCallableContractTemplate {
    owner: CallableSymbolId,
    expressions: Arc<[CallableContractExpressionTemplate]>,
    capabilities: Arc<[DeclarationCapabilityTemplate]>,
    execution_guarantees: Arc<[ExecutionGuaranteeTemplate]>,
}

impl SourceCallableContractTemplate {
    /// Creates a callable contract template in declaration order.
    pub fn new(
        owner: CallableSymbolId,
        expressions: impl IntoIterator<Item = CallableContractExpressionTemplate>,
        capabilities: impl IntoIterator<Item = DeclarationCapabilityTemplate>,
        execution_guarantees: impl IntoIterator<Item = ExecutionGuaranteeTemplate>,
    ) -> Self {
        Self {
            owner,
            expressions: shared_slice(expressions),
            capabilities: shared_slice(capabilities),
            execution_guarantees: shared_slice(execution_guarantees),
        }
    }

    /// Returns the callable that owns this contract.
    pub const fn owner(&self) -> CallableSymbolId {
        self.owner
    }

    /// Returns predicate expressions in declaration order.
    pub fn expressions(&self) -> &[CallableContractExpressionTemplate] {
        &self.expressions
    }

    /// Returns declared trusted capability paths in declaration order.
    pub fn capabilities(&self) -> &[DeclarationCapabilityTemplate] {
        &self.capabilities
    }

    /// Returns declared execution properties awaiting semantic verification.
    pub fn execution_guarantees(&self) -> &[ExecutionGuaranteeTemplate] {
        &self.execution_guarantees
    }
}

/// A source callable contract template or an already-resolved imported contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableContractTemplate {
    /// Source-backed clauses retained for later checking.
    Source(SourceCallableContractTemplate),
    /// A source-independent imported contract.
    Resolved(CallableContractSet),
}

impl CallableContractTemplate {
    /// Creates a source-backed callable contract template.
    pub fn source(
        owner: CallableSymbolId,
        expressions: impl IntoIterator<Item = CallableContractExpressionTemplate>,
        capabilities: impl IntoIterator<Item = DeclarationCapabilityTemplate>,
        execution_guarantees: impl IntoIterator<Item = ExecutionGuaranteeTemplate>,
    ) -> Self {
        Self::Source(SourceCallableContractTemplate::new(
            owner,
            expressions,
            capabilities,
            execution_guarantees,
        ))
    }

    /// Returns source-backed clauses when this declaration has source syntax.
    pub const fn source_template(&self) -> Option<&SourceCallableContractTemplate> {
        match self {
            Self::Source(template) => Some(template),
            Self::Resolved(_) => None,
        }
    }

    /// Returns the resolved contract when supplied by a compiled interface.
    pub const fn resolved(&self) -> Option<&CallableContractSet> {
        match self {
            Self::Source(_) => None,
            Self::Resolved(contract) => Some(contract),
        }
    }
}

/// One execution-property declaration and its optional enclosing guard.
///
/// This source template is a proof obligation, never evidence that execution has the property.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionGuaranteeTemplate {
    syntax: SyntaxAnchor,
    guard: Option<SymbolOrdinal>,
}

impl ExecutionGuaranteeTemplate {
    /// Retains one property identifier and its execution-entry domain.
    pub const fn new(syntax: SyntaxAnchor, guard: Option<SymbolOrdinal>) -> Self {
        Self { syntax, guard }
    }

    /// Returns the property identifier for checking and source diagnostics.
    pub const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }

    /// Returns the immediate guard's expression ordinal, or the unconditional domain.
    pub const fn guard(self) -> Option<SymbolOrdinal> {
        self.guard
    }
}
