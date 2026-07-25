use std::collections::{BTreeMap, BTreeSet};

use bray_declarations::SyntaxAnchor;
use bray_source::TextSize;

use super::{
    AnonymousCallableParameterSymbol, AnonymousCallableParameterSymbolId, AnonymousCallableSymbol,
    AnonymousCallableSymbolId, AnyLocalSymbolId, LocalBindingSymbol, LocalBindingSymbolId,
    LocalConstantSymbol, LocalConstantSymbolId, LocalScope, LocalScopeBoundary, LocalScopeId,
    LocalSymbolBuildError, LocalSymbolKey, LocalSymbolRegionId, LocalSymbolRegionKey,
    LocalSymbolSnapshot, PostconditionResultSymbol, PostconditionResultSymbolId,
};
use crate::{AnySymbolId, SymbolKind, SymbolName, SymbolOrdinal};

#[derive(Debug)]
struct PendingScope {
    id: LocalScopeId,
    parent: Option<LocalScopeId>,
    boundary: LocalScopeBoundary,
    syntax: SyntaxAnchor,
    visibility_start: TextSize,
    local_names: BTreeMap<SymbolName, Vec<AnyLocalSymbolId>>,
    surface_names: BTreeMap<SymbolName, Vec<AnySymbolId>>,
    postcondition_result: Option<PostconditionResultSymbolId>,
}

impl PendingScope {
    fn finish(self) -> LocalScope {
        LocalScope {
            id: self.id,
            parent: self.parent,
            boundary: self.boundary,
            syntax: self.syntax,
            visibility_start: self.visibility_start,
            local_names: freeze_name_index(self.local_names),
            surface_names: freeze_name_index(self.surface_names),
            postcondition_result: self.postcondition_result,
        }
    }
}

#[derive(Debug)]
enum LocalSymbolMutation {
    LocalName { scope: usize, name: SymbolName },
    SurfaceName { scope: usize, name: SymbolName },
    AnonymousParameter { callable: usize },
    PostconditionResult { scope: usize },
}

/// A checkpoint in one local-symbol builder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalSymbolSnapshotCheckpoint {
    region: LocalSymbolRegionId,
    scopes: usize,
    bindings: usize,
    constants: usize,
    anonymous_callables: usize,
    anonymous_parameters: usize,
    postcondition_results: usize,
    mutations: usize,
}

/// Builder for one region's local symbols and lexical scopes.
#[derive(Debug)]
pub struct LocalSymbolSnapshotBuilder {
    region: LocalSymbolRegionId,
    key: LocalSymbolRegionKey,
    scopes: Vec<PendingScope>,
    bindings: Vec<LocalBindingSymbol>,
    constants: Vec<LocalConstantSymbol>,
    anonymous_callables: Vec<AnonymousCallableSymbol>,
    anonymous_callable_scopes: BTreeSet<LocalScopeId>,
    anonymous_callable_parameters: Vec<Vec<AnonymousCallableParameterSymbolId>>,
    anonymous_parameters: Vec<AnonymousCallableParameterSymbol>,
    postcondition_results: Vec<PostconditionResultSymbol>,
    mutations: Vec<LocalSymbolMutation>,
}

impl LocalSymbolSnapshotBuilder {
    /// Creates a builder for one exact local semantic region.
    pub fn new(region: LocalSymbolRegionId, key: LocalSymbolRegionKey) -> Self {
        Self {
            region,
            key,
            scopes: Vec::new(),
            bindings: Vec::new(),
            constants: Vec::new(),
            anonymous_callables: Vec::new(),
            anonymous_callable_scopes: BTreeSet::new(),
            anonymous_callable_parameters: Vec::new(),
            anonymous_parameters: Vec::new(),
            postcondition_results: Vec::new(),
            mutations: Vec::new(),
        }
    }

    /// Returns the region being constructed.
    pub const fn region(&self) -> LocalSymbolRegionId {
        self.region
    }

    /// Adds one lexical scope in canonical syntax order.
    pub fn push_scope(
        &mut self,
        parent: Option<LocalScopeId>,
        boundary: LocalScopeBoundary,
        syntax: SyntaxAnchor,
        visibility_start: TextSize,
    ) -> Result<LocalScopeId, LocalSymbolBuildError> {
        match (boundary, parent) {
            (LocalScopeBoundary::Root, Some(_)) => {
                return Err(LocalSymbolBuildError::RootHasParentScope);
            }
            (LocalScopeBoundary::Root, None) if self.scopes.is_empty() => {}
            (LocalScopeBoundary::Root, None) => {
                return Err(LocalSymbolBuildError::DuplicateRootScope);
            }
            (_, None) => return Err(LocalSymbolBuildError::MissingParentScope),
            (_, Some(parent)) => {
                self.checked_scope_index(parent)?;
            }
        }

        let id = LocalScopeId::new(self.region, checked_slot(self.scopes.len())?);

        self.scopes.push(PendingScope {
            id,
            parent,
            boundary,
            syntax,
            visibility_start,
            local_names: BTreeMap::new(),
            surface_names: BTreeMap::new(),
            postcondition_result: None,
        });

        Ok(id)
    }

    /// Adds one local binding without automatically making it visible by name.
    pub fn push_binding(
        &mut self,
        scope: LocalScopeId,
        name: SymbolName,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<LocalBindingSymbolId, LocalSymbolBuildError> {
        self.checked_scope_index(scope)?;

        let key = local_key(&self.key, SymbolKind::LocalBinding, anchors, ordinal)?;
        let id = LocalBindingSymbolId::new(self.region, checked_slot(self.bindings.len())?);

        self.bindings
            .push(LocalBindingSymbol::new(id, key, scope, name, is_recovered));

        Ok(id)
    }

    /// Adds one local constant without automatically making it visible by name.
    pub fn push_constant(
        &mut self,
        scope: LocalScopeId,
        name: SymbolName,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<LocalConstantSymbolId, LocalSymbolBuildError> {
        self.checked_scope_index(scope)?;

        let key = local_key(&self.key, SymbolKind::LocalConstant, anchors, ordinal)?;
        let id = LocalConstantSymbolId::new(self.region, checked_slot(self.constants.len())?);

        self.constants
            .push(LocalConstantSymbol::new(id, key, scope, name, is_recovered));

        Ok(id)
    }

    /// Adds one anonymous callable with a callable boundary below its introduction scope.
    pub fn push_anonymous_callable(
        &mut self,
        introduction_scope: LocalScopeId,
        callable_scope: LocalScopeId,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<AnonymousCallableSymbolId, LocalSymbolBuildError> {
        self.checked_scope_index(introduction_scope)?;

        let callable_scope_record = self.checked_scope(callable_scope)?;

        if callable_scope_record.boundary != LocalScopeBoundary::Callable
            || callable_scope_record.parent != Some(introduction_scope)
        {
            return Err(LocalSymbolBuildError::InvalidAnonymousCallableScope);
        }

        if self.anonymous_callable_scopes.contains(&callable_scope) {
            return Err(LocalSymbolBuildError::AnonymousCallableScopeAlreadyAssigned);
        }

        let key = local_key(&self.key, SymbolKind::AnonymousCallable, anchors, ordinal)?;
        let id = AnonymousCallableSymbolId::new(
            self.region,
            checked_slot(self.anonymous_callables.len())?,
        );

        self.anonymous_callables.push(AnonymousCallableSymbol::new(
            id,
            key,
            introduction_scope,
            callable_scope,
            is_recovered,
        ));
        self.anonymous_callable_scopes.insert(callable_scope);
        self.anonymous_callable_parameters.push(Vec::new());

        Ok(id)
    }

    /// Adds one parameter to its anonymous callable's exact boundary scope.
    pub fn push_anonymous_parameter(
        &mut self,
        callable: AnonymousCallableSymbolId,
        scope: LocalScopeId,
        name: SymbolName,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: SymbolOrdinal,
        is_recovered: bool,
    ) -> Result<AnonymousCallableParameterSymbolId, LocalSymbolBuildError> {
        let (callable_index, callable_record) = self.checked_anonymous_callable(callable)?;
        let scope_record = self.checked_scope(scope)?;

        if scope != callable_record.callable_scope()
            || scope_record.boundary != LocalScopeBoundary::Callable
            || scope_record.parent != Some(callable_record.scope())
        {
            return Err(LocalSymbolBuildError::AnonymousCallableParameterScopeMismatch);
        }

        let key = local_key(
            &self.key,
            SymbolKind::AnonymousCallableParameter,
            anchors,
            Some(ordinal),
        )?;

        let id = AnonymousCallableParameterSymbolId::new(
            self.region,
            checked_slot(self.anonymous_parameters.len())?,
        );

        let parameters = self
            .anonymous_callable_parameters
            .get_mut(callable_index)
            .ok_or(LocalSymbolBuildError::UnknownAnonymousCallable)?;

        self.anonymous_parameters
            .push(AnonymousCallableParameterSymbol::new(
                id,
                key,
                callable,
                scope,
                name,
                ordinal,
                is_recovered,
            ));

        parameters.push(id);

        self.mutations
            .push(LocalSymbolMutation::AnonymousParameter {
                callable: callable_index,
            });

        Ok(id)
    }

    /// Adds one contextual postcondition result and attaches it to its contract scope.
    pub fn push_postcondition_result(
        &mut self,
        scope: LocalScopeId,
        syntax: SyntaxAnchor,
        ordinal: Option<SymbolOrdinal>,
        is_recovered: bool,
    ) -> Result<PostconditionResultSymbolId, LocalSymbolBuildError> {
        let scope_index = self.checked_scope_index(scope)?;

        let scope_record = self
            .scopes
            .get(scope_index)
            .ok_or(LocalSymbolBuildError::UnknownScope)?;

        if scope_record.boundary != LocalScopeBoundary::Contract {
            return Err(LocalSymbolBuildError::InvalidScopeBoundary);
        }

        if scope_record.postcondition_result.is_some() {
            return Err(LocalSymbolBuildError::DuplicatePostconditionResult);
        }

        let key = local_key(
            &self.key,
            SymbolKind::PostconditionResult,
            [syntax],
            ordinal,
        )?;

        let id = PostconditionResultSymbolId::new(
            self.region,
            checked_slot(self.postcondition_results.len())?,
        );

        let scope_record = self
            .scopes
            .get_mut(scope_index)
            .ok_or(LocalSymbolBuildError::UnknownScope)?;

        self.postcondition_results
            .push(PostconditionResultSymbol::new(
                id,
                key,
                scope,
                syntax,
                is_recovered,
            ));

        scope_record.postcondition_result = Some(id);

        self.mutations
            .push(LocalSymbolMutation::PostconditionResult { scope: scope_index });

        Ok(id)
    }

    /// Returns the contextual postcondition result attached to one scope.
    pub fn postcondition_result(
        &self,
        scope: LocalScopeId,
    ) -> Result<Option<PostconditionResultSymbolId>, LocalSymbolBuildError> {
        let scope = self
            .scopes
            .get(self.checked_scope_index(scope)?)
            .ok_or(LocalSymbolBuildError::UnknownScope)?;

        Ok(scope.postcondition_result)
    }

    /// Inserts a named local symbol into one scope's ordinary-name index.
    pub fn insert_local_name(
        &mut self,
        scope: LocalScopeId,
        symbol: AnyLocalSymbolId,
    ) -> Result<(), LocalSymbolBuildError> {
        let scope_index = self.checked_scope_index(scope)?;

        let (symbol_scope, name) = self.local_symbol_scope_and_name(symbol)?;

        if symbol_scope != scope {
            return Err(LocalSymbolBuildError::SymbolOutsideScope);
        }

        // Symbol names use shared immutable text, so this clone releases the record borrow.
        let name = name.clone();

        // The scope index and rollback trail independently retain the shared name.
        let mutation_name = name.clone();

        let scope_record = self
            .scopes
            .get_mut(scope_index)
            .ok_or(LocalSymbolBuildError::UnknownScope)?;

        scope_record
            .local_names
            .entry(name)
            .or_default()
            .push(symbol);

        self.mutations.push(LocalSymbolMutation::LocalName {
            scope: scope_index,
            name: mutation_name,
        });

        Ok(())
    }

    /// Inserts a declaration-surface symbol into one scope's ordinary-name index.
    pub fn insert_surface_name(
        &mut self,
        scope: LocalScopeId,
        name: SymbolName,
        symbol: AnySymbolId,
    ) -> Result<(), LocalSymbolBuildError> {
        let scope_index = self.checked_scope_index(scope)?;

        // The scope index and rollback trail independently own the immutable shared name.
        let mutation_name = name.clone();

        let scope_record = self
            .scopes
            .get_mut(scope_index)
            .ok_or(LocalSymbolBuildError::UnknownScope)?;

        scope_record
            .surface_names
            .entry(name)
            .or_default()
            .push(symbol);
        self.mutations.push(LocalSymbolMutation::SurfaceName {
            scope: scope_index,
            name: mutation_name,
        });

        Ok(())
    }

    /// Returns the current lexical parent of one scope.
    pub fn scope_parent(
        &self,
        scope: LocalScopeId,
    ) -> Result<Option<LocalScopeId>, LocalSymbolBuildError> {
        let scope_index = self.checked_scope_index(scope)?;
        let scope = self
            .scopes
            .get(scope_index)
            .ok_or(LocalSymbolBuildError::UnknownScope)?;

        Ok(scope.parent)
    }

    /// Returns the current lexical boundary category of one scope.
    pub fn scope_boundary(
        &self,
        scope: LocalScopeId,
    ) -> Result<LocalScopeBoundary, LocalSymbolBuildError> {
        self.checked_scope(scope).map(|scope| scope.boundary)
    }

    /// Returns named local candidates currently visible in one scope.
    pub fn local_symbols_named(
        &self,
        scope: LocalScopeId,
        name: &str,
    ) -> Result<&[AnyLocalSymbolId], LocalSymbolBuildError> {
        let scope_index = self.checked_scope_index(scope)?;
        let scope = self
            .scopes
            .get(scope_index)
            .ok_or(LocalSymbolBuildError::UnknownScope)?;

        Ok(scope.local_names.get(name).map_or(&[], Vec::as_slice))
    }

    /// Returns whether one current named local candidate contains recovery.
    pub fn local_symbol_is_recovered(
        &self,
        symbol: AnyLocalSymbolId,
    ) -> Result<bool, LocalSymbolBuildError> {
        if symbol.region() != self.region {
            return Err(LocalSymbolBuildError::ForeignRegion);
        }

        match symbol {
            AnyLocalSymbolId::Binding(id) => self
                .bindings
                .get(
                    id.to_index()
                        .ok_or(LocalSymbolBuildError::UnknownLocalSymbol)?,
                )
                .map(LocalBindingSymbol::is_recovered),
            AnyLocalSymbolId::Constant(id) => self
                .constants
                .get(
                    id.to_index()
                        .ok_or(LocalSymbolBuildError::UnknownLocalSymbol)?,
                )
                .map(LocalConstantSymbol::is_recovered),
            AnyLocalSymbolId::AnonymousCallableParameter(id) => self
                .anonymous_parameters
                .get(
                    id.to_index()
                        .ok_or(LocalSymbolBuildError::UnknownLocalSymbol)?,
                )
                .map(AnonymousCallableParameterSymbol::is_recovered),
            AnyLocalSymbolId::PostconditionResult(id) => self
                .postcondition_results
                .get(
                    id.to_index()
                        .ok_or(LocalSymbolBuildError::UnknownLocalSymbol)?,
                )
                .map(PostconditionResultSymbol::is_recovered),
            AnyLocalSymbolId::AnonymousCallable(_) => None,
        }
        .ok_or(LocalSymbolBuildError::UnknownLocalSymbol)
    }

    /// Returns declaration-surface candidates currently visible in one scope.
    pub fn surface_symbols_named(
        &self,
        scope: LocalScopeId,
        name: &str,
    ) -> Result<&[AnySymbolId], LocalSymbolBuildError> {
        let scope_index = self.checked_scope_index(scope)?;
        let scope = self
            .scopes
            .get(scope_index)
            .ok_or(LocalSymbolBuildError::UnknownScope)?;

        Ok(scope.surface_names.get(name).map_or(&[], Vec::as_slice))
    }

    /// Captures the current builder state for later rollback.
    pub const fn checkpoint(&self) -> LocalSymbolSnapshotCheckpoint {
        LocalSymbolSnapshotCheckpoint {
            region: self.region,
            scopes: self.scopes.len(),
            bindings: self.bindings.len(),
            constants: self.constants.len(),
            anonymous_callables: self.anonymous_callables.len(),
            anonymous_parameters: self.anonymous_parameters.len(),
            postcondition_results: self.postcondition_results.len(),
            mutations: self.mutations.len(),
        }
    }

    /// Returns whether this builder can restore the supplied checkpoint without mutation.
    pub fn can_rollback_to(&self, checkpoint: LocalSymbolSnapshotCheckpoint) -> bool {
        checkpoint.region == self.region
            && checkpoint.scopes <= self.scopes.len()
            && checkpoint.bindings <= self.bindings.len()
            && checkpoint.constants <= self.constants.len()
            && checkpoint.anonymous_callables <= self.anonymous_callables.len()
            && checkpoint.anonymous_parameters <= self.anonymous_parameters.len()
            && checkpoint.postcondition_results <= self.postcondition_results.len()
            && checkpoint.mutations <= self.mutations.len()
    }

    /// Restores all local records and indexes to a checkpoint from this region.
    pub fn rollback(&mut self, checkpoint: LocalSymbolSnapshotCheckpoint) -> bool {
        if !self.can_rollback_to(checkpoint) {
            return false;
        }

        for mutation in self.mutations.drain(checkpoint.mutations..).rev() {
            match mutation {
                LocalSymbolMutation::LocalName { scope, name } => {
                    let Some(scope) = self.scopes.get_mut(scope) else {
                        return false;
                    };

                    rollback_name_entry(&mut scope.local_names, &name);
                }
                LocalSymbolMutation::SurfaceName { scope, name } => {
                    let Some(scope) = self.scopes.get_mut(scope) else {
                        return false;
                    };

                    rollback_name_entry(&mut scope.surface_names, &name);
                }
                LocalSymbolMutation::AnonymousParameter { callable } => {
                    let Some(parameters) = self.anonymous_callable_parameters.get_mut(callable)
                    else {
                        return false;
                    };

                    parameters.pop();
                }
                LocalSymbolMutation::PostconditionResult { scope } => {
                    let Some(scope) = self.scopes.get_mut(scope) else {
                        return false;
                    };

                    scope.postcondition_result = None;
                }
            }
        }

        self.scopes.truncate(checkpoint.scopes);
        self.bindings.truncate(checkpoint.bindings);
        self.constants.truncate(checkpoint.constants);
        self.anonymous_callables
            .truncate(checkpoint.anonymous_callables);
        self.anonymous_callable_parameters
            .truncate(checkpoint.anonymous_callables);
        self.anonymous_parameters
            .truncate(checkpoint.anonymous_parameters);
        self.postcondition_results
            .truncate(checkpoint.postcondition_results);

        self.anonymous_callable_scopes.clear();
        self.anonymous_callable_scopes.extend(
            self.anonymous_callables
                .iter()
                .map(AnonymousCallableSymbol::callable_scope),
        );

        true
    }

    /// Completes and returns the validated local-symbol snapshot.
    pub fn finish(self) -> Result<LocalSymbolSnapshot, LocalSymbolBuildError> {
        if self.scopes.is_empty() {
            return Err(LocalSymbolBuildError::MissingRootScope);
        }

        let scopes = self
            .scopes
            .into_iter()
            .map(PendingScope::finish)
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let anonymous_callables = self
            .anonymous_callables
            .into_iter()
            .zip(self.anonymous_callable_parameters)
            .map(|(callable, parameters)| callable.with_parameters(parameters.into_boxed_slice()))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(LocalSymbolSnapshot {
            region: self.region,
            key: self.key,
            scopes,
            bindings: self.bindings.into_boxed_slice(),
            constants: self.constants.into_boxed_slice(),
            anonymous_callables,
            anonymous_parameters: self.anonymous_parameters.into_boxed_slice(),
            postcondition_results: self.postcondition_results.into_boxed_slice(),
        })
    }

    fn checked_scope_index(&self, id: LocalScopeId) -> Result<usize, LocalSymbolBuildError> {
        if id.region() != self.region {
            return Err(LocalSymbolBuildError::ForeignRegion);
        }

        let Some(index) = id.to_index() else {
            return Err(LocalSymbolBuildError::UnknownScope);
        };

        self.scopes
            .get(index)
            .map(|_| index)
            .ok_or(LocalSymbolBuildError::UnknownScope)
    }

    fn checked_scope(&self, id: LocalScopeId) -> Result<&PendingScope, LocalSymbolBuildError> {
        let index = self.checked_scope_index(id)?;

        self.scopes
            .get(index)
            .ok_or(LocalSymbolBuildError::UnknownScope)
    }

    fn checked_anonymous_callable(
        &self,
        id: AnonymousCallableSymbolId,
    ) -> Result<(usize, &AnonymousCallableSymbol), LocalSymbolBuildError> {
        if id.region() != self.region {
            return Err(LocalSymbolBuildError::ForeignRegion);
        }

        let Some(index) = id.to_index() else {
            return Err(LocalSymbolBuildError::UnknownAnonymousCallable);
        };

        let callable = self
            .anonymous_callables
            .get(index)
            .ok_or(LocalSymbolBuildError::UnknownAnonymousCallable)?;

        Ok((index, callable))
    }

    fn local_symbol_scope_and_name(
        &self,
        symbol: AnyLocalSymbolId,
    ) -> Result<(LocalScopeId, &SymbolName), LocalSymbolBuildError> {
        if symbol.region() != self.region {
            return Err(LocalSymbolBuildError::ForeignRegion);
        }

        match symbol {
            AnyLocalSymbolId::Binding(id) => self
                .bindings
                .get(
                    id.to_index()
                        .ok_or(LocalSymbolBuildError::SymbolHasNoOrdinaryName)?,
                )
                .map(|record| (record.scope(), record.name())),
            AnyLocalSymbolId::Constant(id) => self
                .constants
                .get(
                    id.to_index()
                        .ok_or(LocalSymbolBuildError::SymbolHasNoOrdinaryName)?,
                )
                .map(|record| (record.scope(), record.name())),
            AnyLocalSymbolId::AnonymousCallableParameter(id) => self
                .anonymous_parameters
                .get(
                    id.to_index()
                        .ok_or(LocalSymbolBuildError::SymbolHasNoOrdinaryName)?,
                )
                .map(|record| (record.scope(), record.name())),
            AnyLocalSymbolId::AnonymousCallable(_) | AnyLocalSymbolId::PostconditionResult(_) => {
                None
            }
        }
        .ok_or(LocalSymbolBuildError::SymbolHasNoOrdinaryName)
    }
}

fn checked_slot(length: usize) -> Result<u32, LocalSymbolBuildError> {
    u32::try_from(length).map_err(|_| LocalSymbolBuildError::CapacityExceeded)
}

fn local_key(
    region: &LocalSymbolRegionKey,
    kind: SymbolKind,
    anchors: impl IntoIterator<Item = SyntaxAnchor>,
    ordinal: Option<SymbolOrdinal>,
) -> Result<LocalSymbolKey, LocalSymbolBuildError> {
    // Region keys contain shared immutable symbol and anchor storage.
    LocalSymbolKey::try_new(region.clone(), kind, anchors, ordinal)
        .ok_or(LocalSymbolBuildError::MissingSyntaxAnchor)
}

fn freeze_name_index<I>(index: BTreeMap<SymbolName, Vec<I>>) -> BTreeMap<SymbolName, Box<[I]>> {
    index
        .into_iter()
        .map(|(name, symbols)| (name, symbols.into_boxed_slice()))
        .collect()
}

fn rollback_name_entry<I>(index: &mut BTreeMap<SymbolName, Vec<I>>, name: &SymbolName) {
    let remove_entry = match index.get_mut(name) {
        Some(entries) => {
            entries.pop();
            entries.is_empty()
        }
        None => false,
    };

    if remove_entry {
        index.remove(name);
    }
}

#[cfg(test)]
mod tests {
    use bray_declarations::{
        SyntaxAnchor, discover_source_unit_declarations, merge_declaration_chunks,
    };
    use bray_source::TextSize;
    use bray_testing::{test_source_at, test_source_store};

    use super::{LocalSymbolBuildError, LocalSymbolSnapshotBuilder};
    use crate::{
        AnonymousCallableParameterSymbolId, AnonymousCallableSymbolId, AnyLocalSymbolId,
        AnySymbolId, LocalBindingSymbolId, LocalScopeBoundary, LocalScopeId, LocalSymbolRegionId,
        LocalSymbolRegionKey, LocalSymbolRegionRole, LocalSymbolSnapshot, PackageIdentity,
        PostconditionResultSymbolId, SymbolGraph, SymbolKey, SymbolName, SymbolOrdinal,
        SymbolOrigin,
    };

    #[test]
    fn snapshot_preserves_scopes_recovery_and_duplicate_lookup_entries() {
        let (region_key, syntax, surface_symbol) = fixture();

        let mut builder = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(4), region_key);

        let root = scope(&mut builder, None, LocalScopeBoundary::Root, syntax);
        let block = scope(&mut builder, Some(root), LocalScopeBoundary::Block, syntax);
        let first = binding(&mut builder, block, "value", syntax, 0, false);
        let duplicate = binding(&mut builder, block, "value", syntax, 1, true);

        let constant = match builder.push_constant(
            block,
            symbol_name("limit"),
            [syntax],
            Some(SymbolOrdinal::new(0)),
            false,
        ) {
            Ok(constant) => constant,
            Err(error) => panic!("test local constant must build: {error:?}"),
        };

        assert_eq!(builder.insert_local_name(block, first.into()), Ok(()));
        assert_eq!(builder.insert_local_name(block, duplicate.into()), Ok(()));
        assert_eq!(builder.insert_local_name(block, constant.into()), Ok(()));

        assert_eq!(
            builder.insert_surface_name(root, symbol_name("surface"), surface_symbol),
            Ok(())
        );

        let snapshot = finish(builder);

        let Some(block_scope) = snapshot.scope(block) else {
            panic!("block scope must belong to the finished snapshot");
        };

        assert_eq!(block_scope.parent(), Some(root));

        assert_eq!(
            block_scope.local_symbols_named("value"),
            &[
                AnyLocalSymbolId::from(first),
                AnyLocalSymbolId::from(duplicate)
            ]
        );

        assert!(
            snapshot
                .binding(duplicate)
                .is_some_and(|symbol| symbol.is_recovered())
        );

        assert_eq!(
            block_scope.local_symbols_named("limit"),
            &[AnyLocalSymbolId::from(constant)]
        );

        assert!(snapshot.constant(constant).is_some());

        assert_eq!(snapshot.syntax_anchor(first.into()), Some(syntax));
        assert_eq!(snapshot.syntax_anchor(constant.into()), Some(syntax));

        assert_eq!(
            snapshot
                .scope(root)
                .map(|scope| scope.surface_symbols_named("surface")),
            Some(&[surface_symbol][..])
        );
    }

    #[test]
    fn anonymous_callables_parameters_and_contextual_results_remain_typed() {
        let (region_key, syntax, _) = fixture();

        let mut builder = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(8), region_key);

        let root = scope(&mut builder, None, LocalScopeBoundary::Root, syntax);
        let callable_scope = scope(
            &mut builder,
            Some(root),
            LocalScopeBoundary::Callable,
            syntax,
        );

        let callable = match builder.push_anonymous_callable(
            root,
            callable_scope,
            [syntax],
            Some(SymbolOrdinal::new(0)),
            false,
        ) {
            Ok(callable) => callable,
            Err(error) => panic!("test anonymous callable must build: {error:?}"),
        };

        let parameter = match builder.push_anonymous_parameter(
            callable,
            callable_scope,
            symbol_name("item"),
            [syntax],
            SymbolOrdinal::new(0),
            false,
        ) {
            Ok(parameter) => parameter,
            Err(error) => panic!("test anonymous parameter must build: {error:?}"),
        };

        assert_eq!(
            builder.insert_local_name(callable_scope, parameter.into()),
            Ok(())
        );

        let contract_scope = scope(
            &mut builder,
            Some(callable_scope),
            LocalScopeBoundary::Contract,
            syntax,
        );

        let result = match builder.push_postcondition_result(contract_scope, syntax, None, false) {
            Ok(result) => result,
            Err(error) => panic!("test postcondition result must build: {error:?}"),
        };

        assert_eq!(
            builder.push_postcondition_result(contract_scope, syntax, None, false),
            Err(LocalSymbolBuildError::DuplicatePostconditionResult)
        );

        assert_eq!(
            builder.postcondition_result(contract_scope),
            Ok(Some(result))
        );

        assert_eq!(builder.local_symbol_is_recovered(result.into()), Ok(false));

        let snapshot = finish(builder);

        assert_eq!(
            snapshot
                .anonymous_parameter(parameter)
                .map(|item| item.callable()),
            Some(callable)
        );

        assert_eq!(
            snapshot
                .anonymous_parameter(parameter)
                .map(|item| item.ordinal()),
            Some(SymbolOrdinal::new(0))
        );

        assert_eq!(
            snapshot
                .anonymous_callable(callable)
                .map(|item| item.parameters()),
            Some(&[parameter][..])
        );

        assert_eq!(
            snapshot
                .anonymous_callable(callable)
                .map(|item| item.callable_scope()),
            Some(callable_scope)
        );

        assert_eq!(
            snapshot
                .scope(contract_scope)
                .and_then(|scope| scope.postcondition_result()),
            Some(result)
        );

        assert!(snapshot.postcondition_result(result).is_some());

        assert_eq!(snapshot.syntax_anchor(callable.into()), Some(syntax));
        assert_eq!(snapshot.syntax_anchor(parameter.into()), Some(syntax));
        assert_eq!(snapshot.syntax_anchor(result.into()), Some(syntax));
    }

    #[test]
    fn anonymous_callable_and_parameter_scope_relationships_are_enforced() {
        let (region_key, syntax, _) = fixture();

        let mut builder = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(9), region_key);

        let root = scope(&mut builder, None, LocalScopeBoundary::Root, syntax);

        let callable_scope = scope(
            &mut builder,
            Some(root),
            LocalScopeBoundary::Callable,
            syntax,
        );

        let unrelated_callable_scope = scope(
            &mut builder,
            Some(root),
            LocalScopeBoundary::Callable,
            syntax,
        );

        let block_scope = scope(&mut builder, Some(root), LocalScopeBoundary::Block, syntax);

        let wrong_parent_callable_scope = scope(
            &mut builder,
            Some(block_scope),
            LocalScopeBoundary::Callable,
            syntax,
        );

        let callable = match builder.push_anonymous_callable(
            root,
            callable_scope,
            [syntax],
            Some(SymbolOrdinal::new(0)),
            false,
        ) {
            Ok(callable) => callable,
            Err(error) => panic!("test anonymous callable must build: {error:?}"),
        };

        assert_eq!(
            builder.push_anonymous_callable(
                root,
                block_scope,
                [syntax],
                Some(SymbolOrdinal::new(1)),
                false,
            ),
            Err(LocalSymbolBuildError::InvalidAnonymousCallableScope)
        );

        assert_eq!(
            builder.push_anonymous_callable(
                root,
                wrong_parent_callable_scope,
                [syntax],
                Some(SymbolOrdinal::new(2)),
                false,
            ),
            Err(LocalSymbolBuildError::InvalidAnonymousCallableScope)
        );

        assert_eq!(
            builder.push_anonymous_callable(
                root,
                callable_scope,
                [syntax],
                Some(SymbolOrdinal::new(3)),
                false,
            ),
            Err(LocalSymbolBuildError::AnonymousCallableScopeAlreadyAssigned)
        );

        assert_eq!(
            builder.push_anonymous_parameter(
                callable,
                unrelated_callable_scope,
                symbol_name("unrelated"),
                [syntax],
                SymbolOrdinal::new(0),
                false,
            ),
            Err(LocalSymbolBuildError::AnonymousCallableParameterScopeMismatch)
        );

        assert_eq!(
            builder.push_anonymous_parameter(
                callable,
                block_scope,
                symbol_name("block"),
                [syntax],
                SymbolOrdinal::new(1),
                false,
            ),
            Err(LocalSymbolBuildError::AnonymousCallableParameterScopeMismatch)
        );

        assert!(finish(builder).anonymous_parameters().is_empty());
    }

    #[test]
    fn foreign_ids_and_invalid_scope_shapes_are_rejected_without_panics() {
        let (snapshot_key, syntax, _) = fixture();

        // Region keys contain shared immutable identity and syntax storage.
        let empty_snapshot_key = snapshot_key.clone();

        let empty_builder =
            LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(0), empty_snapshot_key);

        assert_eq!(
            empty_builder.finish(),
            Err(LocalSymbolBuildError::MissingRootScope)
        );

        let mut builder =
            LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(1), snapshot_key);

        assert_eq!(
            builder.push_scope(None, LocalScopeBoundary::Block, syntax, TextSize::ZERO),
            Err(LocalSymbolBuildError::MissingParentScope)
        );

        let root = scope(&mut builder, None, LocalScopeBoundary::Root, syntax);

        assert_eq!(
            builder.push_postcondition_result(root, syntax, None, false),
            Err(LocalSymbolBuildError::InvalidScopeBoundary)
        );

        assert_eq!(
            builder.push_scope(None, LocalScopeBoundary::Root, syntax, TextSize::ZERO),
            Err(LocalSymbolBuildError::DuplicateRootScope)
        );

        let foreign_key = region_key(
            SymbolKey::compiler_known_environment(),
            SymbolOrdinal::new(2),
            syntax,
        );

        let mut foreign_builder =
            LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(2), foreign_key);

        let foreign_root = scope(&mut foreign_builder, None, LocalScopeBoundary::Root, syntax);

        let foreign_binding = binding(
            &mut foreign_builder,
            foreign_root,
            "foreign",
            syntax,
            0,
            false,
        );

        assert_eq!(
            builder.insert_local_name(root, foreign_binding.into()),
            Err(LocalSymbolBuildError::ForeignRegion)
        );

        assert_eq!(finish(builder).binding(foreign_binding), None);
    }

    #[test]
    fn canonical_construction_assigns_repeatable_category_slots_and_keys() {
        let (key, syntax, _) = fixture();

        // Region keys contain shared immutable identity and syntax storage.
        let second_key = key.clone();

        let first = snapshot_with_one_binding(LocalSymbolRegionId::new(6), key, syntax);
        let second = snapshot_with_one_binding(LocalSymbolRegionId::new(6), second_key, syntax);

        assert_eq!(first, second);

        let [binding] = first.bindings() else {
            panic!("test snapshot must contain one binding");
        };

        assert_eq!(binding.key().region(), first.key());
    }

    #[test]
    fn checkpoints_restore_local_records_and_existing_scope_indexes() {
        let (key, syntax, surface_symbol) = fixture();

        let mut builder = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(7), key);

        let root = scope(&mut builder, None, LocalScopeBoundary::Root, syntax);
        let checkpoint = builder.checkpoint();
        let abandoned = binding(&mut builder, root, "abandoned", syntax, 0, false);

        assert_eq!(builder.insert_local_name(root, abandoned.into()), Ok(()));

        assert_eq!(
            builder.insert_surface_name(root, symbol_name("surface"), surface_symbol),
            Ok(())
        );

        assert!(builder.rollback(checkpoint));

        let reused = binding(&mut builder, root, "reused", syntax, 0, false);
        let snapshot = finish(builder);

        assert_eq!(reused, abandoned);
        assert!(snapshot.binding(abandoned).is_some());

        let Some(scope) = snapshot.scope(root) else {
            panic!("root scope must be retained");
        };

        assert!(scope.local_symbols_named("abandoned").is_empty());
        assert!(scope.surface_symbols_named("surface").is_empty());
    }

    #[test]
    fn checkpoints_from_another_region_are_rejected_without_mutation() {
        let (key, syntax, _) = fixture();

        // Region keys retain shared immutable surface identity.
        let second_key = key.clone();

        let mut first = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(11), key);
        let second = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(12), second_key);

        let root = scope(&mut first, None, LocalScopeBoundary::Root, syntax);
        let binding = binding(&mut first, root, "retained", syntax, 0, false);

        assert!(!first.rollback(second.checkpoint()));
        assert!(finish(first).binding(binding).is_some());
    }

    #[test]
    fn rollback_trails_restore_callable_parameters_and_contract_results() {
        let (key, syntax, _) = fixture();

        let mut builder = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(13), key);

        let root = scope(&mut builder, None, LocalScopeBoundary::Root, syntax);

        let callable_scope = scope(
            &mut builder,
            Some(root),
            LocalScopeBoundary::Callable,
            syntax,
        );

        let contract_scope = scope(
            &mut builder,
            Some(callable_scope),
            LocalScopeBoundary::Contract,
            syntax,
        );

        let callable = match builder.push_anonymous_callable(
            root,
            callable_scope,
            [syntax],
            Some(SymbolOrdinal::new(0)),
            false,
        ) {
            Ok(callable) => callable,
            Err(error) => panic!("test callable must build: {error:?}"),
        };

        let checkpoint = builder.checkpoint();

        let abandoned_parameter =
            anonymous_parameter(&mut builder, callable, callable_scope, syntax);

        let abandoned_result = postcondition_result(&mut builder, contract_scope, syntax);

        assert_eq!(
            builder.insert_local_name(callable_scope, abandoned_parameter.into()),
            Ok(())
        );

        assert!(builder.rollback(checkpoint));

        let reused_parameter = anonymous_parameter(&mut builder, callable, callable_scope, syntax);
        let reused_result = postcondition_result(&mut builder, contract_scope, syntax);

        assert_eq!(abandoned_parameter, reused_parameter);
        assert_eq!(abandoned_result, reused_result);

        assert_eq!(
            builder.insert_local_name(callable_scope, reused_parameter.into()),
            Ok(())
        );

        let snapshot = finish(builder);

        assert_eq!(
            snapshot
                .anonymous_callable(callable)
                .map(|record| record.parameters()),
            Some(&[reused_parameter][..])
        );

        assert_eq!(
            snapshot
                .scope(contract_scope)
                .and_then(|scope| scope.postcondition_result()),
            Some(reused_result)
        );
    }

    #[test]
    fn local_snapshot_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<LocalSymbolSnapshot>();
        assert_send_sync::<super::super::LocalScope>();
        assert_send_sync::<AnyLocalSymbolId>();
    }

    fn fixture() -> (LocalSymbolRegionKey, SyntaxAnchor, AnySymbolId) {
        let sources = test_source_store(["module app; func main() {}"]);
        let parsed = bray_parser::parse_source_unit(test_source_at(&sources, 0));
        let chunk = discover_source_unit_declarations(parsed.source_unit());
        let merged = merge_declaration_chunks([&chunk]);

        let package = match PackageIdentity::try_new("test.package") {
            Some(package) => package,
            None => panic!("test package identity must be valid"),
        };

        let syntax = bray_syntax::SyntaxTree::compilation_unit([parsed.source_unit().clone()]);

        let graph = match SymbolGraph::build_source(package, merged.table(), &syntax) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        };

        let source_functions = graph
            .functions()
            .iter()
            .filter(|function| function.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [function] = source_functions.as_slice() else {
            panic!("test source must declare one function");
        };

        let Some(syntax) = function.syntax_anchor() else {
            panic!("source function must retain its syntax anchor");
        };

        // The region key intentionally shares the function's immutable surface identity.
        let key = region_key(function.key().clone(), SymbolOrdinal::new(0), syntax);

        (key, syntax, function.id().into())
    }

    fn region_key(
        owner: SymbolKey,
        ordinal: SymbolOrdinal,
        syntax: SyntaxAnchor,
    ) -> LocalSymbolRegionKey {
        match LocalSymbolRegionKey::try_new(
            owner,
            LocalSymbolRegionRole::CallableBody,
            [syntax],
            Some(ordinal),
        ) {
            Some(key) => key,
            None => panic!("test region key must be valid"),
        }
    }

    fn scope(
        builder: &mut LocalSymbolSnapshotBuilder,
        parent: Option<LocalScopeId>,
        boundary: LocalScopeBoundary,
        syntax: SyntaxAnchor,
    ) -> LocalScopeId {
        match builder.push_scope(parent, boundary, syntax, syntax.full_range().start()) {
            Ok(scope) => scope,
            Err(error) => panic!("test scope must build: {error:?}"),
        }
    }

    fn binding(
        builder: &mut LocalSymbolSnapshotBuilder,
        scope: LocalScopeId,
        name: &str,
        syntax: SyntaxAnchor,
        ordinal: u32,
        is_recovered: bool,
    ) -> LocalBindingSymbolId {
        match builder.push_binding(
            scope,
            symbol_name(name),
            [syntax],
            Some(SymbolOrdinal::new(ordinal)),
            is_recovered,
        ) {
            Ok(binding) => binding,
            Err(error) => panic!("test binding must build: {error:?}"),
        }
    }

    fn anonymous_parameter(
        builder: &mut LocalSymbolSnapshotBuilder,
        callable: AnonymousCallableSymbolId,
        scope: LocalScopeId,
        syntax: SyntaxAnchor,
    ) -> AnonymousCallableParameterSymbolId {
        match builder.push_anonymous_parameter(
            callable,
            scope,
            symbol_name("parameter"),
            [syntax],
            SymbolOrdinal::new(0),
            false,
        ) {
            Ok(parameter) => parameter,
            Err(error) => panic!("test parameter must build: {error:?}"),
        }
    }

    fn postcondition_result(
        builder: &mut LocalSymbolSnapshotBuilder,
        scope: LocalScopeId,
        syntax: SyntaxAnchor,
    ) -> PostconditionResultSymbolId {
        match builder.push_postcondition_result(scope, syntax, None, false) {
            Ok(result) => result,
            Err(error) => panic!("test postcondition result must build: {error:?}"),
        }
    }

    fn symbol_name(name: &str) -> SymbolName {
        match SymbolName::try_new(name) {
            Some(name) => name,
            None => panic!("test symbol name must be valid"),
        }
    }

    fn finish(builder: LocalSymbolSnapshotBuilder) -> LocalSymbolSnapshot {
        match builder.finish() {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test snapshot must publish: {error:?}"),
        }
    }

    fn snapshot_with_one_binding(
        region: LocalSymbolRegionId,
        key: LocalSymbolRegionKey,
        syntax: SyntaxAnchor,
    ) -> LocalSymbolSnapshot {
        let mut builder = LocalSymbolSnapshotBuilder::new(region, key);

        let root = scope(&mut builder, None, LocalScopeBoundary::Root, syntax);
        let binding = binding(&mut builder, root, "value", syntax, 0, false);

        assert_eq!(builder.insert_local_name(root, binding.into()), Ok(()));

        finish(builder)
    }
}
