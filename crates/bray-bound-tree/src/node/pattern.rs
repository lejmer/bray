use std::sync::Arc;

use bray_base::shared_slice;
use bray_source::TextRange;
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, LocalBindingSymbolId, SymbolName, TypeId};

use crate::{BoundLiteralKind, BoundNodeOrigin, BoundPatternId};

/// The source-semantic operation performed by a bound pattern.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundPatternMode {
    /// Introduce local bindings that become visible after the pattern succeeds.
    Declaration,
    /// Assign through names and projections already visible at the pattern site.
    Assignment,
    /// Observe a match subject and introduce arm-local bindings on success.
    MatchObserve,
    /// Consume a match subject and introduce arm-local bindings on success.
    MatchConsume,
}

/// The source-shaped semantic form retained for a bound pattern.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundPatternKind {
    /// A name-binding pattern.
    Binding,
    /// A discard pattern.
    Discard,
    /// A literal-value pattern.
    Literal,
    /// A nullable-absent pattern.
    NullableAbsent,
    /// A nullable-present pattern.
    NullablePresent,
    /// An owning-box pattern.
    Box,
    /// A path-selected pattern.
    Path,
    /// A leading-dot or path-selected variant pattern.
    Variant,
    /// A product destructuring pattern.
    Product,
    /// A tuple destructuring pattern.
    Tuple,
    /// An array destructuring pattern.
    Array,
    /// A grouped pattern.
    Grouped,
    /// A case-pattern alternative set.
    Alternative,
    /// A remaining-elements or remaining-fields pattern.
    Remaining,
    /// A pattern preserved after semantic recovery.
    Error,
}

/// An existing semantic entity selected by a non-binding pattern.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundPatternTarget {
    /// A unit-local assignment destination.
    Local(AnyLocalSymbolId),
    /// A declaration selected by a named or assignment pattern.
    Surface(AnySymbolId),
}

/// A literal retained by a source pattern.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundPatternLiteral {
    kind: BoundLiteralKind,
    range: TextRange,
}

impl BoundPatternLiteral {
    /// Creates one source-correlated pattern literal.
    pub const fn new(kind: BoundLiteralKind, range: TextRange) -> Self {
        Self { kind, range }
    }

    /// Returns the literal category.
    pub const fn kind(self) -> BoundLiteralKind {
        self.kind
    }

    /// Returns the exact literal-token range.
    pub const fn range(self) -> TextRange {
        self.range
    }
}

/// One product, tuple, array, or variant payload entry in source order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPatternEntry {
    name: Option<SymbolName>,
    kind: BoundPatternEntryKind,
}

/// The exact semantic content of one structured pattern entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundPatternEntryKind {
    /// A nested source pattern.
    Pattern(BoundPatternId),
    /// A shorthand binding whose name also selects the field.
    Binding(LocalBindingSymbolId),
    /// The `..` marker accounting for remaining fields or elements.
    Remaining,
    /// Recovery prevented the entry from retaining valid semantic content.
    Recovered,
}

impl BoundPatternEntry {
    /// Creates one source-ordered pattern entry.
    pub const fn new(name: Option<SymbolName>, kind: BoundPatternEntryKind) -> Self {
        Self { name, kind }
    }

    /// Returns the optional named-field selector.
    pub const fn name(&self) -> Option<&SymbolName> {
        self.name.as_ref()
    }

    /// Returns the nested pattern when this entry contains one.
    pub const fn pattern(&self) -> Option<BoundPatternId> {
        match self.kind {
            BoundPatternEntryKind::Pattern(pattern) => Some(pattern),
            BoundPatternEntryKind::Binding(_)
            | BoundPatternEntryKind::Remaining
            | BoundPatternEntryKind::Recovered => None,
        }
    }

    /// Returns the shorthand binding introduced by this entry.
    pub const fn binding(&self) -> Option<LocalBindingSymbolId> {
        match self.kind {
            BoundPatternEntryKind::Binding(binding) => Some(binding),
            BoundPatternEntryKind::Pattern(_)
            | BoundPatternEntryKind::Remaining
            | BoundPatternEntryKind::Recovered => None,
        }
    }

    /// Returns whether this is a remaining-elements or remaining-fields entry.
    pub const fn is_remaining(&self) -> bool {
        matches!(self.kind, BoundPatternEntryKind::Remaining)
    }

    /// Returns the exact entry content.
    pub const fn kind(&self) -> BoundPatternEntryKind {
        self.kind
    }
}

/// One checked source pattern and its exact local identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPattern {
    origin: BoundNodeOrigin,
    input_type: TypeId,
    mode: BoundPatternMode,
    kind: BoundPatternKind,
    children: Arc<[BoundPatternId]>,
    entries: Arc<[BoundPatternEntry]>,
    bindings: Arc<[LocalBindingSymbolId]>,
    target: Option<BoundPatternTarget>,
    name: Option<SymbolName>,
    literal: Option<BoundPatternLiteral>,
    is_mutable: bool,
    is_recovered: bool,
}

impl BoundPattern {
    /// Creates one immutable checked or error-aware pattern.
    pub fn new(
        origin: BoundNodeOrigin,
        input_type: TypeId,
        mode: BoundPatternMode,
        kind: BoundPatternKind,
        children: impl IntoIterator<Item = BoundPatternId>,
        bindings: impl IntoIterator<Item = LocalBindingSymbolId>,
    ) -> Self {
        Self {
            origin,
            input_type,
            mode,
            kind,
            children: shared_slice(children),
            entries: Arc::from([]),
            bindings: shared_slice(bindings),
            target: None,
            name: None,
            literal: None,
            is_mutable: false,
            is_recovered: false,
        }
    }

    /// Returns this pattern with source-ordered structured entries recorded.
    pub fn with_entries(mut self, entries: impl IntoIterator<Item = BoundPatternEntry>) -> Self {
        self.entries = shared_slice(entries);

        self
    }

    /// Returns this pattern with its binding mutability recorded.
    pub const fn with_mutability(mut self, is_mutable: bool) -> Self {
        self.is_mutable = is_mutable;
        self
    }

    /// Returns this pattern with syntax or semantic recovery recorded.
    pub const fn with_recovery(mut self, is_recovered: bool) -> Self {
        self.is_recovered = is_recovered;
        self
    }

    /// Returns this pattern with its resolved non-binding target recorded.
    pub const fn with_target(mut self, target: Option<BoundPatternTarget>) -> Self {
        self.target = target;
        self
    }

    /// Returns this pattern with its source name recorded.
    pub fn with_name(mut self, name: Option<SymbolName>) -> Self {
        self.name = name;

        self
    }

    /// Returns this pattern with its source literal recorded.
    pub const fn with_literal(mut self, literal: Option<BoundPatternLiteral>) -> Self {
        self.literal = literal;

        self
    }

    /// Returns the source or synthesized origin of this pattern.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the checked or recovery input type of this pattern.
    pub const fn input_type(&self) -> TypeId {
        self.input_type
    }

    /// Returns the operation performed by this pattern.
    pub const fn mode(&self) -> BoundPatternMode {
        self.mode
    }

    /// Returns this pattern's exact source-shaped form.
    pub const fn kind(&self) -> BoundPatternKind {
        self.kind
    }

    /// Returns direct nested patterns in deterministic source order.
    pub fn children(&self) -> &[BoundPatternId] {
        &self.children
    }

    /// Returns structured entries in deterministic source order.
    pub fn entries(&self) -> &[BoundPatternEntry] {
        &self.entries
    }

    /// Returns local bindings introduced directly by this pattern.
    pub fn bindings(&self) -> &[LocalBindingSymbolId] {
        &self.bindings
    }

    /// Returns the existing local or declaration selected by this pattern.
    pub const fn target(&self) -> Option<BoundPatternTarget> {
        self.target
    }

    /// Returns the source name retained by this pattern.
    pub const fn name(&self) -> Option<&SymbolName> {
        self.name.as_ref()
    }

    /// Returns the source literal retained by this pattern.
    pub const fn literal(&self) -> Option<BoundPatternLiteral> {
        self.literal
    }

    /// Returns whether this binding pattern requested mutable storage access.
    pub const fn is_mutable(&self) -> bool {
        self.is_mutable
    }

    /// Returns whether syntax or semantic recovery contributed to this pattern.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{error_type, source_anchor};
    use crate::{
        BoundNodeOrigin, BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode,
        BoundUnitId,
    };

    #[test]
    fn patterns_retain_mode_shape_children_and_local_identities() {
        let source = source_anchor();
        let input_type = error_type();
        let unit = BoundUnitId::new(3);
        let child = BoundPatternId::from_slot(unit, 0);

        let pattern = BoundPattern::new(
            BoundNodeOrigin::source(source),
            input_type,
            BoundPatternMode::Declaration,
            BoundPatternKind::Tuple,
            [child],
            [],
        )
        .with_mutability(true);

        assert_eq!(pattern.origin().source_anchor(), source);
        assert_eq!(pattern.input_type(), input_type);
        assert_eq!(pattern.mode(), BoundPatternMode::Declaration);
        assert_eq!(pattern.kind(), BoundPatternKind::Tuple);
        assert_eq!(pattern.children(), &[child]);
        assert!(pattern.bindings().is_empty());
        assert!(pattern.is_mutable());
        assert!(!pattern.is_recovered());
    }
}
