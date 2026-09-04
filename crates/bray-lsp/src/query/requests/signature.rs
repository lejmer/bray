use super::QueryError;
use super::presentation::{range_covers_cursor, source};
use crate::model::Position;
use crate::model::{ParameterInformation, SignatureHelp, SignatureInformation};
use crate::workspace::{DocumentSnapshot, offset_for_position};
use bray_bound_tree::BoundSourceAnchor;
use bray_compilation::{CancellationToken, QueryPriority, SemanticAvailability};
use bray_declarations::SyntaxAnchor;
use bray_source::TextSize;
use bray_symbols::CallableDefinitionId;
use bray_syntax::{
    ArgumentListSyntax, CallOperationSyntax, SyntaxKind, SyntaxWalkControl, SyntaxWalkEvent,
    walk_syntax_node,
};

pub(super) fn signature_help(
    document: &DocumentSnapshot,
    position: Position,
    cancellation: &CancellationToken,
) -> Result<Option<SignatureHelp>, QueryError> {
    let source = source(document)?;
    let offset = offset_for_position(source, position)?;

    let Some((call_syntax, arguments, active_parameter)) = call_syntax_at(document, offset)? else {
        return Ok(None);
    };

    let selection = document.compilation.selected_call_for_syntax(
        BoundSourceAnchor::new(SyntaxAnchor::from_node(&call_syntax), source.version()),
        cancellation,
        QueryPriority::Interactive,
    )?;

    let definition = match selection {
        SemanticAvailability::Available(call) | SemanticAvailability::Recovered(Some(call)) => {
            call.target().declaration()
        }
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => None,
    };

    let definition = match definition {
        Some(definition) => definition,
        None => match callable_definition_before_call(document, &call_syntax, cancellation)? {
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
        .ok_or(QueryError::MissingCallableParameters(
            definition.callable_symbol(),
        ))?;

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
        .ok_or(QueryError::MissingSyntax(document.source_id))?;

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
        SemanticAvailability::Available(anchor) | SemanticAvailability::Recovered(Some(anchor)) => {
            anchor
        }
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
        .ok_or(QueryError::MissingSyntax(document.source_id))?;

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
