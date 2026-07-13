use std::collections::BTreeMap;

use bray_compiler_known::{
    CompilerKnownCatalog, CompilerKnownDeclarationId, CompilerKnownRepresentationTarget,
    CompilerKnownValueId, ImplementationHook, RepresentationRole,
};

use super::CompilerKnownSymbolBuildError;
use crate::{AnySymbolId, ExactSymbolId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RepresentationTarget {
    Symbol(AnySymbolId),
    Value(CompilerKnownValueId),
}

/// Compilation-local typed routes from compiler roles to semantic identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownSymbolRoleRegistry {
    representations: BTreeMap<RepresentationRole, RepresentationTarget>,
    symbol_representations: BTreeMap<AnySymbolId, RepresentationRole>,
    value_representations: BTreeMap<CompilerKnownValueId, RepresentationRole>,
    implementations: BTreeMap<ImplementationHook, Box<[AnySymbolId]>>,
    symbol_implementations: BTreeMap<AnySymbolId, ImplementationHook>,
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
                    let Some(symbol) = declaration_symbols.get(&declaration).copied() else {
                        return Err(
                            CompilerKnownSymbolBuildError::MissingRoleDeclarationSymbol {
                                declaration,
                            },
                        );
                    };

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

            let Some(symbol) = declaration_symbols.get(&declaration).copied() else {
                return Err(
                    CompilerKnownSymbolBuildError::MissingRoleDeclarationSymbol { declaration },
                );
            };

            implementations
                .entry(binding.hook())
                .or_default()
                .push(symbol);

            symbol_implementations.insert(symbol, binding.hook());
        }

        Ok(Self {
            representations,
            symbol_representations,
            value_representations,
            implementations: implementations
                .into_iter()
                .map(|(hook, symbols)| (hook, symbols.into_boxed_slice()))
                .collect(),
            symbol_implementations,
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

    /// Returns the implementation hook carried by an exact compiler-known symbol.
    pub fn symbol_implementation<I: ExactSymbolId>(&self, symbol: I) -> Option<ImplementationHook> {
        self.symbol_implementations.get(&symbol.into()).copied()
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

#[cfg(test)]
mod tests {
    use bray_compiler_known::{ImplementationHook, RepresentationRole};

    use super::CompilerKnownSymbolRoleRegistry;
    use crate::compiler_known::test_support::{build_provider, declaration_key};
    use crate::{FunctionSymbolId, StructSymbolId, UnionSymbolId};

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
}
