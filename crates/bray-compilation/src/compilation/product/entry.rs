use bray_binder::SymbolFactProvider;
use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_symbols::{
    AnySymbolId, AvailableCompilerKnownSymbols, CallableConstness, CallableExecution,
    CallableSignatureFact, CallableSignatureTemplate, CallableTrust, FunctionSymbolId,
    GenericArgument, GenericArgumentTemplate, NamedTypeSymbolId, SemanticValueStore,
    SymbolFactRequest, SymbolGraph, TypeData, TypeExpressionTemplate,
};

use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::compilation::diagnostics::source_diagnostic;
use crate::fact::FactQueryError;

#[derive(Clone, Copy)]
pub(super) enum ProductEntryKind {
    Executable,
    Test,
}

#[derive(Clone, Copy, Default)]
pub(super) struct EntrypointSelection {
    function: Option<FunctionSymbolId>,
    is_async: bool,
    is_recovered: bool,
}

impl EntrypointSelection {
    pub(super) const fn function(self) -> Option<FunctionSymbolId> {
        self.function
    }

    pub(super) const fn is_async(self) -> bool {
        self.is_async
    }

    pub(super) const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

pub(super) fn select_executable_entrypoint(
    binder: &CompilationBinderFacts<'_>,
    semantic_values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
    symbols: &SymbolGraph,
    functions: &[FunctionSymbolId],
    explicit: &[(FunctionSymbolId, SyntaxAnchor)],
    diagnostics: &mut DiagnosticBag,
) -> Result<EntrypointSelection, FactQueryError> {
    if explicit.len() > 1 {
        for (_, anchor) in explicit.iter().skip(1) {
            diagnostics.add(source_diagnostic(
                *anchor,
                DiagnosticKind::CheckingDuplicateEntrypoint,
            ));
        }

        return Ok(EntrypointSelection {
            is_recovered: true,
            ..EntrypointSelection::default()
        });
    }

    if let Some((function, _)) = explicit.first().copied() {
        return validated_selection(
            binder,
            semantic_values,
            available,
            symbols,
            function,
            diagnostics,
        );
    }

    let main_functions = functions
        .iter()
        .copied()
        .filter(|function| {
            matches!(
                symbols.containing_symbol((*function).into()),
                Some(AnySymbolId::Module(_))
            ) && symbols
                .member_name((*function).into())
                .is_some_and(|name| name.as_str() == "main")
        })
        .collect::<Vec<_>>();

    if main_functions.is_empty() {
        diagnostics.add(missing_entrypoint_diagnostic(symbols));

        return Ok(EntrypointSelection {
            is_recovered: true,
            ..EntrypointSelection::default()
        });
    }

    let mut valid = Vec::new();

    for function in main_functions {
        let validation = validate_entry(
            binder,
            semantic_values,
            available,
            symbols,
            function,
            ProductEntryKind::Executable,
            diagnostics,
        )?;

        if let Some(is_async) = validation {
            valid.push((function, is_async));
        }
    }

    match valid.as_slice() {
        [(function, is_async)] => Ok(EntrypointSelection {
            function: Some(*function),
            is_async: *is_async,
            is_recovered: false,
        }),
        [] => {
            diagnostics.add(missing_entrypoint_diagnostic(symbols));

            Ok(EntrypointSelection {
                is_recovered: true,
                ..EntrypointSelection::default()
            })
        }
        [_, rest @ ..] => {
            for (function, _) in rest {
                if let Some(anchor) = symbols.declaration_syntax_anchor((*function).into()) {
                    diagnostics.add(source_diagnostic(
                        anchor,
                        DiagnosticKind::CheckingDuplicateEntrypoint,
                    ));
                }
            }

            Ok(EntrypointSelection {
                is_recovered: true,
                ..EntrypointSelection::default()
            })
        }
    }
}

fn validated_selection(
    binder: &CompilationBinderFacts<'_>,
    semantic_values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
    symbols: &SymbolGraph,
    function: FunctionSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<EntrypointSelection, FactQueryError> {
    let validation = validate_entry(
        binder,
        semantic_values,
        available,
        symbols,
        function,
        ProductEntryKind::Executable,
        diagnostics,
    )?;

    Ok(EntrypointSelection {
        function: validation.map(|_| function),
        is_async: validation.unwrap_or(false),
        is_recovered: validation.is_none(),
    })
}

pub(super) fn validate_entry(
    binder: &CompilationBinderFacts<'_>,
    semantic_values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
    symbols: &SymbolGraph,
    function: FunctionSymbolId,
    kind: ProductEntryKind,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<bool>, FactQueryError> {
    let Some(symbol) = symbols.function(function) else {
        return Err(FactQueryError::InfrastructureFailure);
    };

    let Some(anchor) = symbol.syntax_anchor() else {
        return Err(FactQueryError::InfrastructureFailure);
    };

    let signature = binder
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
            function.into(),
        ))
        .map_err(binder_fact_error)?;

    diagnostics.add_range(signature.diagnostics().iter().cloned());

    let mut is_valid = !signature.diagnostics().has_errors();

    if !symbol.generic_type_parameters().is_empty() || !symbol.generic_const_parameters().is_empty()
    {
        diagnostics.add(source_diagnostic(
            anchor,
            DiagnosticKind::CheckingEntryCannotBeGeneric,
        ));

        is_valid = false;
    }

    if !signature.value().parameters().is_empty() {
        diagnostics.add(source_diagnostic(
            anchor,
            DiagnosticKind::CheckingEntryCannotTakeParameters,
        ));

        is_valid = false;
    }

    let Some((constness, execution, trust)) =
        callable_properties(signature.value(), semantic_values)
    else {
        return Ok(None);
    };

    if constness == CallableConstness::Constant {
        diagnostics.add(source_diagnostic(
            anchor,
            DiagnosticKind::CheckingEntryCannotBeConstant,
        ));

        is_valid = false;
    }

    if trust == CallableTrust::Trusted {
        diagnostics.add(source_diagnostic(
            anchor,
            DiagnosticKind::CheckingEntryCannotRequireTrust,
        ));

        is_valid = false;
    }

    let valid_result = match kind {
        ProductEntryKind::Executable => {
            is_unit(signature.value().result(), semantic_values, available)
                || is_role(
                    signature.value().result(),
                    semantic_values,
                    available,
                    RepresentationRole::ScalarI32,
                )
                || is_result_of_unit(signature.value().result(), semantic_values, available)
        }
        ProductEntryKind::Test => {
            is_unit(signature.value().result(), semantic_values, available)
                || is_result_of_unit(signature.value().result(), semantic_values, available)
        }
    };

    if !valid_result {
        let diagnostic = match kind {
            ProductEntryKind::Executable => DiagnosticKind::CheckingInvalidEntrypointResult,
            ProductEntryKind::Test => DiagnosticKind::CheckingInvalidTestResult,
        };

        diagnostics.add(source_diagnostic(anchor, diagnostic));

        is_valid = false;
    }

    Ok(is_valid.then_some(execution == CallableExecution::Asynchronous))
}

fn callable_properties(
    signature: &CallableSignatureTemplate,
    semantic_values: &SemanticValueStore,
) -> Option<(CallableConstness, CallableExecution, CallableTrust)> {
    match signature.callable_type() {
        TypeExpressionTemplate::Callable(callable) => {
            Some((callable.constness(), callable.execution(), callable.trust()))
        }
        TypeExpressionTemplate::Resolved(ty) => {
            let data = semantic_values.type_data(*ty).ok()?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return None;
            };

            Some((callable.constness(), callable.execution(), callable.trust()))
        }
        _ => None,
    }
}

fn is_result_of_unit(
    template: &TypeExpressionTemplate,
    semantic_values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
) -> bool {
    match template {
        TypeExpressionTemplate::Named {
            definition,
            arguments,
            ..
        } if named_role(*definition, available) == Some(RepresentationRole::Result) => {
            matches!(
                arguments.first(),
                Some(GenericArgumentTemplate::Type(result))
                    if is_unit(result, semantic_values, available)
            )
        }
        TypeExpressionTemplate::Resolved(ty) => {
            let Ok(data) = semantic_values.type_data(*ty) else {
                return false;
            };

            let TypeData::Named {
                definition,
                substitution,
            } = data.as_ref()
            else {
                return false;
            };

            if named_role(*definition, available) != Some(RepresentationRole::Result) {
                return false;
            }

            let Ok(substitution) = semantic_values.generic_substitution_data(*substitution) else {
                return false;
            };

            matches!(
                substitution.bindings().first().map(|binding| binding.argument()),
                Some(GenericArgument::Type(result))
                    if resolved_is_unit(result, semantic_values, available)
            )
        }
        _ => false,
    }
}

fn is_unit(
    template: &TypeExpressionTemplate,
    semantic_values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
) -> bool {
    is_role(
        template,
        semantic_values,
        available,
        RepresentationRole::Unit,
    )
}

fn is_role(
    template: &TypeExpressionTemplate,
    semantic_values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
    expected: RepresentationRole,
) -> bool {
    match template {
        TypeExpressionTemplate::Named { definition, .. } => {
            named_role(*definition, available) == Some(expected)
        }
        TypeExpressionTemplate::Resolved(ty) => {
            semantic_values
                .type_data(*ty)
                .ok()
                .and_then(|data| match data.as_ref() {
                    TypeData::Named { definition, .. } => named_role(*definition, available),
                    _ => None,
                })
                == Some(expected)
        }
        _ => false,
    }
}

fn resolved_is_unit(
    ty: bray_symbols::TypeId,
    semantic_values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
) -> bool {
    semantic_values
        .type_data(ty)
        .ok()
        .and_then(|data| match data.as_ref() {
            TypeData::Named { definition, .. } => named_role(*definition, available),
            _ => None,
        })
        == Some(RepresentationRole::Unit)
}

fn named_role(
    definition: NamedTypeSymbolId,
    available: &AvailableCompilerKnownSymbols,
) -> Option<RepresentationRole> {
    match definition {
        NamedTypeSymbolId::Struct(symbol) => available.symbol_representation(symbol),
        NamedTypeSymbolId::Union(symbol) => available.symbol_representation(symbol),
    }
}

fn missing_entrypoint_diagnostic(symbols: &SymbolGraph) -> Diagnostic {
    let kind = DiagnosticKind::CheckingMissingEntrypoint;
    let anchor = symbols
        .modules()
        .iter()
        .find(|module| module.origin() == bray_symbols::SymbolOrigin::Source)
        .and_then(|module| symbols.declaration_syntax_anchor(module.id().into()));

    anchor.map_or_else(
        || Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error),
        |anchor| source_diagnostic(anchor, kind),
    )
}
