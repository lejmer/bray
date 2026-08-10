use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_diagnostics::{DiagnosticArg, DiagnosticBag, DiagnosticKind};
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
        let Some(arguments) = named_arguments(directive, &["name", "kind"]) else {
            diagnostics.add(source_diagnostic(
                directive.syntax(),
                DiagnosticKind::CheckingInvalidNativeLinkDirective,
            ));

            continue;
        };

        let Some(name_argument) = arguments.get("name") else {
            diagnostics.add(source_diagnostic(
                directive.syntax(),
                DiagnosticKind::CheckingInvalidNativeLinkDirective,
            ));

            continue;
        };

        let name =
            directive_string_argument(compilation, name_argument, cancellation, diagnostics)?
                .and_then(NonEmptySharedStr::try_new);

        let Some(kind) = directive_link_kind(compilation, arguments.get("kind").copied())? else {
            diagnostics.add(source_diagnostic(
                directive.syntax(),
                DiagnosticKind::CheckingInvalidNativeLinkDirective,
            ));

            continue;
        };

        match name {
            Some(name) => {
                let requirement =
                    native_link_requirement(compilation, name, kind, directive, diagnostics);

                links.extend(requirement);
            }
            _ => diagnostics.add(source_diagnostic(
                directive.syntax(),
                DiagnosticKind::CheckingInvalidNativeLinkDirective,
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
    let Some(arguments) = named_arguments(directive, &["name"]) else {
        diagnostics.add(source_diagnostic(
            directive.syntax(),
            DiagnosticKind::CheckingInvalidNativeSymbolDirective,
        ));

        return Ok(None);
    };

    let Some(argument) = arguments.get("name") else {
        diagnostics.add(source_diagnostic(
            directive.syntax(),
            DiagnosticKind::CheckingInvalidNativeSymbolDirective,
        ));

        return Ok(None);
    };

    let name = directive_string_argument(compilation, argument, cancellation, diagnostics)?
        .and_then(NonEmptySharedStr::try_new);

    if name.is_none() {
        diagnostics.add(source_diagnostic(
            directive.syntax(),
            DiagnosticKind::CheckingInvalidNativeSymbolDirective,
        ));
    }

    Ok(name)
}

fn directive_string_argument(
    compilation: &Compilation,
    argument: &DirectiveArgumentTemplate,
    cancellation: &CancellationToken,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<Arc<str>>, FactQueryError> {
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
        return Ok(None);
    }

    let data = compilation
        .semantic_value_store()?
        .constant_value_data(*value.value())
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let bray_symbols::ConstantValueKind::String(value) = data.kind() else {
        return Ok(None);
    };

    Ok(Some(Arc::clone(value)))
}

fn directive_link_kind(
    compilation: &Compilation,
    argument: Option<&DirectiveArgumentTemplate>,
) -> Result<Option<Option<NativeLinkKind>>, FactQueryError> {
    let Some(argument) = argument else {
        return Ok(Some(None));
    };

    let syntax = argument.expression().syntax();

    let source = compilation
        .source(syntax.source_id())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let text = source
        .text_slice(syntax.full_range())
        .ok_or(FactQueryError::InfrastructureFailure)?
        .trim();

    Ok(NativeLinkKind::for_name(text).map(Some))
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

    if available.next().is_some() {
        diagnostics.add(source_diagnostic(
            directive.syntax(),
            DiagnosticKind::CheckingInvalidNativeLinkDirective,
        ));

        return None;
    }

    requirement
}

fn named_arguments<'directive>(
    directive: &'directive DirectiveTemplate,
    accepted: &[&str],
) -> Option<BTreeMap<&'directive str, &'directive DirectiveArgumentTemplate>> {
    let mut arguments = BTreeMap::new();

    for argument in directive.arguments() {
        let DirectiveArgumentName::Named(name) = argument.name() else {
            return None;
        };

        let name = name.as_str();

        if !accepted.contains(&name) || arguments.insert(name, argument).is_some() {
            return None;
        }
    }

    Some(arguments)
}
