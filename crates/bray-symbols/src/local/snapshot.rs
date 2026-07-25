use bray_declarations::SyntaxAnchor;

use super::{
    AnonymousCallableParameterSymbol, AnonymousCallableParameterSymbolId, AnonymousCallableSymbol,
    AnonymousCallableSymbolId, AnyLocalSymbolId, LocalBindingSymbol, LocalBindingSymbolId,
    LocalConstantSymbol, LocalConstantSymbolId, LocalScope, LocalScopeId, LocalSymbolRegionId,
    LocalSymbolRegionKey, PostconditionResultSymbol, PostconditionResultSymbolId,
};

/// An immutable symbol and lexical-scope snapshot for one checked semantic region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSymbolSnapshot {
    pub(super) region: LocalSymbolRegionId,
    pub(super) key: LocalSymbolRegionKey,
    pub(super) scopes: Box<[LocalScope]>,
    pub(super) bindings: Box<[LocalBindingSymbol]>,
    pub(super) constants: Box<[LocalConstantSymbol]>,
    pub(super) anonymous_callables: Box<[AnonymousCallableSymbol]>,
    pub(super) anonymous_parameters: Box<[AnonymousCallableParameterSymbol]>,
    pub(super) postcondition_results: Box<[PostconditionResultSymbol]>,
}

impl LocalSymbolSnapshot {
    /// Returns the local semantic region that owns this snapshot.
    pub const fn region(&self) -> LocalSymbolRegionId {
        self.region
    }

    /// Returns this region's deterministic key.
    pub const fn key(&self) -> &LocalSymbolRegionKey {
        &self.key
    }

    /// Returns lexical scopes in canonical order.
    pub fn scopes(&self) -> &[LocalScope] {
        &self.scopes
    }

    /// Returns a lexical scope when the ID belongs to this snapshot.
    pub fn scope(&self, id: LocalScopeId) -> Option<&LocalScope> {
        self.get(id.region(), id.to_index(), &self.scopes)
    }

    /// Returns local bindings in deterministic category order.
    pub fn bindings(&self) -> &[LocalBindingSymbol] {
        &self.bindings
    }

    /// Returns a local binding when the ID belongs to this snapshot.
    pub fn binding(&self, id: LocalBindingSymbolId) -> Option<&LocalBindingSymbol> {
        self.get(id.region(), id.to_index(), &self.bindings)
    }

    /// Returns local constants in deterministic category order.
    pub fn constants(&self) -> &[LocalConstantSymbol] {
        &self.constants
    }

    /// Returns a local constant when the ID belongs to this snapshot.
    pub fn constant(&self, id: LocalConstantSymbolId) -> Option<&LocalConstantSymbol> {
        self.get(id.region(), id.to_index(), &self.constants)
    }

    /// Returns anonymous callables in deterministic category order.
    pub fn anonymous_callables(&self) -> &[AnonymousCallableSymbol] {
        &self.anonymous_callables
    }

    /// Returns an anonymous callable when the ID belongs to this snapshot.
    pub fn anonymous_callable(
        &self,
        id: AnonymousCallableSymbolId,
    ) -> Option<&AnonymousCallableSymbol> {
        self.get(id.region(), id.to_index(), &self.anonymous_callables)
    }

    /// Returns anonymous callable parameters in deterministic category order.
    pub fn anonymous_parameters(&self) -> &[AnonymousCallableParameterSymbol] {
        &self.anonymous_parameters
    }

    /// Returns an anonymous parameter when the ID belongs to this snapshot.
    pub fn anonymous_parameter(
        &self,
        id: AnonymousCallableParameterSymbolId,
    ) -> Option<&AnonymousCallableParameterSymbol> {
        self.get(id.region(), id.to_index(), &self.anonymous_parameters)
    }

    /// Returns contextual postcondition results in deterministic category order.
    pub fn postcondition_results(&self) -> &[PostconditionResultSymbol] {
        &self.postcondition_results
    }

    /// Returns a postcondition result when the ID belongs to this snapshot.
    pub fn postcondition_result(
        &self,
        id: PostconditionResultSymbolId,
    ) -> Option<&PostconditionResultSymbol> {
        self.get(id.region(), id.to_index(), &self.postcondition_results)
    }

    /// Returns the source construct that introduced a local symbol.
    pub fn syntax_anchor(&self, id: AnyLocalSymbolId) -> Option<SyntaxAnchor> {
        match id {
            AnyLocalSymbolId::Binding(id) => self.binding(id)?.key().anchors().first().copied(),
            AnyLocalSymbolId::Constant(id) => self.constant(id)?.key().anchors().first().copied(),
            AnyLocalSymbolId::AnonymousCallable(id) => self
                .anonymous_callable(id)?
                .key()
                .anchors()
                .first()
                .copied(),
            AnyLocalSymbolId::AnonymousCallableParameter(id) => self
                .anonymous_parameter(id)?
                .key()
                .anchors()
                .first()
                .copied(),
            AnyLocalSymbolId::PostconditionResult(id) => {
                Some(self.postcondition_result(id)?.syntax_anchor())
            }
        }
    }

    /// Returns the local symbol introduced by an exact source construct.
    pub fn symbol_for_syntax(&self, syntax: SyntaxAnchor) -> Option<AnyLocalSymbolId> {
        let mut symbols = self
            .bindings
            .iter()
            .filter(|symbol| symbol.key().anchors().contains(&syntax))
            .map(|symbol| AnyLocalSymbolId::from(symbol.id()))
            .chain(
                self.constants
                    .iter()
                    .filter(|symbol| symbol.key().anchors().contains(&syntax))
                    .map(|symbol| AnyLocalSymbolId::from(symbol.id())),
            )
            .chain(
                self.anonymous_callables
                    .iter()
                    .filter(|symbol| symbol.key().anchors().contains(&syntax))
                    .map(|symbol| AnyLocalSymbolId::from(symbol.id())),
            )
            .chain(
                self.anonymous_parameters
                    .iter()
                    .filter(|symbol| symbol.key().anchors().contains(&syntax))
                    .map(|symbol| AnyLocalSymbolId::from(symbol.id())),
            )
            .chain(
                self.postcondition_results
                    .iter()
                    .filter(|symbol| symbol.syntax_anchor() == syntax)
                    .map(|symbol| AnyLocalSymbolId::from(symbol.id())),
            );

        let symbol = symbols.next()?;

        symbols.next().is_none().then_some(symbol)
    }

    fn get<'a, T>(
        &self,
        region: LocalSymbolRegionId,
        index: Option<usize>,
        records: &'a [T],
    ) -> Option<&'a T> {
        if region != self.region {
            return None;
        }

        index.and_then(|index| records.get(index))
    }
}
