use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundSourceAnchor, BoundTreeBuilder, BoundTreeCheckpoint, BoundUnitId, BoundUnitKey,
    BoundUnitKeyData, BoundUnitView,
};
use bray_declarations::SyntaxAnchor;
use bray_source::TextSize;
use bray_symbols::{
    AnonymousCallableParameterSymbolId, AnonymousCallableSymbolId, AnyLocalSymbolId, AnySymbolId,
    LocalBindingSymbolId, LocalConstantSymbolId, LocalScopeBoundary, LocalScopeId,
    LocalSymbolRegionId, LocalSymbolSnapshotBuilder, LocalSymbolSnapshotCheckpoint,
    PostconditionResultSymbolId, SymbolName, SymbolOrdinal,
};

use super::{
    AnonymousCallableBoundary, BoundUnitConstructionError, BoundUnitConstructionResult,
    local_region_key,
};

type AnonymousCallableIdentity = (LocalScopeId, SyntaxAnchor, Option<SymbolOrdinal>);
type AnonymousParameterIdentity = (AnonymousCallableSymbolId, SymbolOrdinal);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BoundUnitLocalCheckpoint {
    unit: BoundUnitId,
    region: LocalSymbolRegionId,
    tree: BoundTreeCheckpoint,
    local_symbols: LocalSymbolSnapshotCheckpoint,
    activated_locals: usize,
    anonymous_callables: usize,
    anonymous_parameters: usize,
}

/// Binder-owned construction of bound nodes, local symbols, and lexical scopes for one unit.
#[derive(Debug)]
pub(crate) struct BoundUnitLocalBuilder {
    key: BoundUnitKey,
    tree: BoundTreeBuilder,
    local_symbols: LocalSymbolSnapshotBuilder,
    root_scope: LocalScopeId,
    activated_locals: BTreeSet<AnyLocalSymbolId>,
    activated_local_log: Vec<AnyLocalSymbolId>,
    anonymous_callables: BTreeSet<AnonymousCallableIdentity>,
    anonymous_callable_log: Vec<AnonymousCallableIdentity>,
    anonymous_parameters: BTreeSet<AnonymousParameterIdentity>,
    anonymous_parameter_log: Vec<AnonymousParameterIdentity>,
}

impl BoundUnitLocalBuilder {
    pub(crate) fn new(
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
            activated_local_log: Vec::new(),
            anonymous_callables: BTreeSet::new(),
            anonymous_callable_log: Vec::new(),
            anonymous_parameters: BTreeSet::new(),
            anonymous_parameter_log: Vec::new(),
        })
    }

    pub(crate) const fn unit(&self) -> BoundUnitId {
        self.tree.unit()
    }

    pub(crate) const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    pub(crate) const fn region(&self) -> LocalSymbolRegionId {
        self.local_symbols.region()
    }

    pub(crate) const fn root_scope(&self) -> LocalScopeId {
        self.root_scope
    }

    pub(crate) const fn tree_mut(&mut self) -> &mut BoundTreeBuilder {
        &mut self.tree
    }

    pub(crate) fn view(&self) -> BoundUnitView<'_> {
        self.tree.view(&self.key)
    }

    pub(crate) fn push_scope(
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
    pub(crate) fn push_binding(
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

    pub(crate) fn push_constant(
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

    pub(crate) fn activate_local(
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
        self.activated_local_log.push(symbol);

        Ok(())
    }

    pub(crate) fn insert_surface_name(
        &mut self,
        scope: LocalScopeId,
        name: SymbolName,
        symbol: AnySymbolId,
    ) -> Result<(), BoundUnitConstructionError> {
        self.local_symbols
            .insert_surface_name(scope, name, symbol)
            .map_err(Into::into)
    }

    pub(crate) fn scope_parent(
        &self,
        scope: LocalScopeId,
    ) -> Result<Option<LocalScopeId>, BoundUnitConstructionError> {
        self.local_symbols.scope_parent(scope).map_err(Into::into)
    }

    pub(crate) fn local_symbols_named(
        &self,
        scope: LocalScopeId,
        name: &str,
    ) -> Result<&[AnyLocalSymbolId], BoundUnitConstructionError> {
        self.local_symbols
            .local_symbols_named(scope, name)
            .map_err(Into::into)
    }

    pub(crate) fn local_symbol_is_recovered(
        &self,
        symbol: AnyLocalSymbolId,
    ) -> Result<bool, BoundUnitConstructionError> {
        self.local_symbols
            .local_symbol_is_recovered(symbol)
            .map_err(Into::into)
    }

    pub(crate) fn surface_symbols_named(
        &self,
        scope: LocalScopeId,
        name: &str,
    ) -> Result<&[AnySymbolId], BoundUnitConstructionError> {
        self.local_symbols
            .surface_symbols_named(scope, name)
            .map_err(Into::into)
    }

    pub(crate) fn push_postcondition_result(
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

    pub(crate) fn push_anonymous_callable(
        &mut self,
        introduction_scope: LocalScopeId,
        source: BoundSourceAnchor,
        visibility_start: TextSize,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<AnonymousCallableBoundary, BoundUnitConstructionError> {
        self.validate_anonymous_source(source)?;

        let syntax = source.syntax();
        let identity = (introduction_scope, syntax, ordinal);

        if self.anonymous_callables.contains(&identity) {
            return Err(
                BoundUnitConstructionError::AnonymousCallableAlreadyAssigned {
                    introduction_scope,
                    ordinal,
                },
            );
        }

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

        self.anonymous_callables.insert(identity);
        self.anonymous_callable_log.push(identity);

        Ok(AnonymousCallableBoundary::new(
            callable,
            callable_scope,
            unit,
        ))
    }

    pub(crate) fn push_anonymous_parameter(
        &mut self,
        boundary: &AnonymousCallableBoundary,
        name: SymbolName,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: SymbolOrdinal,
        is_recovered: bool,
    ) -> Result<AnonymousCallableParameterSymbolId, BoundUnitConstructionError> {
        self.validate_anonymous_boundary(boundary)?;

        let identity = (boundary.callable(), ordinal);

        if self.anonymous_parameters.contains(&identity) {
            return Err(
                BoundUnitConstructionError::AnonymousCallableParameterAlreadyAssigned {
                    callable: boundary.callable(),
                    ordinal,
                },
            );
        }

        let parameter = self.local_symbols.push_anonymous_parameter(
            boundary.callable(),
            boundary.scope(),
            name,
            anchors,
            ordinal,
            is_recovered,
        )?;

        self.local_symbols
            .insert_local_name(boundary.scope(), parameter.into())?;

        self.activated_locals.insert(parameter.into());
        self.activated_local_log.push(parameter.into());

        self.anonymous_parameters.insert(identity);
        self.anonymous_parameter_log.push(identity);

        Ok(parameter)
    }

    pub(crate) fn checkpoint(&self) -> BoundUnitLocalCheckpoint {
        BoundUnitLocalCheckpoint {
            unit: self.unit(),
            region: self.region(),
            tree: self.tree.checkpoint(),
            local_symbols: self.local_symbols.checkpoint(),
            activated_locals: self.activated_local_log.len(),
            anonymous_callables: self.anonymous_callable_log.len(),
            anonymous_parameters: self.anonymous_parameter_log.len(),
        }
    }

    pub(crate) fn rollback(&mut self, checkpoint: BoundUnitLocalCheckpoint) -> bool {
        if checkpoint.unit != self.unit() || checkpoint.region != self.region() {
            return false;
        }

        if checkpoint.activated_locals > self.activated_local_log.len()
            || checkpoint.anonymous_callables > self.anonymous_callable_log.len()
            || checkpoint.anonymous_parameters > self.anonymous_parameter_log.len()
        {
            return false;
        }

        if !self.tree.can_rollback_to(checkpoint.tree)
            || !self.local_symbols.can_rollback_to(checkpoint.local_symbols)
        {
            return false;
        }

        let tree_rolled_back = self.tree.rollback(checkpoint.tree);
        let locals_rolled_back = self.local_symbols.rollback(checkpoint.local_symbols);

        if !tree_rolled_back || !locals_rolled_back {
            return false;
        }

        for symbol in self
            .activated_local_log
            .drain(checkpoint.activated_locals..)
        {
            self.activated_locals.remove(&symbol);
        }

        for identity in self
            .anonymous_callable_log
            .drain(checkpoint.anonymous_callables..)
        {
            self.anonymous_callables.remove(&identity);
        }

        for identity in self
            .anonymous_parameter_log
            .drain(checkpoint.anonymous_parameters..)
        {
            self.anonymous_parameters.remove(&identity);
        }

        true
    }

    pub(crate) fn finish(self) -> Result<BoundUnitConstructionResult, BoundUnitConstructionError> {
        let local_symbols = self.local_symbols.finish()?;
        let tree = self.tree.finish();

        Ok(BoundUnitConstructionResult::new(
            self.key,
            tree,
            local_symbols,
            self.root_scope,
        ))
    }

    fn validate_anonymous_source(
        &self,
        source: BoundSourceAnchor,
    ) -> Result<(), BoundUnitConstructionError> {
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

        Ok(())
    }

    fn validate_anonymous_boundary(
        &self,
        boundary: &AnonymousCallableBoundary,
    ) -> Result<(), BoundUnitConstructionError> {
        let BoundUnitKeyData::AnonymousCallable(unit) = boundary.unit().data() else {
            return Err(BoundUnitConstructionError::AnonymousCallableBoundaryMismatch);
        };

        if unit.enclosing() != &self.key {
            return Err(BoundUnitConstructionError::AnonymousCallableBoundaryMismatch);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundNodeOrigin, BoundSourceAnchor, BoundUnitId,
        BoundUnitKind,
    };
    use bray_source::{SourceVersion, TextSize};
    use bray_symbols::{
        AnyLocalSymbolId, LocalScopeBoundary, LocalSymbolBuildError, LocalSymbolRegionId,
        SemanticValueStore, SymbolOrdinal, TypeData,
    };

    use crate::unit::BoundUnitConstructionError;
    use crate::unit::builder::BoundUnitLocalBuilder;
    use crate::unit::test_support::{
        builder as new_builder, finish, fixture, push_anonymous_callable, push_binding, push_scope,
        representative_binding, symbol_name,
    };

    #[test]
    fn recovered_identities_remain_hidden_until_activated_once() {
        let fixture = fixture();

        let mut builder = new_builder(&fixture, LocalSymbolRegionId::new(5));

        let root = builder.root_scope();

        let block = push_scope(
            &mut builder,
            root,
            LocalScopeBoundary::Block,
            fixture.first,
            3,
        );

        let binding = push_binding(&mut builder, block, fixture.first, true);

        assert_eq!(builder.activate_local(block, binding), Ok(()));

        assert_eq!(
            builder.activate_local(block, binding),
            Err(BoundUnitConstructionError::LocalAlreadyActivated(
                binding.into()
            ))
        );

        let result = finish(builder);

        let Some(scope) = result.local_symbols().scope(block) else {
            panic!("block scope must be published");
        };

        assert_eq!(scope.visibility_start(), TextSize::new(3));

        assert_eq!(
            scope.local_symbols_named("value"),
            &[AnyLocalSymbolId::from(binding)]
        );

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

        let mut builder = new_builder(&fixture, LocalSymbolRegionId::new(6));

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

        let mut builder = new_builder(&fixture, LocalSymbolRegionId::new(7));

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
    fn anonymous_boundaries_reject_a_different_enclosing_unit() {
        let fixture = fixture();

        let (_, _, boundary) = builder_with_boundary(&fixture, LocalSymbolRegionId::new(8));

        let mut wrong_unit = match BoundUnitLocalBuilder::new(
            BoundUnitId::new(8),
            boundary.unit().clone(),
            LocalSymbolRegionId::new(8),
            TextSize::ZERO,
        ) {
            Ok(builder) => builder,
            Err(error) => panic!("nested test builder must build: {error:?}"),
        };

        assert_eq!(
            wrong_unit.push_anonymous_parameter(
                &boundary,
                symbol_name("wrong_unit"),
                [fixture.first],
                SymbolOrdinal::new(0),
                true,
            ),
            Err(BoundUnitConstructionError::AnonymousCallableBoundaryMismatch)
        );

        assert!(
            finish(wrong_unit)
                .local_symbols()
                .anonymous_parameters()
                .is_empty()
        );
    }

    #[test]
    fn anonymous_boundaries_reject_a_builder_for_another_region() {
        let fixture = fixture();

        let (_, _, boundary) = builder_with_boundary(&fixture, LocalSymbolRegionId::new(8));

        let mut wrong_region = new_builder(&fixture, LocalSymbolRegionId::new(9));

        assert_eq!(
            wrong_region.push_anonymous_parameter(
                &boundary,
                symbol_name("wrong_region"),
                [fixture.first],
                SymbolOrdinal::new(0),
                true,
            ),
            Err(BoundUnitConstructionError::LocalSymbol(
                LocalSymbolBuildError::ForeignRegion
            ))
        );

        assert!(
            finish(wrong_region)
                .local_symbols()
                .anonymous_parameters()
                .is_empty()
        );
    }

    #[test]
    fn anonymous_callable_and_parameter_identities_cannot_be_reused() {
        let fixture = fixture();

        let (mut builder, root, boundary) =
            builder_with_boundary(&fixture, LocalSymbolRegionId::new(10));

        let outer_binding = push_binding(&mut builder, root, fixture.first, false);

        assert_eq!(builder.activate_local(root, outer_binding), Ok(()));

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

        assert_eq!(
            builder.push_anonymous_parameter(
                &boundary,
                symbol_name("duplicate"),
                [fixture.second],
                SymbolOrdinal::new(0),
                true,
            ),
            Err(
                BoundUnitConstructionError::AnonymousCallableParameterAlreadyAssigned {
                    callable: boundary.callable(),
                    ordinal: SymbolOrdinal::new(0),
                }
            )
        );

        assert_eq!(
            builder.push_anonymous_callable(
                root,
                BoundSourceAnchor::new(fixture.first, fixture.version),
                fixture.first.full_range().start(),
                Some(SymbolOrdinal::new(0)),
                true,
            ),
            Err(
                BoundUnitConstructionError::AnonymousCallableAlreadyAssigned {
                    introduction_scope: root,
                    ordinal: Some(SymbolOrdinal::new(0)),
                }
            )
        );

        let callable_scope = boundary.scope();
        let callable = boundary.callable();
        let snapshot = finish(builder);

        let Some(scope) = snapshot.local_symbols().scope(callable_scope) else {
            panic!("callable scope must be published");
        };

        assert_eq!(snapshot.local_symbols().scopes().len(), 2);
        assert_eq!(snapshot.local_symbols().anonymous_parameters().len(), 1);

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
    fn mismatched_callable_sources_leave_no_partial_boundary() {
        let fixture = fixture();

        let mut builder = new_builder(&fixture, LocalSymbolRegionId::new(10));

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
    fn checkpoints_restore_local_identity_and_visibility_state() {
        let fixture = fixture();

        let mut builder = new_builder(&fixture, LocalSymbolRegionId::new(13));

        let root = builder.root_scope();
        let checkpoint = builder.checkpoint();
        let abandoned = push_binding(&mut builder, root, fixture.first, false);

        assert_eq!(builder.activate_local(root, abandoned), Ok(()));
        assert!(builder.rollback(checkpoint));

        let reused = push_binding(&mut builder, root, fixture.first, false);

        assert_eq!(reused, abandoned);
        assert_eq!(builder.activate_local(root, reused), Ok(()));

        let result = finish(builder);

        assert_eq!(
            result
                .local_symbols()
                .scope(root)
                .map(|scope| scope.local_symbols_named("value")),
            Some(&[AnyLocalSymbolId::from(reused)][..])
        );
    }

    #[test]
    fn rejected_composite_checkpoints_leave_tree_and_locals_unchanged() {
        let fixture = fixture();
        let region = LocalSymbolRegionId::new(14);

        let mut builder = new_builder(&fixture, region);

        let root = builder.root_scope();
        let retained = push_binding(&mut builder, root, fixture.first, false);

        assert_eq!(builder.activate_local(root, retained), Ok(()));

        let expression = push_error_expression(&mut builder, &fixture);
        let mut other = new_builder(&fixture, region);

        let other_root = other.root_scope();

        push_binding(&mut other, other_root, fixture.first, false);
        representative_binding(
            &mut other,
            other_root,
            [fixture.second],
            SymbolOrdinal::new(1),
            false,
        );

        let incompatible = other.checkpoint();

        assert!(!builder.rollback(incompatible));
        assert!(builder.view().expression(expression).is_some());

        let result = finish(builder);

        assert!(result.local_symbols().binding(retained).is_some());

        assert_eq!(
            result
                .local_symbols()
                .scope(root)
                .map(|scope| scope.local_symbols_named("value")),
            Some(&[AnyLocalSymbolId::from(retained)][..])
        );
    }

    #[test]
    fn builders_can_move_to_independent_parallel_workers() {
        fn assert_send<T: Send>() {}

        assert_send::<BoundUnitLocalBuilder>();
    }

    fn builder_with_boundary(
        fixture: &crate::unit::test_support::Fixture,
        region: LocalSymbolRegionId,
    ) -> (
        BoundUnitLocalBuilder,
        bray_symbols::LocalScopeId,
        crate::unit::AnonymousCallableBoundary,
    ) {
        let mut builder = new_builder(fixture, region);

        let root = builder.root_scope();
        let boundary = push_anonymous_callable(&mut builder, root, fixture);

        assert_eq!(boundary.unit().kind(), BoundUnitKind::AnonymousCallable);

        (builder, root, boundary)
    }

    fn push_error_expression(
        builder: &mut BoundUnitLocalBuilder,
        fixture: &crate::unit::test_support::Fixture,
    ) -> bray_bound_tree::BoundExpressionId {
        let values = match SemanticValueStore::try_new() {
            Ok(values) => values,
            Err(error) => panic!("test semantic store must build: {error:?}"),
        };

        let error_type = match values.intern_type(TypeData::Error) {
            Ok(error_type) => error_type,
            Err(error) => panic!("test error type must build: {error:?}"),
        };

        let origin =
            BoundNodeOrigin::source(BoundSourceAnchor::new(fixture.first, fixture.version));

        let expression = BoundExpression::Error(BoundErrorExpression::new(origin, error_type));

        match builder.tree_mut().push_expression(expression) {
            Ok(expression) => expression,
            Err(error) => panic!("test expression must build: {error:?}"),
        }
    }
}
