use super::super::callable::format_english_receiver;
use super::super::interface::{
    format_english_interface_symbol_identity, format_english_interface_symbol_kind,
    interface_symbol_identity_text,
};
use super::super::source::format_english_type;
use bray_diagnostics::DiagnosticInterfaceSymbolIdentity;

pub(crate) fn format_english_selection_candidates(
    candidates: &bray_diagnostics::DiagnosticSelectionCandidates,
) -> String {
    let mut rendered = candidates
        .candidates()
        .iter()
        .map(format_english_selection_candidate)
        .collect::<Vec<_>>();

    if candidates.omitted_count() != 0 {
        rendered.push(format!(
            "{} additional candidate{}",
            candidates.omitted_count(),
            if candidates.omitted_count() == 1 {
                ""
            } else {
                "s"
            }
        ));
    }

    match rendered.as_slice() {
        [] => "no candidates".to_owned(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        [leading @ .., final_candidate] => {
            format!("{}, and {final_candidate}", leading.join(", "))
        }
    }
}

pub(crate) fn format_english_selection_rejections(
    rejections: &bray_diagnostics::DiagnosticSelectionRejections,
) -> String {
    let mut rendered = rejections
        .rejections()
        .iter()
        .map(|rejection| {
            format!(
                "{}: {}",
                format_english_selection_candidate(rejection.candidate()),
                format_english_selection_rejection_reason(rejection.reason())
            )
        })
        .collect::<Vec<_>>();

    if rejections.omitted_count() != 0 {
        rendered.push(format!(
            "{} additional rejected candidate{}",
            rejections.omitted_count(),
            if rejections.omitted_count() == 1 {
                ""
            } else {
                "s"
            }
        ));
    }

    match rendered.as_slice() {
        [] => "no rejected candidates".to_owned(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        [leading @ .., final_rejection] => {
            format!("{}, and {final_rejection}", leading.join(", "))
        }
    }
}

fn format_english_selection_rejection_reason(
    reason: &bray_diagnostics::DiagnosticSelectionRejectionReason,
) -> String {
    use bray_diagnostics::DiagnosticSelectionRejectionReason as Reason;

    match reason {
        Reason::GenericArgumentCount { provided, maximum } => {
            format!("the call supplies {provided} generic arguments but accepts at most {maximum}")
        }
        Reason::ReceiverPresence { provided, required } => match (*provided, *required) {
            (true, false) => "the call supplies a receiver but this candidate has none".to_owned(),
            (false, true) => "the call has no receiver but this candidate requires one".to_owned(),
            _ => "the call and candidate disagree about receiver presence".to_owned(),
        },
        Reason::ReceiverType { provided, required } => format!(
            "the receiver has type {} but this candidate requires {}",
            format_english_type(provided),
            format_english_type(required)
        ),
        Reason::ReceiverCapability { provided, required } => format!(
            "the receiver is {} but this candidate requires a {} receiver",
            format_english_receiver_capability(*provided),
            format_english_receiver(Some(*required))
        ),
        Reason::CallableArgument(reason) => format_english_callable_argument_rejection(reason),
        Reason::OperandTypes { provided } => format!(
            "the supplied operand types are ({})",
            provided
                .iter()
                .map(format_english_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Reason::ConstructionInput(reason) => format_english_construction_input_rejection(reason),
        Reason::ExpressionForm => "the operation does not match this expression form".to_owned(),
        Reason::RequiredImplementation => {
            "a required implementation is unavailable for the supplied types".to_owned()
        }
        Reason::RequiredLanguageOperation => {
            "a required language operation is unavailable for the supplied types".to_owned()
        }
    }
}

const fn format_english_receiver_capability(
    capability: bray_diagnostics::DiagnosticReceiverCapability,
) -> &'static str {
    use bray_diagnostics::DiagnosticReceiverCapability;

    match capability {
        DiagnosticReceiverCapability::Shared => "shared",
        DiagnosticReceiverCapability::Mutable => "mutable",
        DiagnosticReceiverCapability::Owned => "owned",
        DiagnosticReceiverCapability::OwnedMutable => "owned and mutable",
    }
}

fn format_english_callable_argument_rejection(
    reason: &bray_diagnostics::DiagnosticCallableArgumentRejection,
) -> String {
    use bray_diagnostics::DiagnosticCallableArgumentRejection as Reason;

    match reason {
        Reason::PositionalAfterNamed { ordinal } => format!(
            "argument {} is positional after a named argument",
            ordinal.saturating_add(1)
        ),
        Reason::UnknownName { provided, accepted } => format!(
            "argument name {provided} is not accepted. Candidate parameter names are {}",
            accepted.join(", ")
        ),
        Reason::PositionalUnavailable { ordinal } => format!(
            "argument {} has no positional parameter",
            ordinal.saturating_add(1)
        ),
        Reason::Duplicate { name, ordinal } => format!(
            "argument {} supplies {} more than once",
            ordinal.saturating_add(1),
            name.as_deref().unwrap_or("the same parameter")
        ),
        Reason::Type {
            name,
            ordinal,
            expected,
            actual,
        } => format!(
            "argument {}{} has type {} but requires {}",
            ordinal.saturating_add(1),
            name.as_ref()
                .map_or_else(String::new, |name| format!(" ({name})")),
            format_english_type(actual),
            format_english_type(expected)
        ),
        Reason::Missing {
            name,
            ordinal,
            expected,
        } => format!(
            "required parameter {name} at position {} with type {} is missing",
            ordinal.saturating_add(1),
            format_english_type(expected)
        ),
    }
}

fn format_english_construction_input_rejection(
    reason: &bray_diagnostics::DiagnosticConstructionInputRejection,
) -> String {
    use bray_diagnostics::DiagnosticConstructionInputRejection as Reason;

    match reason {
        Reason::PositionalAfterNamed => "a positional input follows a named input".to_owned(),
        Reason::UnknownName { provided, accepted } => format!(
            "input name {provided} is not accepted. Candidate input names are {}",
            accepted.join(", ")
        ),
        Reason::PositionalUnavailable { ordinal } => format!(
            "input {} has no positional construction input",
            ordinal.saturating_add(1)
        ),
        Reason::Duplicate { name, ordinal } => format!(
            "input {} supplies {} more than once",
            ordinal.saturating_add(1),
            name.as_deref().unwrap_or("the same construction input")
        ),
        Reason::Type {
            name,
            ordinal,
            expected,
            actual,
        } => format!(
            "input {}{} has type {} but requires {}",
            ordinal.saturating_add(1),
            name.as_ref()
                .map_or_else(String::new, |name| format!(" ({name})")),
            format_english_type(actual),
            format_english_type(expected)
        ),
        Reason::Missing {
            name,
            ordinal,
            expected,
        } => format!(
            "required input {name} at position {} with type {} is missing",
            ordinal.saturating_add(1),
            format_english_type(expected)
        ),
    }
}

fn format_english_selection_candidate(
    candidate: &bray_diagnostics::DiagnosticSelectionCandidate,
) -> String {
    use bray_diagnostics::DiagnosticSelectionCandidateIdentity as Identity;
    use bray_diagnostics::DiagnosticSelectionCandidateSignature as Signature;

    let identity = match candidate.identity() {
        Identity::BuiltIn => "built-in operation".to_owned(),
        Identity::Declaration(identity) => format_selection_declaration_identity(identity),
        Identity::NamedDeclaration { identity, name } => format!(
            "{name} ({})",
            format_selection_declaration_identity(identity)
        ),
        Identity::Iteration { iterable, iterator } => format!(
            "iteration implementations {} and {}",
            format_selection_declaration_identity(iterable),
            format_selection_declaration_identity(iterator)
        ),
        Identity::ExpressionValue => "callable expression value".to_owned(),
        Identity::PatternValue => "callable pattern value".to_owned(),
        Identity::LocalValue => "local callable value".to_owned(),
        Identity::SurfaceValue(identity) => format!(
            "callable value {}",
            format_selection_declaration_identity(identity)
        ),
        Identity::NamedSurfaceValue { identity, name } => format!(
            "callable value {name} ({})",
            format_selection_declaration_identity(identity)
        ),
    };

    let signature = match candidate.signature() {
        Signature::Callable {
            parameter_types,
            result_type,
        } => {
            let parameters = parameter_types
                .iter()
                .map(format_english_type)
                .collect::<Vec<_>>()
                .join(", ");

            format!("({parameters}) -> {}", format_english_type(result_type))
        }
        Signature::Operation {
            operand_types,
            result_type,
        } => {
            let operands = operand_types
                .iter()
                .map(format_english_type)
                .collect::<Vec<_>>()
                .join(", ");

            result_type.as_ref().map_or_else(
                || format!("({operands})"),
                |result| format!("({operands}) -> {}", format_english_type(result)),
            )
        }
        Signature::Iteration {
            source_type,
            cursor_type,
            element_type,
        } => format!(
            "{} through {} produces {}",
            format_english_type(source_type),
            format_english_type(cursor_type),
            format_english_type(element_type)
        ),
    };

    format!("{identity} with signature {signature}")
}

fn format_selection_declaration_identity(identity: &DiagnosticInterfaceSymbolIdentity) -> String {
    use bray_diagnostics::DiagnosticInterfaceSymbolIdentity as Identity;

    match identity {
        Identity::SourceDeclaration { owner, kind, .. } => format!(
            "{} in {}",
            format_english_interface_symbol_kind(*kind),
            interface_symbol_identity_text(owner)
        ),
        _ => format_english_interface_symbol_identity(identity),
    }
}
