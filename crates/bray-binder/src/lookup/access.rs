use bray_bound_tree::{AnyBoundNodeId, BoundUnitView};
use bray_diagnostics::{DiagnosticBag, DiagnosticNameKind};
use bray_symbols::{AnySymbolId, ImportedSymbolSkeleton, ModuleSymbol, NameAccess, SymbolGraph};

use super::diagnostic::{NameReference, lookup_diagnostic};
use crate::{BindingQueryContext, BindingQueryResult};

/// Returns whether a known declaration and its owning scopes permit this name reference.
///
/// Source and imported identities share the same declaration, owner and module-path caps.
pub(crate) fn can_access_symbol(
    source: Option<&SymbolGraph>,
    imported: Option<&bray_symbols::ImportedSymbolSkeleton>,
    symbol: AnySymbolId,
    access: NameAccess,
) -> bool {
    let mut current = Some(symbol);

    while let Some(symbol) = current {
        let source_origin = source.and_then(|source| source.symbol_origin(symbol));

        let origin = source_origin
            .or_else(|| {
                imported
                    .and_then(|imported| imported.symbol_key(symbol))
                    .map(|_| bray_symbols::SymbolOrigin::Imported)
            })
            .unwrap_or_else(|| panic!("name access references unknown symbol {symbol:?}"));

        if matches!(access, NameAccess::Internal)
            || matches!(access, NameAccess::Local) && origin != bray_symbols::SymbolOrigin::Imported
        {
            return true;
        }

        let visibility = source
            .and_then(|source| source.symbol_visibility(symbol))
            .or_else(|| imported.and_then(|imported| imported.symbol_visibility(symbol)));

        if visibility.is_some_and(|visibility| !access.allows(visibility, origin)) {
            return false;
        }

        if let AnySymbolId::Module(module) = symbol {
            let module = source
                .and_then(|source| source.module(module))
                .or_else(|| imported.and_then(|imported| imported.module(module)))
                .expect("module access references a module record");

            for length in 1..module.path().segments().len() {
                let path =
                    bray_symbols::ModulePathKey::try_new(module.path().segments().take(length))
                        .expect("module prefix contains nonempty path segments");

                let ancestor = source
                    .and_then(|source| source.module_by_path(module.owner(), &path))
                    .or_else(|| {
                        let bray_symbols::ModuleOwnerId::Package(package) = module.owner() else {
                            return None;
                        };

                        imported.and_then(|imported| imported.module_by_path(package, &path))
                    });

                if ancestor.is_some_and(|ancestor| {
                    !access.allows(ancestor.visibility(), ancestor.origin())
                }) {
                    return false;
                }
            }
        }

        current = source
            .and_then(|source| source.containing_symbol(symbol))
            .or_else(|| imported.and_then(|imported| imported.containing_symbol(symbol)));
    }

    true
}

/// Checks a typed member reference in its original lexical scope.
///
/// Imported definitions retain their own binding scope; callers receive only the access explicitly
/// recorded on their reference or acknowledged by a checked using declaration in their module.
pub fn check_member_access<C>(
    context: &C,
    unit: BoundUnitView<'_>,
    reference: AnyBoundNodeId,
    owner: AnySymbolId,
    member: AnySymbolId,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<bool, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    let (access, origin) = match reference {
        AnyBoundNodeId::Expression(expression) => (
            unit.name_access(expression)
                .expect("member reference belongs to its bound unit"),
            unit.expression(expression)
                .expect("member expression exists")
                .origin(),
        ),
        AnyBoundNodeId::Pattern(pattern) => (
            NameAccess::Local,
            unit.pattern(pattern)
                .expect("member pattern exists")
                .origin(),
        ),
        _ => panic!("member access requires an expression or pattern reference: {reference:?}"),
    };

    let source = context.symbols();

    let imported =
        if source.symbol_origin(owner).is_some() && source.symbol_origin(member).is_some() {
            None
        } else {
            context.imported_symbols()?
        };

    if can_access_symbol(Some(source), imported, owner, access)
        && can_access_symbol(Some(source), imported, member, access)
    {
        return Ok(true);
    }

    if access == NameAccess::Local {
        let declaration = source
            .symbol_for_key(unit.key().declared_owner())
            .expect("bound unit has a source declaration");

        if let Some(module) = source.containing_module(declaration) {
            let surface = context.module_surface(module.id())?;

            if surface.value().usings().iter().any(|using| {
                using.permits_internal_access()
                    && !matches!(using.target(), AnySymbolId::Package(_))
                    && (scope_contains(source, imported, using.target(), member)
                        || scope_contains(source, imported, using.target(), owner))
            }) {
                return Ok(true);
            }
        }
    }

    let name = source
        .member_name(member)
        .or_else(|| imported.and_then(|imported| imported.member_name(member)))
        .or_else(|| source.member_name(owner))
        .or_else(|| imported.and_then(|imported| imported.member_name(owner)))
        .expect("inaccessible member or its owner has a declaration name");

    let anchor = origin.source_anchor().syntax();
    let reference = NameReference::new(name.as_str(), anchor.source_id(), anchor.full_range());
    let lookup = bray_symbols::MemberLookupResult::<AnySymbolId>::Inaccessible(Box::new([member]));

    diagnostics.add(
        lookup_diagnostic(&reference, DiagnosticNameKind::Value, &lookup)
            .expect("inaccessible lookup produces a diagnostic"),
    );

    Ok(false)
}

fn scope_contains(
    source: &SymbolGraph,
    imported: Option<&ImportedSymbolSkeleton>,
    scope: AnySymbolId,
    member: AnySymbolId,
) -> bool {
    let scope_module = match scope {
        AnySymbolId::Module(module) => source
            .module(module)
            .or_else(|| imported.and_then(|imported| imported.module(module))),
        _ => None,
    };

    let mut current = Some(member);

    while let Some(symbol) = current {
        if symbol == scope {
            return true;
        }

        if let (Some(scope), AnySymbolId::Module(module)) = (scope_module, symbol) {
            let module = source
                .module(module)
                .or_else(|| imported.and_then(|imported| imported.module(module)))
                .expect("member has a module record");

            if module_contains(scope, module) {
                return true;
            }
        }

        current = source
            .containing_symbol(symbol)
            .or_else(|| imported.and_then(|imported| imported.containing_symbol(symbol)));
    }

    false
}

fn module_contains(scope: &ModuleSymbol, module: &ModuleSymbol) -> bool {
    scope.owner() == module.owner()
        && scope.path().segments().len() <= module.path().segments().len()
        && scope
            .path()
            .segments()
            .eq(module.path().segments().take(scope.path().segments().len()))
}
