use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_compiler_known::{
    AvailabilityRule, CompilerKnownCatalog, CompilerKnownDeclarationId,
    CompilerKnownDeclarationKey, CompilerKnownDeclarationOwner,
};

use super::CompilerKnownSymbolProvider;
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

        Self {
            provider,
            declarations,
            declaration_ids,
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
        AvailabilityRule, COMPILER_KNOWN_CATALOG, CompilerKnownDeclarationId,
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

        assert_eq!(view.declarations().len(), complete_count - 1);
        assert_eq!(provider.declaration_symbols().len(), complete_count);

        assert!(
            view.declaration_symbol::<StructSymbolId>(&declaration_key("TargetReal16"))
                .is_some()
        );

        assert_eq!(
            view.declaration_symbol::<FunctionSymbolId>(&declaration_key("MemoryCopy")),
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
    fn unavailable_owners_make_nested_declarations_unavailable() {
        let owner = CompilerKnownDeclarationId::new(4);
        let nested = CompilerKnownDeclarationId::new(5);

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
