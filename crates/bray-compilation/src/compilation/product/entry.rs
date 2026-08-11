use bray_binder::SymbolFactProvider;
use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabelKind, DiagnosticNote,
    DiagnosticNoteKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, AvailableCompilerKnownSymbols, CallableConstness, CallableContractTemplate,
    CallableContractTemplateFact, CallableContractsFact, CallableExecution, CallableSignatureFact,
    CallableSignatureTemplate, DeclarationPredicateClauseKind, FunctionSymbolId, GenericArgument,
    GenericArgumentTemplate, NamedTypeSymbolId, SemanticValueStore, SymbolFactRequest, SymbolGraph,
    TestResultShape, TypeData, TypeExpressionTemplate, TypeId,
};
use bray_syntax::TrustBoundaryExpressionSyntax;

use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::FactQueryError;

fn source_diagnostic(anchor: SyntaxAnchor, kind: DiagnosticKind) -> Diagnostic {
    crate::compilation::diagnostics::labeled_source_diagnostic(
        anchor,
        kind,
        DiagnosticLabelKind::InvalidProductEntry,
    )
}

#[derive(Clone, Copy)]
pub(super) enum ProductEntryKind {
    Executable,
    Test,
}

#[derive(Clone, Copy)]
pub(super) struct ProductEntryValidation {
    execution: CallableExecution,
    test_result: Option<TestResultShape>,
    test_error: Option<TypeId>,
}

impl ProductEntryValidation {
    pub(super) const fn execution(self) -> CallableExecution {
        self.execution
    }

    pub(super) const fn is_async(self) -> bool {
        matches!(self.execution, CallableExecution::Asynchronous)
    }

    pub(super) const fn test_result(self) -> Option<TestResultShape> {
        self.test_result
    }

    pub(super) const fn test_error(self) -> Option<TypeId> {
        self.test_error
    }
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
    product_anchor: SyntaxAnchor,
    diagnostics: &mut DiagnosticBag,
) -> Result<EntrypointSelection, FactQueryError> {
    if explicit.len() > 1 {
        let first = symbols
            .declaration_syntax_anchor(explicit[0].0.into())
            .unwrap_or(explicit[0].1);

        let count = u64::try_from(explicit.len()).unwrap_or(u64::MAX);

        for (_, anchor) in explicit.iter().skip(1) {
            diagnostics.add(
                source_diagnostic(*anchor, DiagnosticKind::CheckingDuplicateEntrypoint)
                    .with_arg(DiagnosticArg::actual_count(count))
                    .with_related_location(DiagnosticRelatedLocation::new(
                        DiagnosticRelatedLocationKind::FirstDeclaration,
                        SourceSpan::new(first.source_id(), first.full_range()),
                    ))
                    .with_note(DiagnosticNote::new(
                        DiagnosticNoteKind::ExecutableEntrypointRequired,
                    )),
            );
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
        diagnostics.add(missing_entrypoint_diagnostic(symbols, product_anchor));

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

        if let Some(validation) = validation {
            valid.push((function, validation.is_async()));
        }
    }

    match valid.as_slice() {
        [(function, is_async)] => Ok(EntrypointSelection {
            function: Some(*function),
            is_async: *is_async,
            is_recovered: false,
        }),
        [] => {
            diagnostics.add(missing_entrypoint_diagnostic(symbols, product_anchor));

            Ok(EntrypointSelection {
                is_recovered: true,
                ..EntrypointSelection::default()
            })
        }
        [_, rest @ ..] => {
            let first = valid[0].0;
            let first_anchor = symbols.declaration_syntax_anchor(first.into());
            let count = u64::try_from(valid.len()).unwrap_or(u64::MAX);

            for (function, _) in rest {
                if let Some(anchor) = symbols.declaration_syntax_anchor((*function).into()) {
                    let mut diagnostic =
                        source_diagnostic(anchor, DiagnosticKind::CheckingDuplicateEntrypoint)
                            .with_arg(DiagnosticArg::actual_count(count))
                            .with_note(DiagnosticNote::new(
                                DiagnosticNoteKind::ExecutableEntrypointRequired,
                            ));

                    if let Some(first_anchor) = first_anchor {
                        diagnostic =
                            diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                                DiagnosticRelatedLocationKind::FirstDeclaration,
                                SourceSpan::new(
                                    first_anchor.source_id(),
                                    first_anchor.full_range(),
                                ),
                            ));
                    }

                    diagnostics.add(diagnostic);
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
        is_async: validation.is_some_and(ProductEntryValidation::is_async),
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
) -> Result<Option<ProductEntryValidation>, FactQueryError> {
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
        let count =
            symbol.generic_type_parameters().len() + symbol.generic_const_parameters().len();

        diagnostics.add(
            source_diagnostic(anchor, DiagnosticKind::CheckingEntryCannotBeGeneric)
                .with_arg(DiagnosticArg::actual_count(
                    u64::try_from(count).unwrap_or(u64::MAX),
                ))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::ProductEntryRequirements,
                )),
        );

        is_valid = false;
    }

    if !signature.value().parameters().is_empty() {
        diagnostics.add(
            source_diagnostic(anchor, DiagnosticKind::CheckingEntryCannotTakeParameters)
                .with_arg(DiagnosticArg::actual_count(
                    u64::try_from(signature.value().parameters().len()).unwrap_or(u64::MAX),
                ))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::ProductEntryRequirements,
                )),
        );

        is_valid = false;
    }

    let Some((constness, execution)) = callable_properties(signature.value(), semantic_values)
    else {
        return Ok(None);
    };

    if constness == CallableConstness::Constant {
        diagnostics.add(
            source_diagnostic(anchor, DiagnosticKind::CheckingEntryCannotBeConstant).with_note(
                DiagnosticNote::new(DiagnosticNoteKind::ProductEntryRequirements),
            ),
        );

        is_valid = false;
    }

    let contracts = binder
        .symbol_fact(SymbolFactRequest::<CallableContractsFact>::new(
            function.into(),
        ))
        .map_err(binder_fact_error)?;

    diagnostics.add_range(contracts.diagnostics().iter().cloned());

    let contract_template = binder
        .symbol_fact(SymbolFactRequest::<CallableContractTemplateFact>::new(
            function.into(),
        ))
        .map_err(binder_fact_error)?;

    diagnostics.add_range(contract_template.diagnostics().iter().cloned());

    if exposes_trusted_caller_obligation(contract_template.value(), binder) {
        diagnostics.add(
            source_diagnostic(anchor, DiagnosticKind::CheckingEntryCannotRequireTrust).with_note(
                DiagnosticNote::new(DiagnosticNoteKind::ProductEntryRequirements),
            ),
        );

        is_valid = false;
    }

    let unit_result = is_unit(signature.value().result(), semantic_values, available);

    let (recoverable_result, test_error) =
        result_of_unit(signature.value().result(), semantic_values, available);

    let valid_result = match kind {
        ProductEntryKind::Executable => {
            unit_result
                || is_role(
                    signature.value().result(),
                    semantic_values,
                    available,
                    RepresentationRole::ScalarI32,
                )
                || recoverable_result
        }
        ProductEntryKind::Test => unit_result || recoverable_result,
    };

    if !valid_result {
        let diagnostic = match kind {
            ProductEntryKind::Executable => DiagnosticKind::CheckingInvalidEntrypointResult,
            ProductEntryKind::Test => DiagnosticKind::CheckingInvalidTestResult,
        };

        let actual = crate::compilation::foreign::diagnostic::template_diagnostic_type(
            binder.compilation(),
            signature.value().result(),
            &binder.compilation().state.cancellation,
        )?;

        diagnostics.add(
            source_diagnostic(anchor, diagnostic)
                .with_arg(DiagnosticArg::actual_type(actual))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::ProductEntryRequirements,
                )),
        );

        is_valid = false;
    }

    let test_result = match kind {
        ProductEntryKind::Test if unit_result => Some(TestResultShape::Unit),
        ProductEntryKind::Test if recoverable_result => Some(TestResultShape::Recoverable),
        ProductEntryKind::Executable | ProductEntryKind::Test => None,
    };

    Ok(is_valid.then_some(ProductEntryValidation {
        execution,
        test_result,
        test_error,
    }))
}

fn callable_properties(
    signature: &CallableSignatureTemplate,
    semantic_values: &SemanticValueStore,
) -> Option<(CallableConstness, CallableExecution)> {
    match signature.callable_type() {
        TypeExpressionTemplate::Callable(callable) => {
            Some((callable.constness(), callable.execution()))
        }
        TypeExpressionTemplate::Resolved(ty) => {
            let data = semantic_values.type_data(*ty).ok()?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return None;
            };

            Some((callable.constness(), callable.execution()))
        }
        _ => None,
    }
}

fn exposes_trusted_caller_obligation(
    template: &CallableContractTemplate,
    binder: &CompilationBinderFacts<'_>,
) -> bool {
    let Some(template) = template.source_template() else {
        return false;
    };

    let syntax = binder.compilation().syntax_tree_result().syntax_tree();

    template.expressions().iter().any(|expression| {
        expression.kind() == DeclarationPredicateClauseKind::Requires
            && expression
                .expression()
                .syntax()
                .find_descendant::<TrustBoundaryExpressionSyntax>(syntax)
                .is_some()
    })
}

fn result_of_unit(
    template: &TypeExpressionTemplate,
    semantic_values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
) -> (bool, Option<TypeId>) {
    match template {
        TypeExpressionTemplate::Named {
            definition,
            arguments,
            ..
        } if named_role(*definition, available) == Some(RepresentationRole::Result) => {
            let [
                GenericArgumentTemplate::Type(result),
                GenericArgumentTemplate::Type(error),
            ] = arguments.as_ref()
            else {
                return (false, None);
            };

            if !is_unit(result, semantic_values, available) {
                return (false, None);
            }

            (true, error.resolved_type())
        }
        TypeExpressionTemplate::Resolved(ty) => {
            let Ok(data) = semantic_values.type_data(*ty) else {
                return (false, None);
            };

            let TypeData::Named {
                definition,
                substitution,
            } = data.as_ref()
            else {
                return (false, None);
            };

            if named_role(*definition, available) != Some(RepresentationRole::Result) {
                return (false, None);
            }

            let Ok(substitution) = semantic_values.generic_substitution_data(*substitution) else {
                return (false, None);
            };

            let [success, error] = substitution.bindings() else {
                return (false, None);
            };

            match (success.argument(), error.argument()) {
                (GenericArgument::Type(result), GenericArgument::Type(error))
                    if resolved_is_unit(result, semantic_values, available) =>
                {
                    (true, Some(error))
                }
                _ => (false, None),
            }
        }
        _ => (false, None),
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
    ty: TypeId,
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

fn missing_entrypoint_diagnostic(
    symbols: &SymbolGraph,
    product_anchor: SyntaxAnchor,
) -> Diagnostic {
    let kind = DiagnosticKind::CheckingMissingEntrypoint;

    let anchor = symbols
        .modules()
        .iter()
        .find(|module| module.origin() == bray_symbols::SymbolOrigin::Source)
        .and_then(|module| symbols.declaration_syntax_anchor(module.id().into()));

    source_diagnostic(anchor.unwrap_or(product_anchor), kind).with_note(DiagnosticNote::new(
        DiagnosticNoteKind::ExecutableEntrypointRequired,
    ))
}
