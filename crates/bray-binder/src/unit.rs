use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundSourceAnchor, BoundTree, BoundTreeBuilder, BoundUnitId, BoundUnitKey, BoundUnitKeyData,
    BoundUnitView,
};
use bray_declarations::SyntaxAnchor;
use bray_source::{SourceId, SourceVersion, TextSize};
use bray_symbols::{
    AnonymousCallableParameterSymbolId, AnonymousCallableSymbolId, AnyLocalSymbolId, AnySymbolId,
    LocalBindingSymbolId, LocalConstantSymbolId, LocalScopeBoundary, LocalScopeId,
    LocalSymbolBuildError, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshot, LocalSymbolSnapshotBuilder, PostconditionResultSymbolId, SymbolFactKind,
    SymbolKey, SymbolKind, SymbolName, SymbolOrdinal,
};

/// A structural failure while integrating bound-tree and local-region construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundUnitConstructionError {
    /// The local-symbol contract rejected a scope or symbol relationship.
    LocalSymbol(LocalSymbolBuildError),
    /// A named local identity was activated more than once.
    LocalAlreadyActivated(AnyLocalSymbolId),
    /// An anonymous callable boundary belongs to another enclosing semantic unit.
    AnonymousCallableBoundaryMismatch,
    /// An anonymous callable anchor belongs to a different source.
    AnonymousCallableSourceMismatch {
        /// The source containing the enclosing semantic unit.
        expected: SourceId,
        /// The source supplied for the anonymous callable.
        actual: SourceId,
    },
    /// An anonymous callable source belongs to a different logical source revision.
    AnonymousCallableSourceVersionMismatch {
        /// The source revision of the enclosing semantic unit.
        expected: SourceVersion,
        /// The source revision supplied for the anonymous callable.
        actual: SourceVersion,
    },
}

impl From<LocalSymbolBuildError> for BoundUnitConstructionError {
    fn from(error: LocalSymbolBuildError) -> Self {
        Self::LocalSymbol(error)
    }
}

/// The exact local identity and nested-unit boundary for one anonymous callable.
///
/// The fields are private so an anonymous parameter can only be attached through the builder
/// that created this validated callable/scope pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnonymousCallableBoundary {
    callable: AnonymousCallableSymbolId,
    scope: LocalScopeId,
    unit: BoundUnitKey,
}

impl AnonymousCallableBoundary {
    /// Returns the anonymous callable's local symbol identity.
    pub const fn callable(&self) -> AnonymousCallableSymbolId {
        self.callable
    }

    /// Returns the callable lookup boundary containing its parameters and body names.
    pub const fn scope(&self) -> LocalScopeId {
        self.scope
    }

    /// Returns the independently checked nested semantic-unit key.
    pub const fn unit(&self) -> &BoundUnitKey {
        &self.unit
    }
}

/// Frozen binder-owned construction state for one semantic unit.
#[derive(Debug, Eq, PartialEq)]
pub struct BoundUnitConstructionResult {
    key: BoundUnitKey,
    tree: BoundTree,
    local_symbols: LocalSymbolSnapshot,
    root_scope: LocalScopeId,
}

impl BoundUnitConstructionResult {
    /// Returns the semantic unit's deterministic construction key.
    pub const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    /// Returns the frozen source-shaped bound tree.
    pub const fn tree(&self) -> &BoundTree {
        &self.tree
    }

    /// Returns the frozen local symbol and lexical-scope snapshot.
    pub const fn local_symbols(&self) -> &LocalSymbolSnapshot {
        &self.local_symbols
    }

    /// Returns the root lexical scope of the local semantic region.
    pub const fn root_scope(&self) -> LocalScopeId {
        self.root_scope
    }

    /// Returns a checked read-only view over the frozen bound tree.
    pub fn view(&self) -> BoundUnitView<'_> {
        self.tree.view(&self.key)
    }
}

/// Binder-owned construction of bound nodes, local symbols, and lexical scopes for one unit.
///
/// Local identities can be allocated before semantic binding. Named locals enter ordinary
/// lookup only when [`activate_local`](Self::activate_local) is called at their language-defined
/// visibility point.
#[derive(Debug)]
pub struct BoundUnitLocalBuilder {
    key: BoundUnitKey,
    tree: BoundTreeBuilder,
    local_symbols: LocalSymbolSnapshotBuilder,
    root_scope: LocalScopeId,
    activated_locals: BTreeSet<AnyLocalSymbolId>,
}

impl BoundUnitLocalBuilder {
    /// Begins construction and creates the region's root lexical scope.
    pub fn new(
        unit: BoundUnitId,
        key: BoundUnitKey,
        region: LocalSymbolRegionId,
        visibility_start: TextSize,
    ) -> Result<Self, BoundUnitConstructionError> {
        let region_key = local_region_key(&key);
        let root_syntax = key.source().syntax();
        let mut local_symbols = LocalSymbolSnapshotBuilder::new(region, region_key);

        let root_scope = local_symbols.push_scope(
            None,
            LocalScopeBoundary::Root,
            root_syntax,
            visibility_start,
        )?;

        Ok(Self {
            key,
            tree: BoundTreeBuilder::new(unit),
            local_symbols,
            root_scope,
            activated_locals: BTreeSet::new(),
        })
    }

    /// Returns the semantic unit being constructed.
    pub const fn unit(&self) -> BoundUnitId {
        self.tree.unit()
    }

    /// Returns the semantic unit's deterministic construction key.
    pub const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    /// Returns the local semantic region being constructed.
    pub const fn region(&self) -> LocalSymbolRegionId {
        self.local_symbols.region()
    }

    /// Returns the root lexical scope created with this unit.
    pub const fn root_scope(&self) -> LocalScopeId {
        self.root_scope
    }

    /// Returns mutable access to the unit's bound-node builder.
    pub const fn tree_mut(&mut self) -> &mut BoundTreeBuilder {
        &mut self.tree
    }

    /// Returns a read-only view over bound nodes committed so far.
    pub fn view(&self) -> BoundUnitView<'_> {
        self.tree.view(&self.key)
    }

    /// Adds a lexical scope with its exact source visibility start.
    pub fn push_scope(
        &mut self,
        parent: LocalScopeId,
        boundary: LocalScopeBoundary,
        syntax: SyntaxAnchor,
        visibility_start: TextSize,
    ) -> Result<LocalScopeId, BoundUnitConstructionError> {
        self.local_symbols
            .push_scope(Some(parent), boundary, syntax, visibility_start)
            .map_err(Into::into)
    }

    /// Allocates one logical pattern or local binding without making it visible by name.
    ///
    /// Coherent alternative-pattern occurrences pass all contributing anchors in canonical
    /// source order. Discard and missing-name patterns do not call this method.
    pub fn push_binding(
        &mut self,
        scope: LocalScopeId,
        name: SymbolName,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<LocalBindingSymbolId, BoundUnitConstructionError> {
        self.local_symbols
            .push_binding(scope, name, anchors, ordinal, is_recovered)
            .map_err(Into::into)
    }

    /// Allocates one local constant without making it visible by name.
    pub fn push_constant(
        &mut self,
        scope: LocalScopeId,
        name: SymbolName,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<LocalConstantSymbolId, BoundUnitConstructionError> {
        self.local_symbols
            .push_constant(scope, name, anchors, ordinal, is_recovered)
            .map_err(Into::into)
    }

    /// Makes a previously allocated named local participate in ordinary lookup.
    pub fn activate_local(
        &mut self,
        scope: LocalScopeId,
        symbol: impl Into<AnyLocalSymbolId>,
    ) -> Result<(), BoundUnitConstructionError> {
        let symbol = symbol.into();

        if self.activated_locals.contains(&symbol) {
            return Err(BoundUnitConstructionError::LocalAlreadyActivated(symbol));
        }

        self.local_symbols.insert_local_name(scope, symbol)?;

        self.activated_locals.insert(symbol);

        Ok(())
    }

    /// Adds one declaration-surface symbol to a lexical scope's ordinary-name index.
    pub fn insert_surface_name(
        &mut self,
        scope: LocalScopeId,
        name: SymbolName,
        symbol: AnySymbolId,
    ) -> Result<(), BoundUnitConstructionError> {
        self.local_symbols
            .insert_surface_name(scope, name, symbol)
            .map_err(Into::into)
    }

    /// Creates a contextual postcondition result in an exact contract scope.
    pub fn push_postcondition_result(
        &mut self,
        scope: LocalScopeId,
        syntax: SyntaxAnchor,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<PostconditionResultSymbolId, BoundUnitConstructionError> {
        self.local_symbols
            .push_postcondition_result(scope, syntax, ordinal, is_recovered)
            .map_err(Into::into)
    }

    /// Creates an anonymous callable and its exact capture-free callable lookup boundary.
    pub fn push_anonymous_callable(
        &mut self,
        introduction_scope: LocalScopeId,
        source: BoundSourceAnchor,
        visibility_start: TextSize,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<AnonymousCallableBoundary, BoundUnitConstructionError> {
        let expected_source = self.key.source().syntax().source_id();
        let actual_source = source.syntax().source_id();

        if actual_source != expected_source {
            return Err(
                BoundUnitConstructionError::AnonymousCallableSourceMismatch {
                    expected: expected_source,
                    actual: actual_source,
                },
            );
        }

        let expected = self.key.source().source_version();
        let actual = source.source_version();

        if actual != expected {
            return Err(
                BoundUnitConstructionError::AnonymousCallableSourceVersionMismatch {
                    expected,
                    actual,
                },
            );
        }

        let syntax = source.syntax();
        let callable_scope = self.local_symbols.push_scope(
            Some(introduction_scope),
            LocalScopeBoundary::Callable,
            syntax,
            visibility_start,
        )?;

        let callable = self.local_symbols.push_anonymous_callable(
            introduction_scope,
            callable_scope,
            [syntax],
            ordinal,
            is_recovered,
        )?;

        // Bound unit keys use shared immutable identity storage; the nested key must retain the
        // enclosing unit independently of this mutable builder.
        let unit = BoundUnitKey::anonymous_callable(self.key.clone(), source);

        Ok(AnonymousCallableBoundary {
            callable,
            scope: callable_scope,
            unit,
        })
    }

    /// Allocates and activates one parameter in its validated anonymous callable boundary.
    pub fn push_anonymous_parameter(
        &mut self,
        boundary: &AnonymousCallableBoundary,
        name: SymbolName,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: SymbolOrdinal,
        is_recovered: bool,
    ) -> Result<AnonymousCallableParameterSymbolId, BoundUnitConstructionError> {
        let BoundUnitKeyData::AnonymousCallable(unit) = boundary.unit.data() else {
            return Err(BoundUnitConstructionError::AnonymousCallableBoundaryMismatch);
        };

        if unit.enclosing() != &self.key {
            return Err(BoundUnitConstructionError::AnonymousCallableBoundaryMismatch);
        }

        let parameter = self.local_symbols.push_anonymous_parameter(
            boundary.callable,
            boundary.scope,
            name,
            anchors,
            ordinal,
            is_recovered,
        )?;

        self.local_symbols
            .insert_local_name(boundary.scope, parameter.into())?;

        self.activated_locals.insert(parameter.into());

        Ok(parameter)
    }

    /// Freezes the bound tree and local snapshot into one construction result.
    pub fn finish(self) -> Result<BoundUnitConstructionResult, BoundUnitConstructionError> {
        let local_symbols = self.local_symbols.finish()?;
        let tree = self.tree.finish();

        Ok(BoundUnitConstructionResult {
            key: self.key,
            tree,
            local_symbols,
            root_scope: self.root_scope,
        })
    }
}

fn local_region_key(key: &BoundUnitKey) -> LocalSymbolRegionKey {
    let (owner, role, anchors) = local_region_key_parts(key);

    match LocalSymbolRegionKey::try_new(owner, role, anchors, None) {
        Some(key) => key,
        None => unreachable!("bound unit keys always contain one source anchor"),
    }
}

fn local_region_key_parts(
    key: &BoundUnitKey,
) -> (SymbolKey, LocalSymbolRegionRole, Vec<SyntaxAnchor>) {
    match key.data() {
        BoundUnitKeyData::CallableBody(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::CallableBody,
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::AnonymousCallable(unit) => {
            let (owner, _, mut anchors) = local_region_key_parts(unit.enclosing());

            if !matches!(
                unit.enclosing().data(),
                BoundUnitKeyData::AnonymousCallable(_)
            ) {
                anchors.clear();
            }

            anchors.push(unit.source().syntax());

            (owner, LocalSymbolRegionRole::AnonymousCallable, anchors)
        }
        BoundUnitKeyData::RuntimeDefault(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(runtime_default_fact(unit.owner().kind())),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::ConstantTemplate(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::ConstantDefinition),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::PredicateDefinition(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::PredicateDefinition),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::Constraint(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::GenericConstraints),
            vec![unit.source().syntax()],
        ),
        BoundUnitKeyData::ContractClause(unit) => (
            shared_symbol_key(unit.owner()),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::CallableContracts),
            vec![unit.source().syntax()],
        ),
    }
}

fn runtime_default_fact(owner: SymbolKind) -> SymbolFactKind {
    match owner {
        SymbolKind::CallableParameterDefaultProvider => SymbolFactKind::CallableParameterDefault,
        SymbolKind::StructFieldDefaultProvider => SymbolFactKind::StructFieldDefault,
        SymbolKind::UnionPayloadDefaultProvider => SymbolFactKind::UnionPayloadFieldDefault,
        _ => unreachable!("bound runtime-default keys validate their owner kind"),
    }
}

fn shared_symbol_key(key: &SymbolKey) -> SymbolKey {
    // Symbol keys are immutable trees backed by shared identity storage.
    key.clone()
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey, BoundUnitKind};
    use bray_declarations::{
        SyntaxAnchor, discover_source_unit_declarations, merge_declaration_chunks,
    };
    use bray_source::{SourceVersion, TextSize};
    use bray_symbols::{
        AnyLocalSymbolId, LocalScopeBoundary, LocalSymbolBuildError, LocalSymbolRegionId,
        LocalSymbolRegionRole, PackageIdentity, SymbolGraph, SymbolName, SymbolOrdinal,
    };
    use bray_testing::{test_source_at, test_source_store};

    use super::{BoundUnitConstructionError, BoundUnitLocalBuilder};

    #[test]
    fn construction_is_deterministic_and_freezes_both_stores() {
        let fixture = fixture();

        let first = representative_unit(&fixture, LocalSymbolRegionId::new(4));
        let second = representative_unit(&fixture, LocalSymbolRegionId::new(4));

        assert_eq!(first, second);
        assert_eq!(first.tree().unit(), BoundUnitId::new(7));
        assert_eq!(first.view().unit(), BoundUnitId::new(7));

        let [binding] = first.local_symbols().bindings() else {
            panic!("representative unit must contain one binding");
        };

        assert_eq!(binding.key().anchors(), &[fixture.first, fixture.second]);
        assert_eq!(binding.key().region(), first.local_symbols().key());
    }

    #[test]
    fn recovered_identities_remain_hidden_until_activated() {
        let fixture = fixture();
        let mut builder = builder(&fixture, LocalSymbolRegionId::new(5));
        let root = builder.root_scope();
        let block = push_scope(
            &mut builder,
            root,
            LocalScopeBoundary::Block,
            fixture.first,
            3,
        );
        let binding = push_binding(&mut builder, block, fixture.first, true);

        let result = finish(builder);
        let Some(scope) = result.local_symbols().scope(block) else {
            panic!("block scope must be published");
        };

        assert_eq!(scope.visibility_start(), TextSize::new(3));
        assert!(scope.local_symbols_named("value").is_empty());
        assert!(
            result
                .local_symbols()
                .binding(binding)
                .is_some_and(|symbol| symbol.is_recovered())
        );
    }

    #[test]
    fn malformed_pattern_identity_is_reported_without_panicking() {
        let fixture = fixture();
        let mut builder = builder(&fixture, LocalSymbolRegionId::new(6));
        let root = builder.root_scope();

        assert_eq!(
            builder.push_binding(root, symbol_name("missing"), [], None, true),
            Err(BoundUnitConstructionError::LocalSymbol(
                LocalSymbolBuildError::MissingSyntaxAnchor
            ))
        );

        assert!(finish(builder).local_symbols().bindings().is_empty());
    }

    #[test]
    fn contextual_results_require_contract_scopes() {
        let fixture = fixture();
        let mut builder = builder(&fixture, LocalSymbolRegionId::new(7));
        let root = builder.root_scope();

        assert_eq!(
            builder.push_postcondition_result(root, fixture.first, None, true),
            Err(BoundUnitConstructionError::LocalSymbol(
                LocalSymbolBuildError::InvalidScopeBoundary
            ))
        );

        let contract = push_scope(
            &mut builder,
            root,
            LocalScopeBoundary::Contract,
            fixture.first,
            4,
        );

        let result = match builder.push_postcondition_result(
            contract,
            fixture.first,
            Some(SymbolOrdinal::new(0)),
            true,
        ) {
            Ok(result) => result,
            Err(error) => panic!("contract result must build: {error:?}"),
        };

        let snapshot = finish(builder);

        assert_eq!(
            snapshot
                .local_symbols()
                .scope(contract)
                .and_then(|scope| scope.postcondition_result()),
            Some(result)
        );
    }

    #[test]
    fn anonymous_boundaries_stop_capture_and_own_exact_parameters() {
        let fixture = fixture();
        let mut builder = builder(&fixture, LocalSymbolRegionId::new(8));
        let root = builder.root_scope();
        let outer_binding = push_binding(&mut builder, root, fixture.first, false);

        assert_eq!(builder.activate_local(root, outer_binding), Ok(()));

        let boundary = push_anonymous_callable(&mut builder, root, &fixture);
        let parameter = match builder.push_anonymous_parameter(
            &boundary,
            symbol_name("parameter"),
            [fixture.first],
            SymbolOrdinal::new(0),
            false,
        ) {
            Ok(parameter) => parameter,
            Err(error) => panic!("anonymous parameter must build: {error:?}"),
        };

        let callable = boundary.callable();
        let callable_scope = boundary.scope();

        assert_eq!(boundary.unit().kind(), BoundUnitKind::AnonymousCallable);

        let mut nested_builder = match BoundUnitLocalBuilder::new(
            BoundUnitId::new(8),
            boundary.unit().clone(),
            LocalSymbolRegionId::new(8),
            TextSize::ZERO,
        ) {
            Ok(builder) => builder,
            Err(error) => panic!("nested test builder must build: {error:?}"),
        };

        assert_eq!(
            nested_builder.push_anonymous_parameter(
                &boundary,
                symbol_name("foreign"),
                [fixture.first],
                SymbolOrdinal::new(1),
                true,
            ),
            Err(BoundUnitConstructionError::AnonymousCallableBoundaryMismatch)
        );

        assert!(
            finish(nested_builder)
                .local_symbols()
                .anonymous_parameters()
                .is_empty()
        );

        let snapshot = finish(builder);
        let Some(scope) = snapshot.local_symbols().scope(callable_scope) else {
            panic!("callable scope must be published");
        };

        assert_eq!(scope.boundary(), LocalScopeBoundary::Callable);
        assert_eq!(scope.parent(), Some(root));
        assert!(scope.local_symbols_named("value").is_empty());
        assert_eq!(
            scope.local_symbols_named("parameter"),
            &[AnyLocalSymbolId::from(parameter)]
        );
        assert_eq!(
            snapshot
                .local_symbols()
                .anonymous_callable(callable)
                .map(|record| record.callable_scope()),
            Some(callable_scope)
        );
    }

    #[test]
    fn nested_anonymous_regions_use_canonical_lambda_anchor_paths() {
        let fixture = fixture();
        let outer = BoundUnitKey::anonymous_callable(
            fixture.key.clone(),
            BoundSourceAnchor::new(fixture.first, fixture.version),
        );
        let inner = BoundUnitKey::anonymous_callable(
            outer,
            BoundSourceAnchor::new(fixture.second, fixture.version),
        );

        let builder = match BoundUnitLocalBuilder::new(
            BoundUnitId::new(9),
            inner,
            LocalSymbolRegionId::new(9),
            TextSize::ZERO,
        ) {
            Ok(builder) => builder,
            Err(error) => panic!("nested anonymous region must build: {error:?}"),
        };

        let snapshot = finish(builder);

        assert_eq!(
            snapshot.local_symbols().key().role(),
            LocalSymbolRegionRole::AnonymousCallable
        );
        assert_eq!(
            snapshot.local_symbols().key().anchors(),
            &[fixture.first, fixture.second]
        );
    }

    #[test]
    fn mismatched_callable_versions_leave_no_partial_boundary() {
        let fixture = fixture();
        let mut builder = builder(&fixture, LocalSymbolRegionId::new(10));
        let root = builder.root_scope();
        let foreign = BoundSourceAnchor::new(fixture.foreign, fixture.version);
        let mismatched =
            BoundSourceAnchor::new(fixture.first, SourceVersion::new(fixture.version.raw() + 1));

        assert_eq!(
            builder.push_anonymous_callable(root, foreign, TextSize::ZERO, None, true),
            Err(
                BoundUnitConstructionError::AnonymousCallableSourceMismatch {
                    expected: fixture.first.source_id(),
                    actual: fixture.foreign.source_id(),
                }
            )
        );

        assert_eq!(
            builder.push_anonymous_callable(root, mismatched, TextSize::ZERO, None, true),
            Err(
                BoundUnitConstructionError::AnonymousCallableSourceVersionMismatch {
                    expected: fixture.version,
                    actual: mismatched.source_version(),
                }
            )
        );

        let snapshot = finish(builder);

        assert_eq!(snapshot.local_symbols().scopes().len(), 1);
        assert!(snapshot.local_symbols().anonymous_callables().is_empty());
    }

    #[test]
    fn builders_can_move_to_independent_parallel_workers() {
        fn assert_send<T: Send>() {}

        assert_send::<BoundUnitLocalBuilder>();
    }

    struct Fixture {
        key: BoundUnitKey,
        first: SyntaxAnchor,
        second: SyntaxAnchor,
        foreign: SyntaxAnchor,
        version: SourceVersion,
    }

    fn fixture() -> Fixture {
        let sources = test_source_store([
            "module app; func main() {} func next() {}",
            "module other; func foreign() {}",
        ]);
        let parsed = bray_parser::parse_source_unit(test_source_at(&sources, 0));
        let chunk = discover_source_unit_declarations(parsed.source_unit());
        let merged = merge_declaration_chunks([&chunk]);
        let foreign_parsed = bray_parser::parse_source_unit(test_source_at(&sources, 1));
        let foreign_chunk = discover_source_unit_declarations(foreign_parsed.source_unit());
        let foreign_merged = merge_declaration_chunks([&foreign_chunk]);

        let Some(package) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity must be valid");
        };

        let graph = match SymbolGraph::build_source(package, merged.table()) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        };

        let [function, second] = graph.functions() else {
            panic!("test source must declare two functions");
        };

        let first = function.syntax_anchor();
        let version = SourceVersion::new(3);
        let source = BoundSourceAnchor::new(first, version);
        let key = match BoundUnitKey::callable_body(function.key().clone(), source) {
            Some(key) => key,
            None => panic!("function must own a callable body"),
        };

        let Some(foreign_package) = PackageIdentity::try_new("foreign.package") else {
            panic!("foreign test package identity must be valid");
        };

        let foreign_graph = match SymbolGraph::build_source(foreign_package, foreign_merged.table())
        {
            Ok(graph) => graph,
            Err(error) => panic!("foreign test symbol graph must build: {error:?}"),
        };

        let [foreign] = foreign_graph.functions() else {
            panic!("foreign test source must declare one function");
        };

        Fixture {
            key,
            first,
            second: second.syntax_anchor(),
            foreign: foreign.syntax_anchor(),
            version,
        }
    }

    fn builder(fixture: &Fixture, region: LocalSymbolRegionId) -> BoundUnitLocalBuilder {
        match BoundUnitLocalBuilder::new(
            BoundUnitId::new(7),
            fixture.key.clone(),
            region,
            TextSize::ZERO,
        ) {
            Ok(builder) => builder,
            Err(error) => panic!("test unit builder must build: {error:?}"),
        }
    }

    fn representative_unit(
        fixture: &Fixture,
        region: LocalSymbolRegionId,
    ) -> super::BoundUnitConstructionResult {
        let mut builder = builder(fixture, region);
        let root = builder.root_scope();
        let block = push_scope(
            &mut builder,
            root,
            LocalScopeBoundary::Block,
            fixture.first,
            2,
        );

        let binding = match builder.push_binding(
            block,
            symbol_name("value"),
            [fixture.first, fixture.second],
            Some(SymbolOrdinal::new(0)),
            false,
        ) {
            Ok(binding) => binding,
            Err(error) => panic!("test binding must build: {error:?}"),
        };

        assert_eq!(builder.activate_local(block, binding), Ok(()));
        assert_eq!(
            builder.activate_local(block, binding),
            Err(BoundUnitConstructionError::LocalAlreadyActivated(
                binding.into()
            ))
        );

        finish(builder)
    }

    fn push_scope(
        builder: &mut BoundUnitLocalBuilder,
        parent: bray_symbols::LocalScopeId,
        boundary: LocalScopeBoundary,
        syntax: SyntaxAnchor,
        visibility_start: u32,
    ) -> bray_symbols::LocalScopeId {
        match builder.push_scope(parent, boundary, syntax, TextSize::new(visibility_start)) {
            Ok(scope) => scope,
            Err(error) => panic!("test scope must build: {error:?}"),
        }
    }

    fn push_binding(
        builder: &mut BoundUnitLocalBuilder,
        scope: bray_symbols::LocalScopeId,
        syntax: SyntaxAnchor,
        is_recovered: bool,
    ) -> bray_symbols::LocalBindingSymbolId {
        match builder.push_binding(
            scope,
            symbol_name("value"),
            [syntax],
            Some(SymbolOrdinal::new(0)),
            is_recovered,
        ) {
            Ok(binding) => binding,
            Err(error) => panic!("test binding must build: {error:?}"),
        }
    }

    fn push_anonymous_callable(
        builder: &mut BoundUnitLocalBuilder,
        introduction_scope: bray_symbols::LocalScopeId,
        fixture: &Fixture,
    ) -> super::AnonymousCallableBoundary {
        match builder.push_anonymous_callable(
            introduction_scope,
            BoundSourceAnchor::new(fixture.first, fixture.version),
            fixture.first.full_range().start(),
            Some(SymbolOrdinal::new(0)),
            false,
        ) {
            Ok(boundary) => boundary,
            Err(error) => panic!("anonymous callable must build: {error:?}"),
        }
    }

    fn finish(builder: BoundUnitLocalBuilder) -> super::BoundUnitConstructionResult {
        match builder.finish() {
            Ok(result) => result,
            Err(error) => panic!("test unit must freeze: {error:?}"),
        }
    }

    fn symbol_name(name: &str) -> SymbolName {
        match SymbolName::try_new(name) {
            Some(name) => name,
            None => panic!("test symbol name must be non-empty"),
        }
    }
}
