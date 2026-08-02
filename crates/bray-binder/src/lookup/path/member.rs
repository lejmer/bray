use bray_diagnostics::DiagnosticNameKind;
use bray_source::SourceSnapshot;
use bray_symbols::{AnySymbolId, MemberLookupResult, ModuleOwnerId};
use bray_syntax::SyntaxToken;

use super::super::binding::{NameLookupResult, combine_name_lookups, lookup_surface_name};
use super::super::category::{ResolvedMemberName, classify_member};
use super::super::diagnostic::{malformed_lookup, report_lookup_result};
use super::core::NameAccess;
use super::prefix::{
    compiler_known_module_for_owner, lookup_surface_name_with_imports, next_imported_module_prefix,
    source_module_prefix, token_reference,
};
use crate::{BinderFactContext, BinderFactResult, binder::Binder};

impl<C> Binder<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    pub(crate) fn bind_member(
        &mut self,
        owner: AnySymbolId,
        source: &SourceSnapshot,
        token: SyntaxToken,
        access: NameAccess,
    ) -> BinderFactResult<NameLookupResult<ResolvedMemberName>> {
        let Some(reference) = token_reference(source, token) else {
            return Ok(malformed_lookup());
        };

        let symbols = self.facts().symbols();
        let imported_symbols = self.facts().imported_symbols()?;

        let mut ordinary = lookup_surface_name_with_imports(
            symbols,
            imported_symbols,
            owner,
            reference.text(),
            access,
        );

        if let Some(compiler_known) =
            compiler_known_module_for_owner(symbols, imported_symbols, owner)
        {
            ordinary = combine_name_lookups(
                ordinary,
                lookup_surface_name(symbols, compiler_known.into(), reference.text(), access),
            );
        }

        let module_prefix = match owner {
            AnySymbolId::Module(module) => symbols.module(module).map_or_else(
                || {
                    imported_symbols
                        .and_then(|symbols| symbols.module(module))
                        .and_then(|module| match module.owner() {
                            ModuleOwnerId::Package(package) => next_imported_module_prefix(
                                imported_symbols?,
                                package,
                                Some(module.path()),
                                std::slice::from_ref(&reference),
                                access,
                            ),
                            ModuleOwnerId::CompilerKnownEnvironment(_) => None,
                        })
                        .map_or(MemberLookupResult::NotFound, |(_, _, lookup)| lookup)
                },
                |module| {
                    let local = source_module_prefix(
                        symbols,
                        module.owner(),
                        Some(module.path()),
                        &reference,
                        access,
                    );

                    if !matches!(local, MemberLookupResult::NotFound) {
                        return local;
                    }

                    source_module_prefix(
                        symbols,
                        ModuleOwnerId::from(symbols.compiler_known_environment().id()),
                        Some(module.path()),
                        &reference,
                        access,
                    )
                },
            ),
            _ => MemberLookupResult::NotFound,
        };

        let result = combine_name_lookups(ordinary, module_prefix).classify(classify_member);

        report_lookup_result(self, &reference, DiagnosticNameKind::Member, &result);

        Ok(result)
    }
}
