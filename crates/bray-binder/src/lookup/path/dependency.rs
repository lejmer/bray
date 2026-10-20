use bray_declarations::DeclarationTable;
use bray_symbols::{AnySymbolId, MemberLookupResult, ModulePathKey, ModuleSymbolId, SymbolGraph};

use super::super::category::ResolvedName;

/// Matches the recorded source path without loading checked module surfaces or reparsing syntax.
pub(crate) fn module_using_access(
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    module: ModuleSymbolId,
    components: &[&str],
) -> Option<(usize, bool)> {
    let module = symbols
        .module(module)
        .expect("dependency scope has a module identity");

    let mut matched = None;

    for part in module.module_parts() {
        for declaration in declarations
            .using_declarations(*part)
            .expect("source module part belongs to the declaration table")
        {
            let declaration = declarations
                .declaration(*declaration)
                .expect("using index references an existing declaration");

            let Some(path) = declaration.name().and_then(|name| name.as_path()) else {
                continue;
            };

            let path = path.segments();

            if !path.is_empty()
                && path.len() <= components.len()
                && path
                    .iter()
                    .map(String::as_str)
                    .eq(components[..path.len()].iter().copied())
            {
                let candidate = (
                    path.len(),
                    declaration.surface().acknowledges_internal_use(),
                );

                if matched.is_none_or(|current: (usize, bool)| {
                    (candidate.1, candidate.0) > (current.1, current.0)
                }) {
                    matched = Some(candidate);
                }
            }
        }
    }

    matched
}

/// Dependency availability is a property of the namespace spelled at this reference.
pub(crate) fn lookup_dependency_declared(
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    module: ModuleSymbolId,
    components: &[&str],
    lookup: &MemberLookupResult<ResolvedName>,
) -> bool {
    let namespace = match lookup {
        MemberLookupResult::Found(ResolvedName::Surface(namespace)) => Some(*namespace),
        MemberLookupResult::Inaccessible(candidates) => match candidates.as_ref() {
            [ResolvedName::Surface(namespace)] => Some(*namespace),
            _ => None,
        },
        _ => None,
    };

    let Some(namespace @ (AnySymbolId::Module(_) | AnySymbolId::Package(_))) = namespace else {
        return true;
    };

    if module_using_access(symbols, declarations, module, components).is_some() {
        return true;
    }

    let AnySymbolId::Module(namespace) = namespace else {
        return false;
    };

    let Some(namespace) = symbols.module(namespace) else {
        return false;
    };

    if matches!(
        namespace.owner(),
        bray_symbols::ModuleOwnerId::CompilerKnownEnvironment(_)
    ) && namespace.path().segments().next() == Some("target")
    {
        return true;
    }

    let current = symbols
        .module(module)
        .expect("dependency scope has a module identity");

    let current_length = current.path().segments().len();

    if namespace.owner() != current.owner()
        || !current
            .path()
            .segments()
            .eq(components.iter().take(current_length).copied())
    {
        return false;
    }

    // The current module does not implicitly make its declared descendant modules available.
    !(current_length + 1..=components.len()).any(|length| {
        let path = ModulePathKey::try_new(components[..length].iter().copied())
            .expect("a source path contains nonempty components");

        symbols.module_by_path(current.owner(), &path).is_some()
    })
}

#[cfg(test)]
mod tests {
    use bray_symbols::{MemberLookupResult, ModuleOwnerId, ModulePathKey};

    use super::lookup_dependency_declared;
    use crate::BindingQueryContext;
    use crate::lookup::ResolvedName;
    use crate::query::test_support::TestFixture;

    #[test]
    fn compiler_known_contracts_use_the_same_logical_module_exception() {
        let fixture = TestFixture::new();
        let context = fixture.context();
        let symbols = context.symbols();
        let owner = ModuleOwnerId::from(symbols.compiler_known_environment().id());
        let path = ModulePathKey::try_new(["core", "memory"]).unwrap();
        let memory = symbols.module_by_path(owner, &path).unwrap();
        let lookup = MemberLookupResult::Found(ResolvedName::Surface(memory.id().into()));

        assert!(lookup_dependency_declared(
            symbols,
            context.declarations(),
            memory.id(),
            &["core", "memory", "owned_allocation"],
            &lookup,
        ));

        let source = symbols
            .modules()
            .iter()
            .find(|module| module.origin() == bray_symbols::SymbolOrigin::Source)
            .unwrap();

        assert!(!lookup_dependency_declared(
            symbols,
            context.declarations(),
            source.id(),
            &["core", "memory", "owned_allocation"],
            &lookup,
        ));
    }
}
