use std::collections::BTreeMap;

use bray_compiler_known::{
    CompilerKnownCatalog, CompilerKnownDeclarationId, CompilerKnownIterationRole,
    CompilerKnownOperationRole, CompilerKnownRepresentationTarget, CompilerKnownValueId,
    ImplementationHook, RepresentationRole,
};

use super::CompilerKnownSymbolBuildError;
use crate::{
    AnySymbolId, ExactSymbolId, NamedTypeSymbolId, TraitCallableMemberSymbolId, TraitSymbolId,
    TraitTypeMemberSymbolId, UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RepresentationTarget {
    Symbol(AnySymbolId),
    Value(CompilerKnownValueId),
}

/// Exact compilation-local symbols assigned to one compiler-known expression operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompilerKnownOperationContract {
    role: CompilerKnownOperationRole,
    trait_definition: TraitSymbolId,
    result_type_member: Option<TraitTypeMemberSymbolId>,
    fixed_callable_result_type: Option<NamedTypeSymbolId>,
    callable: Option<TraitCallableMemberSymbolId>,
}

/// Exact compilation-local symbols defining language iteration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompilerKnownIterationProtocol {
    iterable_trait: TraitSymbolId,
    iterable_element: TraitTypeMemberSymbolId,
    iterable_cursor: TraitTypeMemberSymbolId,
    iterable_iterate: TraitCallableMemberSymbolId,
    iterator_trait: TraitSymbolId,
    iterator_element: TraitTypeMemberSymbolId,
    iterator_next: TraitCallableMemberSymbolId,
}

/// Exact compiler-known declarations defining the `Result` union representation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompilerKnownResultRepresentation {
    success_variant: UnionVariantSymbolId,
    success_field: UnionPayloadFieldSymbolId,
    error_variant: UnionVariantSymbolId,
    error_field: UnionPayloadFieldSymbolId,
}

/// Exact compiler-known declarations defining the `Ordering` union representation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompilerKnownOrderingRepresentation {
    definition: crate::UnionSymbolId,
    less_variant: UnionVariantSymbolId,
    equal_variant: UnionVariantSymbolId,
    greater_variant: UnionVariantSymbolId,
}

impl CompilerKnownOrderingRepresentation {
    pub(super) const fn new(
        definition: crate::UnionSymbolId,
        less_variant: UnionVariantSymbolId,
        equal_variant: UnionVariantSymbolId,
        greater_variant: UnionVariantSymbolId,
    ) -> Self {
        Self {
            definition,
            less_variant,
            equal_variant,
            greater_variant,
        }
    }

    /// Returns the `Ordering` union definition.
    pub const fn definition(self) -> crate::UnionSymbolId {
        self.definition
    }

    /// Returns the `Ordering.Less` variant.
    pub const fn less_variant(self) -> UnionVariantSymbolId {
        self.less_variant
    }

    /// Returns the `Ordering.Equal` variant.
    pub const fn equal_variant(self) -> UnionVariantSymbolId {
        self.equal_variant
    }

    /// Returns the `Ordering.Greater` variant.
    pub const fn greater_variant(self) -> UnionVariantSymbolId {
        self.greater_variant
    }
}

impl CompilerKnownResultRepresentation {
    pub(super) const fn new(
        success_variant: UnionVariantSymbolId,
        success_field: UnionPayloadFieldSymbolId,
        error_variant: UnionVariantSymbolId,
        error_field: UnionPayloadFieldSymbolId,
    ) -> Self {
        Self {
            success_variant,
            success_field,
            error_variant,
            error_field,
        }
    }

    /// Returns the `Result.Ok` variant.
    pub const fn success_variant(self) -> UnionVariantSymbolId {
        self.success_variant
    }

    /// Returns the `Result.Ok.value` payload field.
    pub const fn success_field(self) -> UnionPayloadFieldSymbolId {
        self.success_field
    }

    /// Returns the `Result.Error` variant.
    pub const fn error_variant(self) -> UnionVariantSymbolId {
        self.error_variant
    }

    /// Returns the `Result.Error.error` payload field.
    pub const fn error_field(self) -> UnionPayloadFieldSymbolId {
        self.error_field
    }
}

/// Exact compiler-known declarations defining the `RunResult` union representation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompilerKnownRunResultRepresentation {
    completed_variant: UnionVariantSymbolId,
    completed_field: UnionPayloadFieldSymbolId,
    panicked_variant: UnionVariantSymbolId,
    panicked_field: UnionPayloadFieldSymbolId,
    cancelled_variant: UnionVariantSymbolId,
}

impl CompilerKnownRunResultRepresentation {
    pub(super) const fn new(
        completed_variant: UnionVariantSymbolId,
        completed_field: UnionPayloadFieldSymbolId,
        panicked_variant: UnionVariantSymbolId,
        panicked_field: UnionPayloadFieldSymbolId,
        cancelled_variant: UnionVariantSymbolId,
    ) -> Self {
        Self {
            completed_variant,
            completed_field,
            panicked_variant,
            panicked_field,
            cancelled_variant,
        }
    }

    /// Returns the `RunResult.Completed` variant.
    pub const fn completed_variant(self) -> UnionVariantSymbolId {
        self.completed_variant
    }

    /// Returns the `RunResult.Completed.value` payload field.
    pub const fn completed_field(self) -> UnionPayloadFieldSymbolId {
        self.completed_field
    }

    /// Returns the `RunResult.Panicked` variant.
    pub const fn panicked_variant(self) -> UnionVariantSymbolId {
        self.panicked_variant
    }

    /// Returns the `RunResult.Panicked.report` payload field.
    pub const fn panicked_field(self) -> UnionPayloadFieldSymbolId {
        self.panicked_field
    }

    /// Returns the `RunResult.Cancelled` variant.
    pub const fn cancelled_variant(self) -> UnionVariantSymbolId {
        self.cancelled_variant
    }
}

impl CompilerKnownIterationProtocol {
    /// Returns the `Iterable` trait definition.
    pub const fn iterable_trait(self) -> TraitSymbolId {
        self.iterable_trait
    }

    /// Returns the `Iterable.Element` type-valued member.
    pub const fn iterable_element(self) -> TraitTypeMemberSymbolId {
        self.iterable_element
    }

    /// Returns the `Iterable.Cursor` type-valued member.
    pub const fn iterable_cursor(self) -> TraitTypeMemberSymbolId {
        self.iterable_cursor
    }

    /// Returns the `Iterable.iterate` callable.
    pub const fn iterable_iterate(self) -> TraitCallableMemberSymbolId {
        self.iterable_iterate
    }

    /// Returns the `Iterator` trait definition.
    pub const fn iterator_trait(self) -> TraitSymbolId {
        self.iterator_trait
    }

    /// Returns the `Iterator.Element` type-valued member.
    pub const fn iterator_element(self) -> TraitTypeMemberSymbolId {
        self.iterator_element
    }

    /// Returns the `Iterator.next` callable.
    pub const fn iterator_next(self) -> TraitCallableMemberSymbolId {
        self.iterator_next
    }

    pub(super) fn symbols(self) -> [AnySymbolId; 7] {
        [
            self.iterable_trait.into(),
            self.iterable_element.into(),
            self.iterable_cursor.into(),
            self.iterable_iterate.into(),
            self.iterator_trait.into(),
            self.iterator_element.into(),
            self.iterator_next.into(),
        ]
    }
}

impl CompilerKnownOperationContract {
    /// Returns the language-defined expression operation role.
    pub const fn role(self) -> CompilerKnownOperationRole {
        self.role
    }

    /// Returns the exact compiler-known trait declaration.
    pub const fn trait_definition(self) -> TraitSymbolId {
        self.trait_definition
    }

    /// Returns the associated result member used by this operation, when required.
    pub const fn result_type_member(self) -> Option<TraitTypeMemberSymbolId> {
        self.result_type_member
    }

    /// Returns the exact named result type required from the selected callable, when fixed.
    pub const fn fixed_callable_result_type(self) -> Option<NamedTypeSymbolId> {
        self.fixed_callable_result_type
    }

    /// Returns the trait callable selected by this operation, when it has one.
    pub const fn callable(self) -> Option<TraitCallableMemberSymbolId> {
        self.callable
    }

    pub(super) fn symbols(self) -> impl Iterator<Item = AnySymbolId> {
        std::iter::once(self.trait_definition.into())
            .chain(self.result_type_member.map(Into::into))
            .chain(
                self.fixed_callable_result_type
                    .map(NamedTypeSymbolId::into_any),
            )
            .chain(self.callable.map(Into::into))
    }
}

/// Compilation-local typed routes from compiler roles to semantic identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownSymbolRoleRegistry {
    representations: BTreeMap<RepresentationRole, RepresentationTarget>,
    symbol_representations: BTreeMap<AnySymbolId, RepresentationRole>,
    value_representations: BTreeMap<CompilerKnownValueId, RepresentationRole>,
    implementations: BTreeMap<ImplementationHook, Box<[AnySymbolId]>>,
    symbol_implementations: BTreeMap<AnySymbolId, ImplementationHook>,
    operations: BTreeMap<CompilerKnownOperationRole, CompilerKnownOperationContract>,
    iteration: Option<CompilerKnownIterationProtocol>,
}

impl CompilerKnownSymbolRoleRegistry {
    pub(super) fn build(
        catalog: &CompilerKnownCatalog,
        declaration_symbols: &BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
    ) -> Result<Self, CompilerKnownSymbolBuildError> {
        let mut representations = BTreeMap::new();
        let mut symbol_representations = BTreeMap::new();
        let mut value_representations = BTreeMap::new();

        for binding in catalog.role_registry().representations() {
            let target = match binding.target() {
                CompilerKnownRepresentationTarget::Declaration(declaration) => {
                    let symbol = untyped_role_symbol(declaration_symbols, declaration)?;

                    symbol_representations.insert(symbol, binding.role());

                    RepresentationTarget::Symbol(symbol)
                }
                CompilerKnownRepresentationTarget::Value(value) => {
                    value_representations.insert(value, binding.role());

                    RepresentationTarget::Value(value)
                }
            };

            representations.insert(binding.role(), target);
        }

        let mut implementations = BTreeMap::<ImplementationHook, Vec<AnySymbolId>>::new();
        let mut symbol_implementations = BTreeMap::new();

        for binding in catalog.role_registry().implementations() {
            let declaration = binding.declaration();
            let symbol = untyped_role_symbol(declaration_symbols, declaration)?;

            implementations
                .entry(binding.hook())
                .or_default()
                .push(symbol);

            symbol_implementations.insert(symbol, binding.hook());
        }

        let mut operations = BTreeMap::new();

        for binding in catalog.role_registry().operations() {
            let trait_definition =
                role_symbol::<TraitSymbolId>(declaration_symbols, binding.trait_definition())?;

            let result_type_member = binding
                .result_type_member()
                .map(|declaration| {
                    role_symbol::<TraitTypeMemberSymbolId>(declaration_symbols, declaration)
                })
                .transpose()?;

            let fixed_callable_result_type = binding
                .fixed_callable_result_type()
                .map(|declaration| {
                    let symbol = untyped_role_symbol(declaration_symbols, declaration)?;

                    NamedTypeSymbolId::try_from_any(symbol).ok_or(
                        CompilerKnownSymbolBuildError::InvalidOperationRoleSymbol { declaration },
                    )
                })
                .transpose()?;

            let callable = binding
                .callable()
                .map(|declaration| {
                    role_symbol::<TraitCallableMemberSymbolId>(declaration_symbols, declaration)
                })
                .transpose()?;

            operations.insert(
                binding.role(),
                CompilerKnownOperationContract {
                    role: binding.role(),
                    trait_definition,
                    result_type_member,
                    fixed_callable_result_type,
                    callable,
                },
            );
        }

        let iteration = build_iteration_protocol(catalog, declaration_symbols)?;

        Ok(Self {
            representations,
            symbol_representations,
            value_representations,
            implementations: implementations
                .into_iter()
                .map(|(hook, symbols)| (hook, symbols.into_boxed_slice()))
                .collect(),
            symbol_implementations,
            operations,
            iteration,
        })
    }

    /// Returns the exact symbol carrying a representation role when it has the requested kind.
    pub fn representation_symbol<I: ExactSymbolId>(&self, role: RepresentationRole) -> Option<I> {
        let RepresentationTarget::Symbol(symbol) = self.representations.get(&role)? else {
            return None;
        };

        I::try_from_any(*symbol)
    }

    /// Returns the special value carrying a representation role.
    pub fn representation_value(&self, role: RepresentationRole) -> Option<CompilerKnownValueId> {
        let RepresentationTarget::Value(value) = self.representations.get(&role)? else {
            return None;
        };

        Some(*value)
    }

    /// Returns the representation role carried by an exact compiler-known symbol.
    pub fn symbol_representation<I: ExactSymbolId>(&self, symbol: I) -> Option<RepresentationRole> {
        self.symbol_representations.get(&symbol.into()).copied()
    }

    /// Returns the representation role carried by a compiler-known special value.
    pub fn value_representation(&self, value: CompilerKnownValueId) -> Option<RepresentationRole> {
        self.value_representations.get(&value).copied()
    }

    /// Returns implementation declarations having the requested exact symbol kind.
    pub fn implementation_symbols<'roles, I: ExactSymbolId + 'roles>(
        &'roles self,
        hook: ImplementationHook,
    ) -> impl Iterator<Item = I> + 'roles {
        self.implementations
            .get(&hook)
            .into_iter()
            .flat_map(|symbols| symbols.iter().copied())
            .filter_map(I::try_from_any)
    }

    /// Returns the implementation hook carried by a compiler-known symbol.
    pub fn symbol_implementation(
        &self,
        symbol: impl Into<AnySymbolId>,
    ) -> Option<ImplementationHook> {
        self.symbol_implementations.get(&symbol.into()).copied()
    }

    /// Resolves the exact symbols assigned to one expression operation role.
    pub fn operation_contract(
        &self,
        role: CompilerKnownOperationRole,
    ) -> Option<CompilerKnownOperationContract> {
        self.operations.get(&role).copied()
    }

    /// Returns the exact compiler-known iteration protocol when it is defined.
    pub const fn iteration_protocol(&self) -> Option<CompilerKnownIterationProtocol> {
        self.iteration
    }

    pub(super) fn representation_target(
        &self,
        role: RepresentationRole,
    ) -> Option<RepresentationTarget> {
        self.representations.get(&role).copied()
    }

    pub(super) fn implementation_symbol_ids(&self, hook: ImplementationHook) -> &[AnySymbolId] {
        self.implementations.get(&hook).map_or(&[], Box::as_ref)
    }
}

fn build_iteration_protocol(
    catalog: &CompilerKnownCatalog,
    declaration_symbols: &BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
) -> Result<Option<CompilerKnownIterationProtocol>, CompilerKnownSymbolBuildError> {
    let roles = catalog.role_registry();

    if roles.iterations().is_empty() {
        return Ok(None);
    }

    macro_rules! role {
        ($role:ident, $ty:ty) => {{
            let role = CompilerKnownIterationRole::$role;

            let declaration = roles
                .iteration_declaration(role)
                .ok_or(CompilerKnownSymbolBuildError::MissingIterationRole { role })?;

            let symbol = untyped_role_symbol(declaration_symbols, declaration)?;

            <$ty>::try_from_any(symbol)
                .ok_or(CompilerKnownSymbolBuildError::InvalidIterationRoleSymbol { declaration })?
        }};
    }

    Ok(Some(CompilerKnownIterationProtocol {
        iterable_trait: role!(IterableTrait, TraitSymbolId),
        iterable_element: role!(IterableElement, TraitTypeMemberSymbolId),
        iterable_cursor: role!(IterableCursor, TraitTypeMemberSymbolId),
        iterable_iterate: role!(IterableIterate, TraitCallableMemberSymbolId),
        iterator_trait: role!(IteratorTrait, TraitSymbolId),
        iterator_element: role!(IteratorElement, TraitTypeMemberSymbolId),
        iterator_next: role!(IteratorNext, TraitCallableMemberSymbolId),
    }))
}

fn role_symbol<I: ExactSymbolId>(
    declaration_symbols: &BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
    declaration: CompilerKnownDeclarationId,
) -> Result<I, CompilerKnownSymbolBuildError> {
    let symbol = untyped_role_symbol(declaration_symbols, declaration)?;

    I::try_from_any(symbol)
        .ok_or(CompilerKnownSymbolBuildError::InvalidOperationRoleSymbol { declaration })
}

fn untyped_role_symbol(
    declaration_symbols: &BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
    declaration: CompilerKnownDeclarationId,
) -> Result<AnySymbolId, CompilerKnownSymbolBuildError> {
    declaration_symbols
        .get(&declaration)
        .copied()
        .ok_or(CompilerKnownSymbolBuildError::MissingRoleDeclarationSymbol { declaration })
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::{CompilerKnownOperationRole, ImplementationHook, RepresentationRole};

    use super::CompilerKnownSymbolRoleRegistry;
    use crate::compiler_known::test_support::{build_provider, declaration_key};
    use crate::{
        FunctionSymbolId, NamedTypeSymbolId, PredicateSymbolId, StructSymbolId, SymbolProvider,
        TraitCallableMemberSymbolId, TraitSymbolId, TraitTypeMemberSymbolId,
        TypeCallableMemberSymbolId, UnionSymbolId, UnionVariantSymbolId,
    };

    #[test]
    fn roles_resolve_exact_symbols_without_catalog_key_lookup() {
        let provider = build_provider();
        let roles = provider.role_registry();

        let bool_symbol =
            roles.representation_symbol::<StructSymbolId>(RepresentationRole::ScalarBool);

        let memory_copy = roles
            .implementation_symbols::<FunctionSymbolId>(ImplementationHook::MemoryCopy)
            .collect::<Vec<_>>();

        assert_eq!(
            bool_symbol,
            provider.declaration_symbol(&declaration_key("Bool"))
        );

        assert_eq!(
            bool_symbol.and_then(|symbol| roles.symbol_representation(symbol)),
            Some(RepresentationRole::ScalarBool)
        );

        assert_eq!(
            memory_copy,
            provider
                .declaration_symbol(&declaration_key("MemoryCopy"))
                .into_iter()
                .collect::<Vec<_>>()
        );

        assert_eq!(
            memory_copy
                .first()
                .copied()
                .and_then(|symbol| roles.symbol_implementation(symbol)),
            Some(ImplementationHook::MemoryCopy)
        );

        assert_eq!(
            roles.representation_symbol::<UnionSymbolId>(RepresentationRole::ScalarBool),
            None
        );
    }

    #[test]
    fn special_values_remain_distinct_from_declaration_symbols() {
        let provider = build_provider();
        let roles: &CompilerKnownSymbolRoleRegistry = provider.role_registry();

        let true_value = roles.representation_value(RepresentationRole::BooleanTrue);

        assert_eq!(
            true_value.and_then(|value| roles.value_representation(value)),
            Some(RepresentationRole::BooleanTrue)
        );

        assert_eq!(
            roles.representation_symbol::<StructSymbolId>(RepresentationRole::BooleanTrue),
            None
        );

        assert_eq!(
            roles.representation_value(RepresentationRole::ScalarBool),
            None
        );
    }

    #[test]
    fn operation_roles_resolve_exact_category_specific_symbols() {
        let provider = build_provider();
        let roles = provider.role_registry();

        for role in CompilerKnownOperationRole::ALL {
            assert!(roles.operation_contract(*role).is_some(), "{role:?}");
        }

        let Some(addition) = roles.operation_contract(CompilerKnownOperationRole::BinaryAdd) else {
            panic!("binary addition must have an operation contract");
        };

        assert_eq!(
            Some(addition.trait_definition()),
            provider.declaration_symbol::<TraitSymbolId>(&declaration_key("Add"))
        );

        assert_eq!(
            addition.result_type_member(),
            provider.declaration_symbol::<TraitTypeMemberSymbolId>(&declaration_key("AddOutput"))
        );

        assert_eq!(
            addition.callable(),
            provider.declaration_symbol::<TraitCallableMemberSymbolId>(&declaration_key("AddCall"))
        );

        let Some(box_construction) =
            roles.operation_contract(CompilerKnownOperationRole::BoxConstruction)
        else {
            panic!("box construction must have an operation contract");
        };

        assert_eq!(box_construction.result_type_member(), None);
        assert_eq!(box_construction.fixed_callable_result_type(), None);
        assert_eq!(box_construction.callable(), None);

        let Some(comparison) = roles.operation_contract(CompilerKnownOperationRole::Comparison)
        else {
            panic!("comparison must have an operation contract");
        };

        assert_eq!(
            comparison.fixed_callable_result_type(),
            provider
                .declaration_symbol::<UnionSymbolId>(&declaration_key("Ordering"))
                .map(NamedTypeSymbolId::Union)
        );
    }

    #[test]
    fn execution_roles_materialize_as_category_specific_symbols() {
        let provider = build_provider();
        let roles = provider.role_registry();

        assert_eq!(
            roles.representation_symbol::<StructSymbolId>(RepresentationRole::Future),
            provider.declaration_symbol(&declaration_key("Future"))
        );

        assert_eq!(
            roles.representation_symbol::<StructSymbolId>(RepresentationRole::Task),
            provider.declaration_symbol(&declaration_key("Task"))
        );

        assert_eq!(
            roles.representation_symbol::<UnionSymbolId>(RepresentationRole::RunResult),
            provider.declaration_symbol(&declaration_key("RunResult"))
        );

        assert_eq!(
            roles.representation_symbol::<StructSymbolId>(RepresentationRole::PanicReport),
            provider.declaration_symbol(&declaration_key("PanicReport"))
        );

        let Some(run_result) =
            roles.representation_symbol::<UnionSymbolId>(RepresentationRole::RunResult)
        else {
            panic!("RunResult must carry the run-result representation role");
        };

        let Some(run_result) = SymbolProvider::<UnionSymbolId>::symbol(&provider, run_result)
        else {
            panic!("RunResult union symbol must resolve");
        };

        let variants = [
            "RunResultVariant0Completed",
            "RunResultVariant1Panicked",
            "RunResultVariant2Cancelled",
        ]
        .map(|key| {
            provider
                .declaration_symbol::<UnionVariantSymbolId>(&declaration_key(key))
                .unwrap_or_else(|| panic!("{key} must have a union-variant symbol"))
        });

        assert_eq!(run_result.variants(), variants);

        for (hook, key) in [
            (ImplementationHook::FutureStart, "FutureStart"),
            (ImplementationHook::TaskJoin, "TaskJoin"),
            (ImplementationHook::TaskCancel, "TaskCancel"),
        ] {
            assert_eq!(
                roles
                    .implementation_symbols::<TypeCallableMemberSymbolId>(hook)
                    .collect::<Vec<_>>(),
                provider
                    .declaration_symbol(&declaration_key(key))
                    .into_iter()
                    .collect::<Vec<_>>()
            );
        }

        for (hook, key) in [
            (ImplementationHook::BlockingExecution, "BlockingExecution"),
            (ImplementationHook::ComputeExecution, "ComputeExecution"),
            (
                ImplementationHook::MainThreadExecution,
                "MainThreadExecution",
            ),
        ] {
            assert_eq!(
                roles
                    .implementation_symbols::<PredicateSymbolId>(hook)
                    .collect::<Vec<_>>(),
                provider
                    .declaration_symbol(&declaration_key(key))
                    .into_iter()
                    .collect::<Vec<_>>()
            );
        }
    }
}
