use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticDirectiveArgumentProblem, DiagnosticKind,
    DiagnosticNativeLinkDirectiveProblem, DiagnosticNativeLinkKind,
    DiagnosticNativeSymbolDirectiveProblem, DiagnosticRelatedLocation,
    DiagnosticRelatedLocationKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    CallableContractSet, ConstantExpressionExpectedType, ConstantExpressionOccurrence,
    ConstantExpressionOccurrenceKey, DirectiveArgumentName, DirectiveArgumentTemplate,
    DirectiveKind, DirectiveSurface, DirectiveTemplate, FunctionSymbolId, NamedTypeSymbolId,
    NativeLinkKind, NativeLinkRequirement, StructSymbolId, TrustedCapabilitySymbolId,
};

use super::super::Compilation;
use super::diagnostic::{missing_directive, source_diagnostic};
use crate::compilation::directive::directives_of_kind;
use crate::compilation::substitution::named_type;
use crate::fact::{CancellationToken, FactQueryError};

pub(super) fn validate_foreign_import_requirements(
    compilation: &Compilation,
    anchor: bray_declarations::SyntaxAnchor,
    contracts: &CallableContractSet,
    diagnostics: &mut DiagnosticBag,
) {
    let foreign_call = CompilerKnownDeclarationKey::try_new("ForeignCall").and_then(|key| {
        compilation
            .available_compiler_known_symbols()
            .declaration_symbol::<TrustedCapabilitySymbolId>(&key)
    });

    let has_foreign_call = foreign_call.is_some_and(|foreign_call| {
        contract_phases(contracts).any(|phase| {
            phase
                .trusted_capabilities()
                .iter()
                .any(|capability| capability.capability() == foreign_call)
        })
    });

    if !has_foreign_call {
        diagnostics.add(
            source_diagnostic(
                anchor,
                DiagnosticKind::CheckingForeignCallableRequiresCapability,
            )
            .with_arg(DiagnosticArg::referenced_name("foreign_call")),
        );
    }
}

fn contract_phases(
    contracts: &CallableContractSet,
) -> impl Iterator<Item = &bray_symbols::CallablePhaseBehavior> {
    std::iter::once(contracts.invocation_behavior()).chain(contracts.deferred_execution_behavior())
}

pub(super) fn foreign_link_requirements(
    compilation: &Compilation,
    function: FunctionSymbolId,
    declaration_directives: &DirectiveSurface,
    cancellation: &CancellationToken,
    diagnostics: &mut DiagnosticBag,
) -> Result<Vec<NativeLinkRequirement>, FactQueryError> {
    let own_links = directives_of_kind(declaration_directives, DirectiveKind::Link);

    let link_directives = if own_links.is_empty() {
        let module = compilation
            .symbol_graph()?
            .containing_module(function.into())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let module_directives = compilation.declaration_directives(module.id().into())?;
        diagnostics.add_range(module_directives.diagnostics().iter().cloned());

        // Link requirements outlive the module directive fact borrowed in this branch.
        directives_of_kind(module_directives.value(), DirectiveKind::Link)
    } else {
        own_links
    };

    if link_directives.is_empty() {
        let anchor = compilation
            .symbol_graph()?
            .function(function)
            .and_then(|record| record.syntax_anchor())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        diagnostics.add(missing_directive(
            anchor,
            bray_syntax::SyntaxKind::LinkDirective,
        ));

        return Ok(Vec::new());
    }

    let mut links = Vec::new();

    for directive in &link_directives {
        let arguments = match named_arguments(directive, &["name", "kind"]) {
            Ok(arguments) => arguments,
            Err(error) => {
                diagnostics.add(invalid_native_link_directive(
                    error.anchor,
                    DiagnosticNativeLinkDirectiveProblem::Argument(error.problem),
                    error.previous,
                ));

                continue;
            }
        };

        let Some(name_argument) = arguments.get("name") else {
            diagnostics.add(invalid_native_link_directive(
                directive.syntax(),
                DiagnosticNativeLinkDirectiveProblem::Argument(
                    DiagnosticDirectiveArgumentProblem::Missing {
                        name: String::from("name"),
                    },
                ),
                None,
            ));

            continue;
        };

        let name = match directive_string_argument(
            compilation,
            name_argument,
            cancellation,
            diagnostics,
        )? {
            DirectiveStringValue::Value(value) => NonEmptySharedStr::try_new(value),
            DirectiveStringValue::Recovered => continue,
        };

        let kind = match directive_link_kind(compilation, arguments.get("kind").copied())? {
            Ok(kind) => kind,
            Err((anchor, provided)) => {
                diagnostics.add(invalid_native_link_directive(
                    anchor,
                    DiagnosticNativeLinkDirectiveProblem::UnsupportedKind { provided },
                    None,
                ));

                continue;
            }
        };

        match name {
            Some(name) => {
                let requirement =
                    native_link_requirement(compilation, name, kind, directive, diagnostics);

                links.extend(requirement);
            }
            _ => diagnostics.add(invalid_native_link_directive(
                name_argument.expression().syntax(),
                DiagnosticNativeLinkDirectiveProblem::Argument(
                    DiagnosticDirectiveArgumentProblem::EmptyString {
                        name: String::from("name"),
                    },
                ),
                None,
            )),
        }
    }

    Ok(links)
}

pub(super) fn foreign_symbol_name(
    compilation: &Compilation,
    directive: &DirectiveTemplate,
    cancellation: &CancellationToken,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<NonEmptySharedStr>, FactQueryError> {
    let arguments = match named_arguments(directive, &["name"]) {
        Ok(arguments) => arguments,
        Err(error) => {
            diagnostics.add(invalid_native_symbol_directive(
                error.anchor,
                DiagnosticNativeSymbolDirectiveProblem::Argument(error.problem),
                error.previous,
            ));

            return Ok(None);
        }
    };

    let Some(argument) = arguments.get("name") else {
        diagnostics.add(invalid_native_symbol_directive(
            directive.syntax(),
            DiagnosticNativeSymbolDirectiveProblem::Argument(
                DiagnosticDirectiveArgumentProblem::Missing {
                    name: String::from("name"),
                },
            ),
            None,
        ));

        return Ok(None);
    };

    let name = match directive_string_argument(compilation, argument, cancellation, diagnostics)? {
        DirectiveStringValue::Value(value) => NonEmptySharedStr::try_new(value),
        DirectiveStringValue::Recovered => return Ok(None),
    };

    if name.is_none() {
        diagnostics.add(invalid_native_symbol_directive(
            argument.expression().syntax(),
            DiagnosticNativeSymbolDirectiveProblem::Argument(
                DiagnosticDirectiveArgumentProblem::EmptyString {
                    name: String::from("name"),
                },
            ),
            None,
        ));
    }

    Ok(name)
}

fn directive_string_argument(
    compilation: &Compilation,
    argument: &DirectiveArgumentTemplate,
    cancellation: &CancellationToken,
    diagnostics: &mut DiagnosticBag,
) -> Result<DirectiveStringValue, FactQueryError> {
    let string = compilation
        .available_compiler_known_symbols()
        .representation_symbol::<StructSymbolId>(RepresentationRole::String)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let string_type = named_type(
        compilation.semantic_value_store()?,
        NamedTypeSymbolId::Struct(string),
    )?;

    let expression = argument.expression();

    let occurrence = ConstantExpressionOccurrence::new(
        ConstantExpressionOccurrenceKey::new(expression.owner(), expression.syntax()),
        ConstantExpressionExpectedType::Resolved(string_type),
    );

    let value = compilation.embedded_constant_value_with_cancellation(occurrence, cancellation)?;

    diagnostics.add_range(value.diagnostics().iter().cloned());

    if value.diagnostics().has_errors() {
        return Ok(DirectiveStringValue::Recovered);
    }

    let data = compilation
        .semantic_value_store()?
        .constant_value_data(*value.value())
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let bray_symbols::ConstantValueKind::String(value) = data.kind() else {
        return Ok(DirectiveStringValue::Recovered);
    };

    Ok(DirectiveStringValue::Value(Arc::clone(value)))
}

enum DirectiveStringValue {
    Value(Arc<str>),
    Recovered,
}

fn directive_link_kind(
    compilation: &Compilation,
    argument: Option<&DirectiveArgumentTemplate>,
) -> Result<Result<Option<NativeLinkKind>, (bray_declarations::SyntaxAnchor, String)>, FactQueryError> {
    let Some(argument) = argument else {
        return Ok(Ok(None));
    };

    let syntax = argument.expression().syntax();

    let source = compilation
        .source(syntax.source_id())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let text = source
        .text_slice(syntax.full_range())
        .ok_or(FactQueryError::InfrastructureFailure)?
        .trim();

    Ok(NativeLinkKind::for_name(text)
        .map(Some)
        .ok_or_else(|| (syntax, text.to_owned())))
}

fn native_link_requirement(
    compilation: &Compilation,
    name: NonEmptySharedStr,
    kind: Option<NativeLinkKind>,
    directive: &DirectiveTemplate,
    diagnostics: &mut DiagnosticBag,
) -> Option<NativeLinkRequirement> {
    let available = compilation
        .native_link_inputs()
        .iter()
        .filter(|input| input.name() == name.as_str())
        .filter(|input| kind.is_none_or(|kind| input.kind() == kind));

    let mut available = available.cloned();
    let requirement = available.next();

    if requirement.is_none() {
        diagnostics.add(
            source_diagnostic(
                directive.syntax(),
                DiagnosticKind::CheckingUnavailableNativeLinkInput,
            )
            .with_arg(DiagnosticArg::referenced_name(name.as_str())),
        );

        return None;
    }

    let additional = available.count();

    if additional != 0 {
        let matches = u64::try_from(additional.saturating_add(1)).unwrap_or(u64::MAX);

        diagnostics.add(invalid_native_link_directive(
            directive.syntax(),
            DiagnosticNativeLinkDirectiveProblem::AmbiguousInput {
                name: name.as_str().to_owned(),
                kind: kind.map(diagnostic_native_link_kind),
                matches,
            },
            None,
        ));

        return None;
    }

    requirement
}

fn named_arguments<'directive>(
    directive: &'directive DirectiveTemplate,
    accepted: &[&str],
) -> Result<
    BTreeMap<&'directive str, &'directive DirectiveArgumentTemplate>,
    NamedArgumentError,
> {
    let mut arguments = BTreeMap::new();

    for (ordinal, argument) in (0u64..).zip(directive.arguments()) {
        let DirectiveArgumentName::Named(name) = argument.name() else {
            return Err(NamedArgumentError {
                anchor: argument.expression().syntax(),
                problem: DiagnosticDirectiveArgumentProblem::Positional { ordinal },
                previous: None,
            });
        };

        let name = name.as_str();

        if !accepted.contains(&name) {
            return Err(NamedArgumentError {
                anchor: argument.expression().syntax(),
                problem: DiagnosticDirectiveArgumentProblem::Unknown {
                    name: name.to_owned(),
                },
                previous: None,
            });
        }

        if let Some(previous) = arguments.insert(name, argument) {
            return Err(NamedArgumentError {
                anchor: argument.expression().syntax(),
                problem: DiagnosticDirectiveArgumentProblem::Duplicate {
                    name: name.to_owned(),
                },
                previous: Some(previous.expression().syntax()),
            });
        }
    }

    Ok(arguments)
}

struct NamedArgumentError {
    anchor: bray_declarations::SyntaxAnchor,
    problem: DiagnosticDirectiveArgumentProblem,
    previous: Option<bray_declarations::SyntaxAnchor>,
}

fn invalid_native_link_directive(
    anchor: bray_declarations::SyntaxAnchor,
    problem: DiagnosticNativeLinkDirectiveProblem,
    previous: Option<bray_declarations::SyntaxAnchor>,
) -> Diagnostic {
    invalid_native_directive(
        source_diagnostic(anchor, DiagnosticKind::CheckingInvalidNativeLinkDirective)
            .with_arg(DiagnosticArg::native_link_directive_problem(problem)),
        previous,
    )
}

fn invalid_native_symbol_directive(
    anchor: bray_declarations::SyntaxAnchor,
    problem: DiagnosticNativeSymbolDirectiveProblem,
    previous: Option<bray_declarations::SyntaxAnchor>,
) -> Diagnostic {
    invalid_native_directive(
        source_diagnostic(anchor, DiagnosticKind::CheckingInvalidNativeSymbolDirective)
            .with_arg(DiagnosticArg::native_symbol_directive_problem(problem)),
        previous,
    )
}

fn invalid_native_directive(
    diagnostic: Diagnostic,
    previous: Option<bray_declarations::SyntaxAnchor>,
) -> Diagnostic {
    previous.map_or(diagnostic.clone(), |previous| {
        diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::FirstDeclaration,
            SourceSpan::new(previous.source_id(), previous.full_range()),
        ))
    })
}

const fn diagnostic_native_link_kind(kind: NativeLinkKind) -> DiagnosticNativeLinkKind {
    match kind {
        NativeLinkKind::Dynamic => DiagnosticNativeLinkKind::Dynamic,
        NativeLinkKind::Static => DiagnosticNativeLinkKind::Static,
        NativeLinkKind::System => DiagnosticNativeLinkKind::System,
        NativeLinkKind::Framework => DiagnosticNativeLinkKind::Framework,
    }
}
