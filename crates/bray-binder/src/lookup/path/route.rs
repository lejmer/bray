use bray_source::SourceSnapshot;
use bray_symbols::{AnySymbolId, MemberLookupResult, ModuleOwnerId, ModuleSymbolId};
use bray_syntax::SyntaxToken;

use super::super::category::ResolvedName;
use super::super::diagnostic::NameReference;
use super::core::{PathBindingContext, visible_imported_path_root};
use super::prefix::{imported_path_prefix, next_module_prefix, token_reference};
use crate::{BinderFactContext, BinderFactResult, binder::Binder};

impl<C> Binder<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    pub(crate) fn lookup_module_route(
        &self,
        context: PathBindingContext,
        source: &SourceSnapshot,
        tokens: impl IntoIterator<Item = SyntaxToken>,
    ) -> BinderFactResult<Option<(ModuleSymbolId, usize)>> {
        let references = tokens
            .into_iter()
            .map(|token| token_reference(source, token))
            .collect::<Option<Vec<_>>>();

        let Some(references) = references else {
            return Ok(None);
        };

        let source = next_module_prefix(
            self.facts().symbols(),
            context.module_owner(),
            None,
            &references,
            context.access(),
        );

        let local = source
            .or_else(|| {
                next_module_prefix(
                    self.facts().symbols(),
                    ModuleOwnerId::from(self.facts().symbols().compiler_known_environment().id()),
                    None,
                    &references,
                    context.access(),
                )
            })
            .and_then(|(module, length, lookup)| {
                matches!(lookup, MemberLookupResult::Found(_)).then_some((module, length))
            });

        let components = references
            .iter()
            .map(NameReference::text)
            .collect::<Vec<_>>();

        let imported = match context.module() {
            Some(module) => visible_imported_path_root(self.facts(), module, &components)?,
            None => None,
        }
        .map(|root| imported_path_prefix(root, &references, context.access()))
        .and_then(|(lookup, length)| match lookup {
            MemberLookupResult::Found(ResolvedName::Surface(AnySymbolId::Module(module))) => {
                Some((module, length))
            }
            MemberLookupResult::Found(ResolvedName::Local(_))
            | MemberLookupResult::Found(ResolvedName::Surface(_))
            | MemberLookupResult::NotFound
            | MemberLookupResult::WrongKind(_)
            | MemberLookupResult::Ambiguous(_)
            | MemberLookupResult::Inaccessible(_)
            | MemberLookupResult::Malformed(_) => None,
        });

        Ok(match (local, imported) {
            (Some(local), Some(imported)) if imported.1 > local.1 => Some(imported),
            (Some(local), _) => Some(local),
            (None, imported) => imported,
        })
    }
}
