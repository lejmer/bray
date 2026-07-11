use bray_declarations::{
    DeclarationChunkResult, DeclarationTable, discover_source_unit_declarations,
    merge_declaration_chunks,
};
use bray_testing::{test_source_at, test_source_store};

pub(crate) fn declaration_table(source_texts: &[&str]) -> DeclarationTable {
    let sources = test_source_store(source_texts);
    let chunks = (0u32..)
        .take(source_texts.len())
        .map(|index| declaration_chunk(&sources, index))
        .collect::<Vec<_>>();

    let result = merge_declaration_chunks(chunks.iter());
    let (table, _diagnostics) = result.into_parts();

    table
}

pub(crate) fn declaration_chunk(
    sources: &bray_source::SourceStore,
    index: u32,
) -> DeclarationChunkResult {
    let parsed = bray_parser::parse_source_unit(test_source_at(sources, index));

    discover_source_unit_declarations(parsed.source_unit())
}
