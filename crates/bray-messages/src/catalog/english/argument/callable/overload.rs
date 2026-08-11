use super::super::interface::{
    format_english_interface_symbol_identity, format_english_interface_symbol_kind,
};
use super::super::source::format_english_type;
use bray_diagnostics::{
    DiagnosticCallableOverloadArm, DiagnosticCallableOverloadContext,
    DiagnosticCallableOverloadProblem, DiagnosticImplementationBorrowKind,
    DiagnosticImplementationFamily, DiagnosticImplementationFamilySubject,
    DiagnosticImplementationOverloadProblem,
};

pub(crate) fn format_english_implementation_overload_problem(
    problem: &DiagnosticImplementationOverloadProblem,
) -> String {
    match problem {
        DiagnosticImplementationOverloadProblem::HeaderSubjectKind { subject, actual } => format!(
            "{} resolves to a {}, which cannot be an overload-family subject",
            format_english_interface_symbol_identity(subject),
            format_english_interface_symbol_kind(*actual),
        ),
        DiagnosticImplementationOverloadProblem::HeaderTraitKind {
            trait_definition,
            actual,
        } => format!(
            "{} resolves to a {} instead of a trait",
            format_english_interface_symbol_identity(trait_definition),
            format_english_interface_symbol_kind(*actual),
        ),
        DiagnosticImplementationOverloadProblem::ArmSymbolKind {
            implementation,
            actual,
        } => format!(
            "{} resolves to a {} instead of a named trait implementation",
            format_english_interface_symbol_identity(implementation),
            format_english_interface_symbol_kind(*actual),
        ),
        DiagnosticImplementationOverloadProblem::DuplicateArm { implementation } => format!(
            "{} appears more than once in this overload family",
            format_english_interface_symbol_identity(implementation),
        ),
        DiagnosticImplementationOverloadProblem::ArmSubjectNotFamilyCompatible {
            implementation,
        } => format!(
            "{} does not implement a named type or a borrow of a named type",
            format_english_interface_symbol_identity(implementation),
        ),
        DiagnosticImplementationOverloadProblem::FamilyMismatch {
            implementation,
            required,
            provided,
        } => format!(
            "{} belongs to {} instead of {}",
            format_english_interface_symbol_identity(implementation),
            format_english_implementation_family(provided),
            format_english_implementation_family(required),
        ),
    }
}

fn format_english_implementation_family(family: &DiagnosticImplementationFamily) -> String {
    let subject = match family.subject() {
        DiagnosticImplementationFamilySubject::Named(subject) => {
            format_english_interface_symbol_identity(subject)
        }
        DiagnosticImplementationFamilySubject::Borrowed { kind, subject } => {
            let borrow = match kind {
                DiagnosticImplementationBorrowKind::Shared => "shared borrow of",
                DiagnosticImplementationBorrowKind::Mutable => "mutable borrow of",
            };

            format!(
                "{borrow} {}",
                format_english_interface_symbol_identity(subject)
            )
        }
    };

    format!(
        "the {subject} family for {}",
        format_english_interface_symbol_identity(family.trait_definition())
    )
}

pub(crate) fn format_english_callable_overload_problem(
    problem: &DiagnosticCallableOverloadProblem,
) -> String {
    match problem {
        DiagnosticCallableOverloadProblem::ArmSymbolKind { symbol, actual } => format!(
            "{} resolves to a {} instead of a callable declaration",
            format_english_interface_symbol_identity(symbol),
            format_english_interface_symbol_kind(*actual),
        ),
        DiagnosticCallableOverloadProblem::ContextMismatch {
            arm,
            required,
            provided,
        } => format!(
            "{} belongs to {} instead of {}",
            format_english_callable_overload_arm(arm),
            format_english_callable_overload_context(provided),
            format_english_callable_overload_context(required),
        ),
        DiagnosticCallableOverloadProblem::DuplicateArm { arm } => format!(
            "{} appears more than once in this overload family",
            format_english_callable_overload_arm(arm),
        ),
        DiagnosticCallableOverloadProblem::ConflictingFamilies { arm, first, second } => format!(
            "{} belongs to both {} and {}",
            format_english_callable_overload_arm(arm),
            format_english_interface_symbol_identity(first),
            format_english_interface_symbol_identity(second),
        ),
        DiagnosticCallableOverloadProblem::ConflictingSignatures { arm, conflicting } => format!(
            "{} has the same call surface as {}",
            format_english_callable_overload_arm(arm),
            format_english_callable_overload_arm(conflicting),
        ),
    }
}

fn format_english_callable_overload_arm(arm: &DiagnosticCallableOverloadArm) -> String {
    let parameters = arm
        .parameter_types()
        .iter()
        .map(format_english_type)
        .collect::<Vec<_>>()
        .join(", ");

    let receiver = if arm.has_receiver() { "receiver, " } else { "" };

    format!(
        "{} with signature ({receiver}{parameters}) -> {}",
        format_english_interface_symbol_identity(arm.identity()),
        format_english_type(arm.result_type()),
    )
}

fn format_english_callable_overload_context(context: &DiagnosticCallableOverloadContext) -> String {
    let (category, identity) = match context {
        DiagnosticCallableOverloadContext::Module(identity) => ("module", identity),
        DiagnosticCallableOverloadContext::NamedType(identity) => ("type", identity),
        DiagnosticCallableOverloadContext::Trait(identity) => ("trait", identity),
        DiagnosticCallableOverloadContext::Implementation(identity) => ("implementation", identity),
    };

    format!(
        "{category} {}",
        format_english_interface_symbol_identity(identity)
    )
}
