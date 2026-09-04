use super::QueryError;
use super::presentation::{
    declaration_label, declaration_name, declaration_symbol_kind, location_for_anchor, lsp_range,
    source,
};
use crate::model::Position;
use crate::model::{DocumentSymbol, Hover, Location, MarkupContent};
use crate::workspace::{DocumentSnapshot, offset_for_position};
use bray_compilation::{CancellationToken, QueryPriority, SemanticAvailability};
use bray_declarations::ContainerId;
use bray_source::{SourceId, SourceSnapshot};
use bray_tooling::format_semantic_type;

pub(super) fn hover(
    document: &DocumentSnapshot,
    position: Position,
    cancellation: &CancellationToken,
) -> Result<Option<Hover>, QueryError> {
    let source = source(document)?;
    let offset = offset_for_position(source, position)?;
    let compilation = document.compilation.as_ref();
    let mut sections = Vec::new();
    let mut range = None;

    match compilation.declaration_at(
        document.source_id,
        offset,
        cancellation,
        QueryPriority::Interactive,
    )? {
        SemanticAvailability::Available(declaration)
        | SemanticAvailability::Recovered(Some(declaration)) => {
            if let Some(record) = compilation.declaration_table().declaration(declaration) {
                sections.push(format!(
                    "```bray\n{}\n```",
                    declaration_label(source, record)
                ));

                range = lsp_range(source, record.full_range());
            }
        }
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => {}
    }

    match compilation.expression_type_at(
        document.source_id,
        offset,
        cancellation,
        QueryPriority::Interactive,
    )? {
        SemanticAvailability::Available(result) | SemanticAvailability::Recovered(Some(result)) => {
            let values = compilation.semantic_value_store()?;
            let symbols = compilation.symbol_graph()?;

            if let Some(ty) = format_semantic_type(values, symbols, result.ty()) {
                sections.push(format!("```bray\n{ty}\n```"));
            }
        }
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => {}
    }

    if sections.is_empty() {
        return Ok(None);
    }

    Ok(Some(Hover {
        contents: MarkupContent {
            kind: "markdown",
            value: sections.join("\n\n"),
        },
        range,
    }))
}

pub(super) fn definition(
    document: &DocumentSnapshot,
    position: Position,
    cancellation: &CancellationToken,
) -> Result<Option<Location>, QueryError> {
    let source = source(document)?;
    let offset = offset_for_position(source, position)?;

    let definition = document.compilation.definition_at(
        document.source_id,
        offset,
        cancellation,
        QueryPriority::Interactive,
    )?;

    let anchor = match definition {
        SemanticAvailability::Available(anchor) | SemanticAvailability::Recovered(Some(anchor)) => {
            anchor
        }
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => {
            return Ok(None);
        }
    };

    location_for_anchor(document.compilation.as_ref(), anchor)
        .ok_or(QueryError::MissingLocation(anchor))
        .map(Some)
}

pub(super) fn references(
    document: &DocumentSnapshot,
    position: Position,
    include_declaration: bool,
    cancellation: &CancellationToken,
) -> Result<Vec<Location>, QueryError> {
    let source = source(document)?;
    let offset = offset_for_position(source, position)?;
    let compilation = document.compilation.as_ref();

    let references = compilation.references_at(
        document.source_id,
        offset,
        cancellation,
        QueryPriority::Interactive,
    )?;

    let references = match references {
        SemanticAvailability::Available(references)
        | SemanticAvailability::Recovered(Some(references)) => references,
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => {
            return Ok(Vec::new());
        }
    };

    let definition = if include_declaration {
        None
    } else {
        match compilation.definition_at(
            document.source_id,
            offset,
            cancellation,
            QueryPriority::Interactive,
        )? {
            SemanticAvailability::Available(anchor)
            | SemanticAvailability::Recovered(Some(anchor)) => Some(anchor),
            SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => None,
        }
    };

    let mut locations = references
        .into_iter()
        .filter(|reference| Some(*reference) != definition)
        .filter_map(|reference| location_for_anchor(compilation, reference))
        .collect::<Vec<_>>();

    locations.sort_by(|left, right| {
        (&left.uri, left.range.start.line, left.range.start.character).cmp(&(
            &right.uri,
            right.range.start.line,
            right.range.start.character,
        ))
    });

    locations.dedup();

    Ok(locations)
}

pub(super) fn document_symbols(
    document: &DocumentSnapshot,
    cancellation: &CancellationToken,
) -> Result<Vec<DocumentSymbol>, QueryError> {
    if cancellation.is_cancelled() {
        return Err(QueryError::Cancelled);
    }

    let source = source(document)?;
    let table = document.compilation.declaration_table();

    Ok(symbols_in_container(
        source,
        table,
        table.root_container(),
        document.source_id,
    ))
}

fn symbols_in_container(
    source: &SourceSnapshot,
    table: &bray_declarations::DeclarationTable,
    container: ContainerId,
    source_id: SourceId,
) -> Vec<DocumentSymbol> {
    let Some(container) = table.container(container) else {
        return Vec::new();
    };

    container
        .declarations()
        .iter()
        .filter_map(|declaration| table.declaration(*declaration))
        .filter(|declaration| declaration.source_id() == source_id)
        .filter_map(|declaration| {
            let name = declaration_name(declaration.name())?;
            let range = lsp_range(source, declaration.full_range())?;

            let children = declaration
                .child_container()
                .map(|child| symbols_in_container(source, table, child, source_id))
                .unwrap_or_default();

            Some(DocumentSymbol {
                name,
                detail: declaration.kind().as_str().to_owned(),
                kind: declaration_symbol_kind(declaration.kind()),
                range,
                selection_range: range,
                children,
            })
        })
        .collect()
}
