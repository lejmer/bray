use bray_declarations::{
    DeclarationChunkResult, DeclarationTable, discover_source_unit_declarations,
    merge_declaration_chunks,
};
use bray_syntax::SyntaxTree;
use bray_testing::{test_source_at, test_source_store};

pub(crate) fn declaration_table(source_texts: &[&str]) -> DeclarationTable {
    declarations_and_syntax(source_texts).0
}

pub(crate) fn declarations_and_syntax(source_texts: &[&str]) -> (DeclarationTable, SyntaxTree) {
    let sources = test_source_store(source_texts);
    let parsed = (0u32..)
        .take(source_texts.len())
        .map(|index| bray_parser::parse_source_unit(test_source_at(&sources, index)))
        .collect::<Vec<_>>();
    let chunks = parsed
        .iter()
        .map(|result| discover_source_unit_declarations(result.source_unit()))
        .collect::<Vec<_>>();

    let result = merge_declaration_chunks(chunks.iter());
    let (table, _diagnostics) = result.into_parts();
    let syntax =
        SyntaxTree::compilation_unit(parsed.iter().map(|result| result.source_unit().clone()));

    (table, syntax)
}

pub(crate) fn declaration_chunk(
    sources: &bray_source::SourceStore,
    index: u32,
) -> DeclarationChunkResult {
    let parsed = bray_parser::parse_source_unit(test_source_at(sources, index));

    discover_source_unit_declarations(parsed.source_unit())
}
