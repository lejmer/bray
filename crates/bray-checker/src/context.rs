use std::sync::Arc;

use bray_base::Cancellation;
use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, BoundSourceAnchor, BoundUnit};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticResult;
use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
use bray_symbols::{
    AnySymbolId, AvailableCompilerKnownSymbols, GenericConstraintObligationKey, ProofOutcome,
    SemanticValueStore, SymbolFactContract, SymbolFactKind, SymbolFactRequest, SymbolFactResult,
    SymbolGraph,
};
use bray_target::TargetProfile;

use crate::{CheckerUnitViewError, SemanticUnitContext};

/// A checker infrastructure failure that is neither a source diagnostic nor cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerInfrastructureError {
    /// The compilation does not contain the source named by a bound anchor.
    MissingSource {
        /// The unavailable source identity.
        source_id: SourceId,
    },
    /// A bound anchor names a different source revision than the compilation.
    SourceVersionMismatch {
        /// The source whose revision did not match.
        source_id: SourceId,
        /// The revision retained by the bound anchor.
        expected: SourceVersion,
        /// The revision available in the compilation.
        actual: SourceVersion,
    },
    /// A bound anchor does not cover a valid UTF-8 range in its source revision.
    InvalidSourceRange {
        /// The invalid source span.
        span: SourceSpan,
    },
    /// A required semantic fact could not be supplied.
    SemanticFactUnavailable {
        /// The exact symbol that owns the fact.
        symbol: AnySymbolId,
        /// The unavailable fact category.
        kind: SymbolFactKind,
    },
    /// Canonical semantic value construction or lookup failed.
    SemanticValueUnavailable,
    /// A required compiler-known representation is unavailable for the selected target.
    CompilerKnownRepresentationUnavailable {
        /// The unavailable representation role.
        role: RepresentationRole,
    },
    /// Type-checking input names an expression outside the requested unit.
    InvalidExpressionTypeInput {
        /// The invalid expression identity.
        expression: BoundExpressionId,
    },
    /// Semantic-selection inputs do not describe the requested bound unit or operation category.
    InvalidSemanticSelectionInput,
    /// Constant-evaluation inputs do not describe the requested bound unit.
    InvalidConstantEvaluationInput,
    /// Pattern-checking inputs disagree for one bound occurrence.
    InvalidPatternCheckInput,
    /// Storage-planning inputs or constructed records violate the requested unit contract.
    InvalidStoragePlan,
    /// Liveness inputs or durable decisions violate the requested unit contract.
    InvalidLivenessFacts,
    /// A committed bound relationship names a node absent from the requested unit.
    InvalidBoundNode {
        /// The missing bound node identity.
        node: AnyBoundNodeId,
    },
    /// One unit contains more expression variables than the checker can identify compactly.
    ExpressionTypeCapacityExceeded,
    /// A checker unit view did not match its canonical bound unit.
    InvalidUnitView(CheckerUnitViewError),
}

/// A failure while requesting a checker dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerFactError {
    /// Cancellation was observed while obtaining the dependency.
    Cancelled,
    /// Compiler infrastructure could not supply the dependency.
    Infrastructure(CheckerInfrastructureError),
}

/// The result of requesting one checker dependency.
pub type CheckerFactResult<T> = Result<T, CheckerFactError>;

/// The exact source span and text covered by one bound source anchor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CheckerSource<'source> {
    span: SourceSpan,
    text: &'source str,
}

impl<'source> CheckerSource<'source> {
    /// Creates a resolved checker source view.
    pub const fn new(span: SourceSpan, text: &'source str) -> Self {
        Self { span, text }
    }

    /// Returns the exact anchored source span.
    pub const fn span(self) -> SourceSpan {
        self.span
    }

    /// Returns the source text covered by the anchor.
    pub const fn text(self) -> &'source str {
        self.text
    }

    /// Returns text for an exact subrange of this resolved source view.
    pub fn text_for_range(self, range: TextRange) -> Option<&'source str> {
        if !self.span.range().contains_range(range) {
            return None;
        }

        let base = self.span.range().start().bytes();

        let relative = TextRange::new(
            TextSize::new(range.start().bytes() - base),
            TextSize::new(range.end().bytes() - base),
        );

        relative.slice_str(self.text)
    }
}

/// Narrow immutable services shared by checker unit views.
pub trait CheckerRequestContext: Sync {
    /// Returns whether semantic context exactly describes the supplied bound unit.
    fn semantic_context_matches(&self, unit: &BoundUnit, context: &SemanticUnitContext) -> bool;

    /// Returns the canonical semantic values used by bound structure and facts.
    fn semantic_values(&self) -> &SemanticValueStore;

    /// Returns the compilation-wide symbol graph.
    fn symbols(&self) -> &SymbolGraph;

    /// Returns compiler-known symbols available for the current target.
    fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols;

    /// Returns the selected language-level target profile.
    fn selected_target(&self) -> &TargetProfile;

    /// Checks one source constant expression embedded in a type template.
    fn checked_constant_expression(
        &self,
        occurrence: bray_symbols::ConstantExpressionOccurrence,
    ) -> CheckerFactResult<DiagnosticResult<bray_symbols::ConstantTermId>>;

    /// Proves the static constraints for one exact generic declaration instance.
    fn generic_constraints(
        &self,
        obligation: GenericConstraintObligationKey,
    ) -> CheckerFactResult<DiagnosticResult<ProofOutcome>>;

    /// Resolves a bound source anchor without exposing its source snapshot.
    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError>;

    /// Returns the cancellation source for the current request.
    fn cancellation(&self) -> &dyn Cancellation;
}

/// Origin-neutral typed access to one family of symbol-owned semantic facts.
pub trait CheckerSemanticFactProvider<C>: CheckerRequestContext
where
    C: SymbolFactContract,
{
    /// Returns the requested immutable semantic fact and its owned diagnostics.
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<C>,
    ) -> CheckerFactResult<Arc<SymbolFactResult<C>>>;
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_symbols::CallableSignatureFact;

    use super::{CheckerRequestContext, CheckerSemanticFactProvider, CheckerSource};

    #[test]
    fn request_context_contracts_are_shareable() {
        fn assert_sync<T: Sync + ?Sized>() {}

        assert_sync::<dyn CheckerRequestContext>();
        assert_sync::<dyn CheckerSemanticFactProvider<CallableSignatureFact>>();
    }

    #[test]
    fn checker_sources_resolve_only_valid_contained_subranges() {
        let source = CheckerSource::new(
            SourceSpan::new(
                SourceId::new(0),
                TextRange::new(TextSize::new(4), TextSize::new(9)),
            ),
            "aébc",
        );

        assert_eq!(
            source.text_for_range(TextRange::new(TextSize::new(5), TextSize::new(8))),
            Some("éb")
        );

        assert_eq!(
            source.text_for_range(TextRange::new(TextSize::new(3), TextSize::new(5))),
            None
        );

        assert_eq!(
            source.text_for_range(TextRange::new(TextSize::new(6), TextSize::new(8))),
            None
        );
    }
}
