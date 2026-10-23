use bray_source::SourceSnapshot;
use bray_symbols::{AnySymbolId, MemberLookupResult, ModuleOwnerId};
use bray_syntax::SyntaxToken;

use super::super::category::ResolvedName;
use super::super::diagnostic::NameReference;
use super::core::{NameAccess, PathBindingContext, visible_imported_path_root};
use super::prefix::{PathLookup, imported_path_prefix, next_module_prefix, token_reference};
use crate::{BindingQueryContext, BindingQueryResult, binder::Binder};

impl<C> Binder<'_, C>
where
    C: BindingQueryContext + ?Sized,
{
    pub(crate) fn lookup_module_route(
        &self,
        context: PathBindingContext,
        source: &SourceSnapshot,
        tokens: impl IntoIterator<Item = SyntaxToken>,
    ) -> BindingQueryResult<Option<(PathLookup, usize, NameAccess)>, C::UpstreamError> {
        let references = tokens
            .into_iter()
            .map(|token| token_reference(source, token))
            .collect::<Option<Vec<_>>>();

        let Some(references) = references else {
            return Ok(None);
        };

        let source = next_module_prefix(
            self.binding_context().symbols(),
            context.module_owner(),
            None,
            &references,
            context.access(),
        );

        let local = source
            .or_else(|| {
                next_module_prefix(
                    self.binding_context().symbols(),
                    ModuleOwnerId::from(
                        self.binding_context()
                            .symbols()
                            .compiler_known_environment()
                            .id(),
                    ),
                    None,
                    &references,
                    context.access(),
                )
            })
            .map(|(_, length, lookup)| (lookup, length, context.access()));

        let components = references
            .iter()
            .map(NameReference::text)
            .collect::<Vec<_>>();

        let imported = match context.module() {
            Some(module) => {
                visible_imported_path_root(self.binding_context(), module, &components)?
            }
            None => None,
        }
        .and_then(|root| {
            let access = root.name_access(context.access());

            let (lookup, length) = imported_path_prefix(root, &references, access);

            match &lookup {
                MemberLookupResult::Found(ResolvedName::Surface(AnySymbolId::Module(_)))
                | MemberLookupResult::Inaccessible(_)
                | MemberLookupResult::Malformed(_)
                | MemberLookupResult::Ambiguous(_)
                | MemberLookupResult::WrongKind(_) => Some((lookup, length, access)),
                MemberLookupResult::Found(_) | MemberLookupResult::NotFound => None,
            }
        });

        let selected = match (local, imported) {
            (Some(local), Some(imported)) => {
                let local_priority = (matches!(local.0, MemberLookupResult::Found(_)), local.1);

                let imported_priority = (
                    matches!(imported.0, MemberLookupResult::Found(_)),
                    imported.1,
                );

                Some(if imported_priority > local_priority {
                    imported
                } else {
                    local
                })
            }
            (Some(local), None) => Some(local),
            (None, imported) => imported,
        };

        Ok(selected.map(|(result, length, access)| {
            let reference_index = length
                .saturating_sub(1)
                .min(references.len().saturating_sub(1));

            let reference = references.into_iter().nth(reference_index);

            (PathLookup { result, reference }, length, access)
        }))
    }
}
