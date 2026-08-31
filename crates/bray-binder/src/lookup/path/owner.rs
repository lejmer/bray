use bray_diagnostics::{DiagnosticBag, DiagnosticNameKind, DiagnosticResult};
use bray_symbols::{AnySymbolId, MemberLookupResult};
use bray_syntax::PathSyntax;

use super::super::binding::{NameLookupResult, lookup_surface_name};
use super::super::category::ResolvedName;
use super::super::diagnostic::{lookup_diagnostic, malformed_lookup};
use super::core::{NameAccess, bind_module_path_lookup, bind_remaining_path, surface_lookup};
use super::prefix::{PathLookup, path_references};
use crate::{BindingQueryContext, BindingQueryResult};

fn bind_owner_path_lookup<C>(
    binding_context: &C,
    owner: AnySymbolId,
    path: &PathSyntax,
    access: NameAccess,
) -> BindingQueryResult<PathLookup, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    let Some(references) = path_references(path) else {
        return Ok(PathLookup {
            result: malformed_lookup(),
            reference: None,
        });
    };

    let Some(first) = references.first() else {
        return Ok(PathLookup {
            result: malformed_lookup(),
            reference: None,
        });
    };

    let ordinary = lookup_surface_name(binding_context.symbols(), owner, first.text(), access);

    let lookup = bind_remaining_path(
        binding_context.symbols(),
        None,
        access,
        references,
        ordinary,
        1,
        &mut |module, name, access| binding_context.module_re_export_lookup(module, name, access),
    )?;

    if !matches!(lookup.result, MemberLookupResult::NotFound) {
        return Ok(lookup);
    }

    let Some(module) = binding_context.symbols().containing_module(owner) else {
        return Ok(lookup);
    };

    bind_module_path_lookup(
        binding_context,
        module.id(),
        path,
        access,
        &mut |module, name, access| binding_context.module_re_export_lookup(module, name, access),
    )
}

pub(crate) fn bind_owner_path<C>(
    binding_context: &C,
    owner: AnySymbolId,
    path: &PathSyntax,
    access: NameAccess,
) -> BindingQueryResult<NameLookupResult<ResolvedName>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    Ok(bind_owner_path_lookup(binding_context, owner, path, access)?.result)
}

/// Resolves one declaration path from its owner first, then its containing module.
pub fn bind_owner_surface_path<C>(
    binding_context: &C,
    owner: AnySymbolId,
    path: &PathSyntax,
    access: NameAccess,
) -> BindingQueryResult<DiagnosticResult<MemberLookupResult<AnySymbolId>>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    let lookup = bind_owner_path_lookup(binding_context, owner, path, access)?;
    let result = surface_lookup(lookup.result);

    let diagnostics = lookup
        .reference
        .map_or_else(DiagnosticBag::new, |reference| {
            lookup_diagnostic(&reference, DiagnosticNameKind::Symbol, &result)
                .map_or_else(DiagnosticBag::new, DiagnosticBag::single)
        });

    Ok(DiagnosticResult::new(result, diagnostics))
}
