use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundSourceAnchor;
use bray_compilation::{
    CancellationToken, FactQueryError, QueryPriority, SemanticAvailability,
};
use bray_declarations::{
    ContainerId, DeclarationKind, DeclarationName, DeclarationRecord, SyntaxAnchor,
};
use bray_source::{LineIndex, LspPosition, SourceId, SourceSnapshot, TextRange, TextSize};
use bray_symbols::{
    CallableDefinitionId, LocalScopeId, LocalSymbolSnapshot, SymbolKind,
};
use bray_syntax::{
    ArgumentListSyntax, CallOperationSyntax,
    SyntaxKind, SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_node,
};
use bray_tooling::format_semantic_type;

use crate::model::{
    CompletionItem, DocumentSymbol, Hover, Location, MarkupContent, ParameterInformation,
    Position, Range, SignatureHelp, SignatureInformation,
};
use crate::workspace::{DocumentSnapshot, WorkspaceError, offset_for_position};

use super::semantic::{diagnostics, semantic_tokens};

pub(crate) enum Query {
    Completion(Position),
    Definition(Position),
    Diagnostics,
    DocumentSymbols,
    Hover(Position),
    References {
        position: Position,
        include_declaration: bool,
    },
    SemanticTokens {
        multiline_support: bool,
    },
    SignatureHelp(Position),
}

pub(crate) fn execute(
    document: &DocumentSnapshot,
    query: Query,
    cancellation: &CancellationToken,
) -> Result<serde_json::Value, QueryError> {
    let result = match query {
        Query::Completion(position) => {
            serde_json::to_value(completion(document, position, cancellation)?)
        }
        Query::Definition(position) => {
            serde_json::to_value(definition(document, position, cancellation)?)
        }
        Query::Diagnostics => {
            serde_json::to_value(diagnostics(document, cancellation)?)
        }
        Query::DocumentSymbols => serde_json::to_value(document_symbols(document, cancellation)?),
        Query::Hover(position) => {
            serde_json::to_value(hover(document, position, cancellation)?)
        }
        Query::References {
            position,
            include_declaration,
        } => serde_json::to_value(references(
            document,
            position,
            include_declaration,
            cancellation,
        )?),
        Query::SemanticTokens { multiline_support } => {
            serde_json::to_value(semantic_tokens(
                document,
                cancellation,
                multiline_support,
            )?)
        }
        Query::SignatureHelp(position) => {
            serde_json::to_value(signature_help(document, position, cancellation)?)
        }
    };

    result.map_err(|_| QueryError::Serialization)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QueryError {
    Cancelled,
    Compiler,
    InvalidPosition,
    Serialization,
}

impl From<FactQueryError> for QueryError {
    fn from(error: FactQueryError) -> Self {
        match error {
            FactQueryError::Cancelled => Self::Cancelled,
            _ => Self::Compiler,
        }
    }
}

impl From<WorkspaceError> for QueryError {
    fn from(_: WorkspaceError) -> Self {
        Self::InvalidPosition
    }
}

fn hover(
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
                sections.push(format!("```bray\n{}\n```", declaration_label(source, record)));
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
        SemanticAvailability::Available(result)
        | SemanticAvailability::Recovered(Some(result)) => {
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

fn definition(
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
        SemanticAvailability::Available(anchor)
        | SemanticAvailability::Recovered(Some(anchor)) => anchor,
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => {
            return Ok(None);
        }
    };

    location_for_anchor(document.compilation.as_ref(), anchor)
        .ok_or(QueryError::Compiler)
        .map(Some)
}

fn references(
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
        (&left.uri, left.range.start.line, left.range.start.character)
            .cmp(&(&right.uri, right.range.start.line, right.range.start.character))
    });

    locations.dedup();

    Ok(locations)
}

fn completion(
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
    )? else {
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

            items.entry(label).or_insert((
                6,
                Some(SymbolKind::LocalBinding.as_str().to_owned()),
            ));
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

            items.entry(label).or_insert((
                21,
                Some(SymbolKind::LocalConstant.as_str().to_owned()),
            ));
        }
    }

    for parameter in locals.anonymous_parameters() {
        if !visible_scopes.contains(&parameter.scope()) {
            continue;
        }

        let label = parameter.name().as_str().to_owned();

        items.entry(label).or_insert((
            5,
            Some(
                SymbolKind::AnonymousCallableParameter
                    .as_str()
                    .to_owned(),
            ),
        ));
    }

    Ok(completion_items(items))
}

fn completion_items(
    items: BTreeMap<String, (u32, Option<String>)>,
) -> Vec<CompletionItem> {
    items
        .into_iter()
        .map(|(label, (kind, detail))| CompletionItem {
            label,
            kind,
            detail,
        })
        .collect()
}

fn visible_local_scopes(
    locals: &LocalSymbolSnapshot,
    offset: TextSize,
) -> BTreeSet<LocalScopeId> {
    let innermost = locals
        .scopes()
        .iter()
        .filter(|scope| {
            let range = scope.syntax_anchor().full_range();

            range.start() <= offset
                && offset <= range.end()
                && scope.visibility_start() <= offset
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

fn signature_help(
    document: &DocumentSnapshot,
    position: Position,
    cancellation: &CancellationToken,
) -> Result<Option<SignatureHelp>, QueryError> {
    let source = source(document)?;
    let offset = offset_for_position(source, position)?;

    let Some((call_syntax, arguments, active_parameter)) =
        call_syntax_at(document, offset)?
    else {
        return Ok(None);
    };

    let selection = document.compilation.selected_call_for_syntax(
        BoundSourceAnchor::new(
            SyntaxAnchor::from_node(&call_syntax),
            source.version(),
        ),
        cancellation,
        QueryPriority::Interactive,
    )?;

    let definition = match selection {
        SemanticAvailability::Available(call)
        | SemanticAvailability::Recovered(Some(call)) => call.target().declaration(),
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => None,
    };

    let definition = match definition {
        Some(definition) => definition,
        None => match callable_definition_before_call(
            document,
            &call_syntax,
            cancellation,
        )? {
            Some(definition) => definition,
            None => return Ok(None),
        },
    };

    let graph = document.compilation.symbol_graph()?;

    if graph
        .callable_parameters_and_receiver(definition.callable_symbol())
        .is_none()
    {
        return Ok(None);
    }

    let (parameter_ids, _) = graph
        .callable_parameters_and_receiver(definition.callable_symbol())
        .ok_or(QueryError::Compiler)?;

    let parameters = parameter_ids
        .iter()
        .map(|parameter| ParameterInformation {
            label: graph
                .member_name((*parameter).into())
                .map(|name| name.as_str().to_owned())
                .unwrap_or_else(|| String::from("_")),
        })
        .collect::<Vec<_>>();

    let callable_name = graph
        .member_name(definition.symbol())
        .map(|name| name.as_str())
        .unwrap_or("<callable>");

    let parameter_text = parameters
        .iter()
        .map(|parameter| parameter.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let parameter_count = u32::try_from(parameter_ids.len()).unwrap_or(u32::MAX);
    let supplied_count = u32::try_from(arguments.arguments().count()).unwrap_or(u32::MAX);
    let last_parameter = parameter_count.max(supplied_count).saturating_sub(1);

    Ok(Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label: format!("{callable_name}({parameter_text})"),
            parameters,
        }],
        active_signature: 0,
        active_parameter: active_parameter.min(last_parameter),
    }))
}

fn callable_definition_before_call(
    document: &DocumentSnapshot,
    call: &CallOperationSyntax,
    cancellation: &CancellationToken,
) -> Result<Option<CallableDefinitionId>, QueryError> {
    let syntax = document
        .compilation
        .source_unit_syntax(document.source_id)
        .ok_or(QueryError::Compiler)?;

    let callable = syntax
        .source_unit()
        .tokens()
        .filter(|token| {
            !token.is_missing()
                && token.kind() == SyntaxKind::IdentifierToken
                && token.range().end() <= call.full_range().start()
        })
        .map(|token| token.range().start())
        .last();

    let Some(callable) = callable else {
        return Ok(None);
    };

    let definition = document.compilation.definition_at(
        document.source_id,
        callable,
        cancellation,
        QueryPriority::Interactive,
    )?;

    let anchor = match definition {
        SemanticAvailability::Available(anchor)
        | SemanticAvailability::Recovered(Some(anchor)) => anchor,
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => {
            return Ok(None);
        }
    };

    let Some(declaration) = document
        .compilation
        .declaration_table()
        .declarations()
        .iter()
        .find(|declaration| declaration.syntax_anchor() == anchor.syntax())
    else {
        return Ok(None);
    };

    let graph = document.compilation.symbol_graph()?;

    let definition = graph
        .symbol_for_declaration(declaration.id())
        .and_then(CallableDefinitionId::try_new);

    Ok(definition)
}

fn call_syntax_at(
    document: &DocumentSnapshot,
    offset: TextSize,
) -> Result<Option<(CallOperationSyntax, ArgumentListSyntax, u32)>, QueryError> {
    let syntax = document
        .compilation
        .source_unit_syntax(document.source_id)
        .ok_or(QueryError::Compiler)?;

    let mut selected = None;

    walk_syntax_node(syntax.source_unit(), |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        if !range_covers_cursor(node.full_range(), offset) {
            return SyntaxWalkControl::SkipChildren;
        }

        let Some(call) = node.cast::<CallOperationSyntax>() else {
            return SyntaxWalkControl::Continue;
        };

        let arguments = call.argument_list();

        if !range_covers_cursor(arguments.full_range(), offset) {
            return SyntaxWalkControl::Continue;
        }

        let active = arguments
            .arguments()
            .enumerate()
            .find(|(_, argument)| range_covers_cursor(argument.full_range(), offset))
            .map(|(index, _)| u32::try_from(index).unwrap_or(u32::MAX))
            .unwrap_or_else(|| {
                u32::try_from(
                    arguments
                        .arguments()
                        .filter(|argument| argument.full_range().end() <= offset)
                        .count(),
                )
                .unwrap_or(u32::MAX)
            });

        selected = Some((call, arguments, active));

        SyntaxWalkControl::Continue
    });

    Ok(selected)
}

fn document_symbols(
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

fn location_for_anchor(
    compilation: &bray_compilation::Compilation,
    anchor: BoundSourceAnchor,
) -> Option<Location> {
    let source = source_for_anchor(compilation, anchor).ok()?;
    let uri = source.origin().document_uri().ok().flatten()?;
    let range = lsp_range(source, anchor.syntax().full_range())?;

    Some(Location { uri, range })
}

fn source_for_anchor(
    compilation: &bray_compilation::Compilation,
    anchor: BoundSourceAnchor,
) -> Result<&SourceSnapshot, QueryError> {
    compilation
        .source(anchor.syntax().source_id())
        .filter(|source| source.version() == anchor.source_version())
        .ok_or(QueryError::Compiler)
}

pub(super) fn source(document: &DocumentSnapshot) -> Result<&SourceSnapshot, QueryError> {
    document
        .compilation
        .source(document.source_id)
        .ok_or(QueryError::Compiler)
}

pub(super) fn lsp_range(source: &SourceSnapshot, range: TextRange) -> Option<Range> {
    let index = LineIndex::new(source.text()).ok()?;
    let start = index.lsp_position(range.start())?;
    let end = index.lsp_position(range.end())?;

    Some(Range {
        start: position(start),
        end: position(end),
    })
}

const fn position(position: LspPosition) -> Position {
    Position {
        line: position.line(),
        character: position.character(),
    }
}

fn declaration_label(source: &SourceSnapshot, declaration: &DeclarationRecord) -> String {
    let text = source
        .text_slice(declaration.full_range())
        .unwrap_or_default()
        .trim();

    text.split(['{', ';'])
        .next()
        .unwrap_or(text)
        .trim()
        .to_owned()
}

fn declaration_name(name: Option<&DeclarationName>) -> Option<String> {
    match name? {
        DeclarationName::Identifier(name) => Some(name.clone()),
        DeclarationName::Keyword(kind) => Some(
            kind.as_str()
                .strip_suffix("_keyword")
                .unwrap_or(kind.as_str())
                .to_owned(),
        ),
        DeclarationName::Path(path) => Some(path.dotted()),
        DeclarationName::Implementation(name) => {
            let subject = name.subject().dotted();

            Some(match name.trait_path() {
                Some(trait_path) => format!("{subject}: {}", trait_path.dotted()),
                None => subject,
            })
        }
    }
}

const fn symbol_completion_kind(kind: SymbolKind) -> u32 {
    match kind {
        SymbolKind::Module => 9,
        SymbolKind::Struct | SymbolKind::Union | SymbolKind::Trait => 7,
        SymbolKind::Function
        | SymbolKind::Predicate
        | SymbolKind::CallableContract
        | SymbolKind::CallableOverload
        | SymbolKind::AnonymousCallable => 3,
        SymbolKind::CallableParameter
        | SymbolKind::PredicateParameter
        | SymbolKind::ReceiverParameter
        | SymbolKind::AnonymousCallableParameter
        | SymbolKind::GenericTypeParameter
        | SymbolKind::GenericConstParameter => 5,
        SymbolKind::Constant
        | SymbolKind::TraitConstantMember
        | SymbolKind::TraitConstantFulfillment
        | SymbolKind::LocalConstant
        | SymbolKind::StructField
        | SymbolKind::UnionPayloadField => 21,
        _ => 6,
    }
}

fn range_covers_cursor(range: TextRange, offset: TextSize) -> bool {
    range.contains(offset) || range.end() == offset
}

const fn declaration_symbol_kind(kind: DeclarationKind) -> u32 {
    match kind {
        DeclarationKind::Module => 2,
        DeclarationKind::Struct => 23,
        DeclarationKind::Union => 10,
        DeclarationKind::Trait => 11,
        DeclarationKind::Function
        | DeclarationKind::Predicate
        | DeclarationKind::CallableContract => 12,
        DeclarationKind::TypeCallableMember
        | DeclarationKind::TraitCallableMember
        | DeclarationKind::TypeConstructorMember
        | DeclarationKind::FinalizerMember
        | DeclarationKind::DestructorMember
        | DeclarationKind::ScopeEnterMember
        | DeclarationKind::ScopeExitMember => 6,
        DeclarationKind::StructField | DeclarationKind::UnionPayloadField => 8,
        DeclarationKind::Constant | DeclarationKind::TraitConstantMember => 14,
        _ => 13,
    }
}

#[cfg(test)]
mod tests {
    use bray_compilation::{
        Compilation, CompilationOptions, CompilationRequest, WorkerBudget,
    };
    use bray_source::{SourceId, SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};

    use super::{
        QueryError, completion, definition, diagnostics, document_symbols, hover, references,
        semantic_tokens, signature_help,
    };
    use crate::model::Position;
    use crate::workspace::{CompilationRevision, DocumentSnapshot};

    #[test]
    fn hover_reports_source_and_type_information() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let hover = hover(&document, Position { line: 9, character: 23 }, &cancellation)
            .unwrap_or_else(|error| panic!("hover should complete: {error:?}"));

        assert!(
            hover
                .as_ref()
                .is_some_and(|hover| !hover.contents.value.is_empty()),
            "hover should contain semantic information: {hover:?}"
        );
    }

    #[test]
    fn definition_finds_the_referenced_declaration() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let definition = definition(
            &document,
            Position { line: 9, character: 23 },
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("definition should complete: {error:?}"));

        assert!(
            definition.as_ref().is_some_and(|location| {
                location.uri == "file:///test.bray"
                    && location.range.start != location.range.end
            }),
            "definition should identify source syntax: {definition:?}"
        );
    }

    #[test]
    fn references_find_declarations_and_uses() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let references = references(
            &document,
            Position { line: 9, character: 23 },
            true,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("references should complete: {error:?}"));

        assert_eq!(references.len(), 2, "unexpected references: {references:?}");

        assert!(references.iter().all(|location| {
            location.uri == "file:///test.bray"
                && location.range.start != location.range.end
        }));

        assert_ne!(references[0].range, references[1].range);
    }

    #[test]
    fn completion_includes_surface_and_visible_local_names() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let completion = completion(
            &document,
            Position { line: 10, character: 4 },
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("completion should complete: {error:?}"));

        assert!(completion.iter().any(|item| item.label == "add"));
        assert!(completion.iter().any(|item| item.label == "value"));
        assert!(!completion.iter().any(|item| item.label == "left"));
        assert!(!completion.iter().any(|item| item.label == "right"));
    }

    #[test]
    fn signature_help_reports_callable_parameters() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let signature = signature_help(
            &document,
            Position { line: 9, character: 29 },
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("signature help should complete: {error:?}"));

        assert!(
            signature.as_ref().is_some_and(|signature| {
                signature.signatures.len() == 1
                    && signature.signatures[0].parameters.len() == 2
                    && signature.active_parameter == 1
            }),
            "unexpected signature help: {signature:?}"
        );
    }

    #[test]
    fn document_symbols_preserve_declaration_hierarchy() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let symbols = document_symbols(&document, &cancellation)
            .unwrap_or_else(|error| panic!("document symbols should complete: {error:?}"));

        assert!(contains_symbol(&symbols, "add"));
    }

    #[test]
    fn semantic_tokens_cover_keywords_and_comments() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let tokens = semantic_tokens(&document, &cancellation, false)
            .unwrap_or_else(|error| panic!("semantic tokens should complete: {error:?}"));

        assert!(!tokens.data.is_empty());
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 7));
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 8));
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 11));
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 13));
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 15));
    }

    #[test]
    fn multiline_tokens_are_split_when_the_client_does_not_support_them() {
        let document = document(concat!(
            "module app;\n",
            "/* first\n",
            "   second */\n",
        ));

        let cancellation = bray_compilation::CancellationToken::new();

        let tokens = semantic_tokens(&document, &cancellation, false)
            .unwrap_or_else(|error| panic!("semantic tokens should complete: {error:?}"));

        let comment_lines = absolute_tokens(&tokens.data)
            .into_iter()
            .filter(|token| token.3 == 15)
            .map(|token| token.0)
            .collect::<Vec<_>>();

        assert_eq!(comment_lines, [1, 2]);
    }

    #[test]
    fn cancelled_queries_stop_before_publishing_results() {
        let document = document("module app;\n");
        let cancellation = bray_compilation::CancellationToken::new();

        cancellation.cancel();

        assert_eq!(
            semantic_tokens(&document, &cancellation, false),
            Err(QueryError::Cancelled)
        );
    }

    #[test]
    fn diagnostics_render_localized_messages() {
        let document = document("module app\n");
        let cancellation = bray_compilation::CancellationToken::new();

        let diagnostics = diagnostics(&document, &cancellation)
            .unwrap_or_else(|error| panic!("diagnostics should complete: {error:?}"));

        assert!(
            diagnostics.items.iter().any(|diagnostic| {
                diagnostic.code == 3003
                    && diagnostic.message
                        == "unexpected end of file while expecting semicolon token"
            }),
            "missing localized syntax diagnostic: {:?}",
            diagnostics.items
        );
    }

    fn feature_document() -> DocumentSnapshot {
        document(concat!(
            "module app;\n",
            "\n",
            "func add(left: i32, right: i32) -> i32\n",
            "{\n",
            "    return left + right;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let value: i32 = add(1, 2);\n",
            "    value;\n",
            "}\n",
            "// trailing comment\n",
        ))
    }

    fn document(text: &str) -> DocumentSnapshot {
        let Some(package) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity should be valid");
        };

        let request = CompilationRequest::with_options(
            package.clone(),
            vec![SourceInput::lsp_open_document(
                SourceIdentity::new(1),
                "file:///test.bray",
                SourceVersion::new(1),
                text,
            )],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                bray_compilation::SelectedTarget::baseline(),
            ),
        );

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation should load: {error:?}"));

        let product = ProductIdentity::try_new(package, "test")
            .unwrap_or_else(|| panic!("test product identity should be valid"));

        let target = bray_compilation::SelectedTarget::baseline()
            .profile()
            .identity()
            .clone();

        DocumentSnapshot {
            compilation: std::sync::Arc::new(compilation),
            source_id: SourceId::new(0),
            uri: String::from("file:///test.bray"),
            revision: 1,
            compilation_revision: CompilationRevision {
                product,
                target,
                generation: 1,
            },
        }
    }

    fn contains_symbol(symbols: &[crate::model::DocumentSymbol], name: &str) -> bool {
        symbols
            .iter()
            .any(|symbol| symbol.name == name || contains_symbol(&symbol.children, name))
    }

    fn absolute_tokens(data: &[u32]) -> Vec<(u32, u32, u32, u32)> {
        let mut line = 0_u32;
        let mut character = 0_u32;
        let mut tokens = Vec::new();

        for token in data.chunks_exact(5) {
            line = line.saturating_add(token[0]);

            character = if token[0] == 0 {
                character.saturating_add(token[1])
            } else {
                token[1]
            };

            tokens.push((line, character, token[2], token[3]));
        }

        tokens
    }
}
