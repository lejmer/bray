use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_compiler_known::{
    AvailabilityRule, CompilerKnownCatalog, CompilerKnownDeclarationId,
    CompilerKnownDeclarationKey, CompilerKnownDeclarationOwner, CompilerKnownOperationRole,
    CompilerKnownValueId, ImplementationHook, RepresentationRole,
};

use super::role::RepresentationTarget;
use super::{CompilerKnownOperationContract, CompilerKnownSymbolProvider};
use crate::availability::resolve_owned_availability;
use crate::{AnySymbolId, ExactSymbolId};

/// An immutable target-filtered view over one complete compiler-known symbol provider.
///
/// The underlying provider retains every catalog symbol. This view only controls which
/// declarations participate in target-dependent lookup and semantic requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableCompilerKnownSymbols {
    provider: Arc<CompilerKnownSymbolProvider>,
    declarations: Box<[AnySymbolId]>,
    declaration_ids: BTreeSet<AnySymbolId>,
    value_ids: BTreeSet<CompilerKnownValueId>,
}

impl AvailableCompilerKnownSymbols {
    pub(crate) fn build(
        provider: Arc<CompilerKnownSymbolProvider>,
        mut rule_is_available: impl FnMut(AvailabilityRule) -> bool,
    ) -> Self {
        let catalog = provider.catalog();

        let direct_availability = catalog
            .compiler_known_declarations()
            .iter()
            .map(|descriptor| {
                (
                    descriptor.id(),
                    rule_is_available(descriptor.availability_rule()),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let mut resolved_availability = BTreeMap::new();
        let mut resolving = BTreeSet::new();

        for descriptor in catalog.compiler_known_declarations() {
            resolve_availability(
                descriptor.id(),
                catalog,
                &direct_availability,
                &mut resolved_availability,
                &mut resolving,
            );
        }

        let declarations = catalog
            .compiler_known_declarations()
            .iter()
            .filter(|descriptor| {
                resolved_availability
                    .get(&descriptor.id())
                    .copied()
                    .unwrap_or(false)
            })
            .map(|descriptor| {
                match provider.untyped_declaration_symbol(descriptor.key()) {
                    Some(symbol) => symbol,
                    // A complete provider is built from this exact validated catalog.
                    None => panic!("compiler-known declaration is missing from its provider"),
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let declaration_ids = declarations.iter().copied().collect();

        let value_ids = catalog
            .compiler_known_values()
            .iter()
            .filter(|value| rule_is_available(value.availability_rule()))
            .map(|value| value.id())
            .collect();

        Self {
            provider,
            declarations,
            declaration_ids,
            value_ids,
        }
    }

    /// Returns the complete immutable provider underlying this filtered view.
    pub fn provider(&self) -> &CompilerKnownSymbolProvider {
        &self.provider
    }

    /// Returns available declaration identities in canonical catalog order.
    pub fn declarations(&self) -> &[AnySymbolId] {
        &self.declarations
    }

    /// Returns whether an exact compiler-known declaration participates in this view.
    pub fn contains(&self, symbol: AnySymbolId) -> bool {
        self.declaration_ids.contains(&symbol)
    }

    pub(super) fn contains_value(&self, value: CompilerKnownValueId) -> bool {
        self.value_ids.contains(&value)
    }

    /// Returns an available declaration by stable key and exact symbol category.
    pub fn declaration_symbol<I: ExactSymbolId>(
        &self,
        key: &CompilerKnownDeclarationKey,
    ) -> Option<I> {
        let symbol = self.provider.untyped_declaration_symbol(key)?;

        if !self.contains(symbol) {
            return None;
        }

        I::try_from_any(symbol)
    }

    /// Returns an available exact symbol carrying a representation role.
    pub fn representation_symbol<I: ExactSymbolId>(&self, role: RepresentationRole) -> Option<I> {
        let RepresentationTarget::Symbol(symbol) = self.representation_target(role)? else {
            return None;
        };

        I::try_from_any(symbol)
    }

    /// Returns an available special value carrying a representation role.
    pub fn representation_value(&self, role: RepresentationRole) -> Option<CompilerKnownValueId> {
        let RepresentationTarget::Value(value) = self.representation_target(role)? else {
            return None;
        };

        Some(value)
    }

    /// Returns available implementation declarations having the requested exact symbol kind.
    pub fn implementation_symbols<'view, I: ExactSymbolId + 'view>(
        &'view self,
        hook: ImplementationHook,
    ) -> impl Iterator<Item = I> + 'view {
        self.implementation_symbol_ids(hook)
            .filter_map(I::try_from_any)
    }

    /// Returns the representation role carried by an available exact symbol.
    pub fn symbol_representation<I: ExactSymbolId>(&self, symbol: I) -> Option<RepresentationRole> {
        if !self.contains(symbol.into()) {
            return None;
        }

        self.provider.role_registry().symbol_representation(symbol)
    }

    /// Returns the representation role carried by an available special value.
    pub fn value_representation(&self, value: CompilerKnownValueId) -> Option<RepresentationRole> {
        if !self.value_ids.contains(&value) {
            return None;
        }

        self.provider.role_registry().value_representation(value)
    }

    /// Returns the implementation hook carried by an available compiler-known symbol.
    pub fn symbol_implementation(
        &self,
        symbol: impl Into<AnySymbolId>,
    ) -> Option<ImplementationHook> {
        let symbol = symbol.into();

        if !self.contains(symbol) {
            return None;
        }

        self.provider.role_registry().symbol_implementation(symbol)
    }

    /// Resolves one expression operation contract when every declaration is target-available.
    pub fn operation_contract(
        &self,
        role: CompilerKnownOperationRole,
    ) -> Option<CompilerKnownOperationContract> {
        let contract = self.provider.role_registry().operation_contract(role)?;

        contract
            .symbols()
            .all(|symbol| self.contains(symbol))
            .then_some(contract)
    }

    pub(super) fn representation_target(
        &self,
        role: RepresentationRole,
    ) -> Option<RepresentationTarget> {
        match self.provider.role_registry().representation_target(role)? {
            RepresentationTarget::Symbol(symbol) if self.contains(symbol) => {
                Some(RepresentationTarget::Symbol(symbol))
            }
            RepresentationTarget::Value(value) if self.contains_value(value) => {
                Some(RepresentationTarget::Value(value))
            }
            RepresentationTarget::Symbol(_) | RepresentationTarget::Value(_) => None,
        }
    }

    pub(super) fn implementation_symbol_ids(
        &self,
        hook: ImplementationHook,
    ) -> impl Iterator<Item = AnySymbolId> + '_ {
        self.provider
            .role_registry()
            .implementation_symbol_ids(hook)
            .iter()
            .copied()
            .filter(|symbol| self.contains(*symbol))
    }
}

impl CompilerKnownSymbolProvider {
    /// Creates an immutable target-filtered view without changing this complete provider.
    pub fn available_symbols(
        self: Arc<Self>,
        rule_is_available: impl FnMut(AvailabilityRule) -> bool,
    ) -> AvailableCompilerKnownSymbols {
        AvailableCompilerKnownSymbols::build(self, rule_is_available)
    }
}

fn resolve_availability(
    declaration: CompilerKnownDeclarationId,
    catalog: &CompilerKnownCatalog,
    direct_availability: &BTreeMap<CompilerKnownDeclarationId, bool>,
    resolved_availability: &mut BTreeMap<CompilerKnownDeclarationId, bool>,
    resolving: &mut BTreeSet<CompilerKnownDeclarationId>,
) -> bool {
    resolve_owned_availability(
        declaration,
        direct_availability,
        resolved_availability,
        resolving,
        |declaration| {
            let descriptor = catalog.compiler_known_declaration(declaration)?;

            match descriptor.owner() {
                CompilerKnownDeclarationOwner::Scope(_) => None,
                CompilerKnownDeclarationOwner::Declaration(owner) => Some(owner),
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use bray_compiler_known::{
        AvailabilityRule, COMPILER_KNOWN_CATALOG, CompilerKnownOperationRole, ImplementationHook,
        RepresentationRole,
    };

    use super::resolve_availability;
    use crate::compiler_known::test_support::{build_provider, declaration_key};
    use crate::{FunctionSymbolId, StructSymbolId};

    #[test]
    fn views_filter_declarations_without_mutating_the_complete_provider() {
        let provider = Arc::new(build_provider());
        let complete_count = provider.declaration_symbols().len();

        let view = Arc::clone(&provider).available_symbols(|rule| {
            matches!(rule, AvailabilityRule::Always | AvailabilityRule::Real16)
        });

        assert_eq!(view.declarations().len(), complete_count - 4);
        assert_eq!(provider.declaration_symbols().len(), complete_count);

        assert!(
            view.declaration_symbol::<StructSymbolId>(&declaration_key("TargetReal16"))
                .is_some()
        );

        assert_eq!(
            view.declaration_symbol::<FunctionSymbolId>(&declaration_key("MemoryCopy")),
            None
        );

        assert_eq!(
            view.declaration_symbol::<StructSymbolId>(&declaration_key("TargetReal128")),
            None
        );

        assert_eq!(
            view.declaration_symbol::<StructSymbolId>(&declaration_key("TargetComplex32")),
            None
        );

        assert_eq!(
            view.declaration_symbol::<StructSymbolId>(&declaration_key("TargetComplex256")),
            None
        );
    }

    #[test]
    fn repeated_views_are_deterministic_and_keep_complete_provider_identity() {
        let provider = Arc::new(build_provider());

        let first =
            Arc::clone(&provider).available_symbols(|rule| rule == AvailabilityRule::Always);

        let second =
            Arc::clone(&provider).available_symbols(|rule| rule == AvailabilityRule::Always);

        assert_eq!(first, second);
        assert!(std::ptr::eq(first.provider(), second.provider()));
    }

    #[test]
    fn typed_roles_follow_target_availability() {
        let provider = Arc::new(build_provider());
        let view = Arc::clone(&provider).available_symbols(|rule| rule == AvailabilityRule::Always);

        let bool_symbol =
            view.representation_symbol::<StructSymbolId>(RepresentationRole::ScalarBool);

        let true_value = view.representation_value(RepresentationRole::BooleanTrue);

        let unavailable_real = provider
            .role_registry()
            .representation_symbol::<StructSymbolId>(RepresentationRole::ScalarR16);

        let unavailable_copy = provider
            .role_registry()
            .implementation_symbols::<FunctionSymbolId>(ImplementationHook::MemoryCopy)
            .next();

        assert!(bool_symbol.is_some());

        assert_eq!(
            view.representation_symbol::<StructSymbolId>(RepresentationRole::ScalarR16),
            None
        );

        assert!(true_value.is_some());

        assert_eq!(
            view.implementation_symbols::<FunctionSymbolId>(ImplementationHook::MemoryCopy)
                .count(),
            0
        );

        assert_eq!(
            bool_symbol.and_then(|symbol| view.symbol_representation(symbol)),
            Some(RepresentationRole::ScalarBool)
        );

        assert_eq!(
            true_value.and_then(|value| view.value_representation(value)),
            Some(RepresentationRole::BooleanTrue)
        );

        assert_eq!(
            unavailable_real.and_then(|symbol| view.symbol_representation(symbol)),
            None
        );

        assert_eq!(
            unavailable_copy.and_then(|symbol| view.symbol_implementation(symbol)),
            None
        );
    }

    #[test]
    fn target_views_publish_only_complete_operation_contracts() {
        let provider = Arc::new(build_provider());
        let view = Arc::clone(&provider).available_symbols(|rule| rule == AvailabilityRule::Always);

        for role in CompilerKnownOperationRole::ALL {
            let Some(contract) = view.operation_contract(*role) else {
                panic!("{role:?} must be available for the portable target");
            };

            assert!(contract.symbols().all(|symbol| view.contains(symbol)));
        }
    }

    #[test]
    fn unavailable_owners_make_nested_declarations_unavailable() {
        let Some(owner) = COMPILER_KNOWN_CATALOG
            .compiler_known_declaration_by_key(&declaration_key("Future"))
            .map(|declaration| declaration.id())
        else {
            panic!("Future must exist in the generated catalog");
        };

        let Some(nested) = COMPILER_KNOWN_CATALOG
            .compiler_known_declaration_by_key(&declaration_key("FutureStart"))
            .map(|declaration| declaration.id())
        else {
            panic!("FutureStart must exist in the generated catalog");
        };

        let direct_availability = BTreeMap::from([(owner, false), (nested, true)]);
        let mut resolved_availability = BTreeMap::new();

        let mut resolving = BTreeSet::new();

        let available = resolve_availability(
            nested,
            &COMPILER_KNOWN_CATALOG,
            &direct_availability,
            &mut resolved_availability,
            &mut resolving,
        );

        assert!(!available);

        assert_eq!(resolved_availability.get(&owner), Some(&false));
        assert_eq!(resolved_availability.get(&nested), Some(&false));
    }
}
