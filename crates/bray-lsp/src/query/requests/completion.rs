use super::QueryError;
use super::presentation::{source, symbol_completion_kind};
use crate::model::CompletionItem;
use crate::model::Position;
use crate::workspace::{DocumentSnapshot, offset_for_position};
use bray_compilation::{CancellationToken, QueryPriority};
use bray_source::TextSize;
use bray_symbols::{LocalScopeId, LocalSymbolSnapshot, SymbolKind};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn completion(
    document: &DocumentSnapshot,
    position: Position,
    cancellation: &CancellationToken,
) -> Result<Vec<CompletionItem>, QueryError> {
    let source = source(document)?;
    let offset = offset_for_position(source, position)?;
    let compilation = document.compilation.as_ref();
    let mut items = BTreeMap::<String, (u32, Option<String>)>::new();

    for candidate in compilation.completion_candidates_at(
        document.source_id,
        offset,
        cancellation,
        QueryPriority::Interactive,
    )? {
        items.entry(candidate.name().to_owned()).or_insert((
            symbol_completion_kind(candidate.kind()),
            Some(candidate.kind().as_str().to_owned()),
        ));
    }

    let Some(unit) = compilation.bound_unit_at(
        document.source_id,
        offset,
        cancellation,
        QueryPriority::Interactive,
    )?
    else {
        return Ok(completion_items(items));
    };

    let locals = unit.value().local_symbols();
    let visible_scopes = visible_local_scopes(locals, offset);

    for binding in locals.bindings() {
        if visible_scopes.contains(&binding.scope())
            && binding
                .key()
                .anchors()
                .first()
                .is_some_and(|anchor| anchor.full_range().start() <= offset)
        {
            let label = binding.name().as_str().to_owned();

            items
                .entry(label)
                .or_insert((6, Some(SymbolKind::LocalBinding.as_str().to_owned())));
        }
    }

    for constant in locals.constants() {
        if visible_scopes.contains(&constant.scope())
            && constant
                .key()
                .anchors()
                .first()
                .is_some_and(|anchor| anchor.full_range().start() <= offset)
        {
            let label = constant.name().as_str().to_owned();

            items
                .entry(label)
                .or_insert((21, Some(SymbolKind::LocalConstant.as_str().to_owned())));
        }
    }

    for parameter in locals.anonymous_parameters() {
        if !visible_scopes.contains(&parameter.scope()) {
            continue;
        }

        let label = parameter.name().as_str().to_owned();

        items.entry(label).or_insert((
            5,
            Some(SymbolKind::AnonymousCallableParameter.as_str().to_owned()),
        ));
    }

    Ok(completion_items(items))
}

fn completion_items(items: BTreeMap<String, (u32, Option<String>)>) -> Vec<CompletionItem> {
    items
        .into_iter()
        .map(|(label, (kind, detail))| CompletionItem {
            label,
            kind,
            detail,
        })
        .collect()
}

fn visible_local_scopes(locals: &LocalSymbolSnapshot, offset: TextSize) -> BTreeSet<LocalScopeId> {
    let innermost = locals
        .scopes()
        .iter()
        .filter(|scope| {
            let range = scope.syntax_anchor().full_range();

            range.start() <= offset && offset <= range.end() && scope.visibility_start() <= offset
        })
        .min_by_key(|scope| scope.syntax_anchor().full_range().len())
        .map(|scope| scope.id());

    let mut visible = BTreeSet::new();
    let mut current = innermost;

    while let Some(scope_id) = current {
        let Some(scope) = locals.scope(scope_id) else {
            break;
        };

        visible.insert(scope_id);
        current = scope.parent();
    }

    visible
}
