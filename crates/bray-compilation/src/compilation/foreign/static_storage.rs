use std::sync::Arc;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_diagnostics::{DiagnosticBag, DiagnosticKind, DiagnosticResult};
use bray_symbols::{
    CallableAbi, DirectiveKind, ForeignCallableDirection, ForeignStaticContract,
    NativeSymbolBinding, NativeSymbolPresence, StaticDeclaredTypeQuery, StaticSymbolId,
    SymbolQueryRequest, TypeData, TypeExpressionTemplate,
};
use bray_syntax::StaticDeclarationSyntax;

use super::super::Compilation;
use super::diagnostic::{missing_directive, source_diagnostic};
use super::directive::{foreign_link_requirements, foreign_symbol_contract, invalid_symbol_policy};
use super::validation::foreign_type_is_supported;
use super::{ForeignDataKind, ForeignQueryContext, ForeignQueryFailure};
use crate::compilation::binder::binding_query_error;
use crate::compilation::directive::first_directive;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    /// Returns the validated foreign-boundary contract of one source static.
    pub fn foreign_static_contract(
        &self,
        declaration: StaticSymbolId,
    ) -> Result<Arc<DiagnosticResult<Option<ForeignStaticContract>>>, FactQueryError> {
        self.foreign_static_contract_with_cancellation(declaration, &self.state.cancellation)
    }

    pub(in crate::compilation) fn foreign_static_contract_with_cancellation(
        &self,
        declaration: StaticSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<ForeignStaticContract>>>, FactQueryError> {
        let binder = self.binding_context(cancellation)?;

        if binder
            .imported_semantic_address(declaration.into())
            .map_err(binding_query_error)?
            .is_some()
        {
            return Ok(Arc::new(DiagnosticResult::without_diagnostics(None)));
        }

        let cell = self.state.foreign_static_contracts.cell(declaration)?;

        let result = self.query_with_cancellation(
            crate::fact::CompilationFactKey::ForeignStaticContract(declaration),
            &cell,
            cancellation,
            |cancellation| self.compute_foreign_static_contract(declaration, cancellation),
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_foreign_static_contract(
        &self,
        declaration: StaticSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<ForeignStaticContract>>>, FactQueryError> {
        cancellation.check()?;

        let binder = self.binding_context(cancellation)?;

        let record = binder.symbols().static_symbol(declaration).ok_or_else(|| {
            ForeignQueryFailure::missing(
                ForeignQueryContext::Static(declaration),
                ForeignDataKind::StaticBindingRecord,
            )
        })?;

        let anchor = record.syntax_anchor().ok_or_else(|| {
            ForeignQueryFailure::missing(
                ForeignQueryContext::Static(declaration),
                ForeignDataKind::SourceAnchor,
            )
        })?;

        let syntax = anchor
            .find_descendant::<StaticDeclarationSyntax>(self.syntax_tree())
            .ok_or_else(|| {
                ForeignQueryFailure::missing(
                    ForeignQueryContext::Static(declaration),
                    ForeignDataKind::StaticDeclarationSyntax,
                )
            })?;

        let directives = self.declaration_directives(declaration.into())?;
        let mut diagnostics = directives.diagnostics().clone();
        let symbol_directive = first_directive(directives.value(), DirectiveKind::Symbol);

        let is_extern = syntax
            .static_declaration_modifiers()
            .extern_token()
            .is_some();

        let direction = match (is_extern, symbol_directive.is_some()) {
            (true, _) => Some(ForeignCallableDirection::Import),
            (false, true) => Some(ForeignCallableDirection::Export),
            (false, false) => None,
        };

        let Some(direction) = direction else {
            return Ok(Arc::new(DiagnosticResult::new(None, diagnostics)));
        };

        validate_static_surface(record, &syntax, direction, anchor, &mut diagnostics);

        let symbol = match symbol_directive {
            Some(directive) => {
                foreign_symbol_contract(self, directive, cancellation, &mut diagnostics)?
            }
            None => {
                diagnostics.add(missing_directive(
                    anchor,
                    bray_syntax::SyntaxKind::SymbolDirective,
                ));

                None
            }
        };

        if let Some(symbol) = &symbol {
            match (direction, symbol.binding(), symbol.presence()) {
                (
                    ForeignCallableDirection::Import,
                    NativeSymbolBinding::Weak,
                    NativeSymbolPresence::Required | NativeSymbolPresence::Optional,
                ) => {
                    diagnostics.add(invalid_symbol_policy(
                        symbol_directive.map_or(anchor, bray_symbols::DirectiveTemplate::syntax),
                        "binding",
                    ));
                }
                (ForeignCallableDirection::Import, _, NativeSymbolPresence::Optional)
                    if !self
                        .requested_target()
                        .profile()
                        .properties()
                        .native_symbols()
                        .optional_data() =>
                {
                    diagnostics.add(invalid_symbol_policy(
                        symbol_directive.map_or(anchor, bray_symbols::DirectiveTemplate::syntax),
                        "presence",
                    ));
                }
                (ForeignCallableDirection::Export, _, NativeSymbolPresence::Optional) => {
                    diagnostics.add(invalid_symbol_policy(
                        symbol_directive.map_or(anchor, bray_symbols::DirectiveTemplate::syntax),
                        "presence",
                    ));
                }
                _ => {}
            }
        }

        let declared_type = binder
            .resolve_symbol_query(SymbolQueryRequest::<StaticDeclaredTypeQuery>::new(
                declaration,
            ))
            .map_err(binding_query_error)?;

        diagnostics.add_range(declared_type.diagnostics().iter().cloned());

        if !native_static_type_is_supported(self, declared_type.value(), direction, cancellation)? {
            diagnostics.add(source_diagnostic(
                anchor,
                DiagnosticKind::CheckingNativeStaticTypeUnsupported,
            ));
        }

        let links = if direction == ForeignCallableDirection::Import {
            foreign_link_requirements(
                self,
                declaration.into(),
                directives.value(),
                cancellation,
                &mut diagnostics,
            )?
        } else {
            Vec::new()
        };

        let contract = if diagnostics.has_errors() {
            None
        } else {
            symbol.map(|symbol| {
                ForeignStaticContract::new(
                    declaration,
                    direction,
                    symbol,
                    syntax.mut_token().is_some(),
                    links,
                )
            })
        };

        Ok(Arc::new(DiagnosticResult::new(contract, diagnostics)))
    }
}

fn validate_static_surface(
    record: &bray_symbols::StaticSymbol,
    syntax: &StaticDeclarationSyntax,
    direction: ForeignCallableDirection,
    anchor: bray_declarations::SyntaxAnchor,
    diagnostics: &mut DiagnosticBag,
) {
    let has_generics = !record.generic_type_parameters().is_empty()
        || !record.generic_const_parameters().is_empty()
        || syntax.with_clauses().next().is_some();

    let valid = match direction {
        ForeignCallableDirection::Import => {
            syntax
                .static_declaration_modifiers()
                .trusted_token()
                .is_some()
                && syntax.expression().is_none()
                && !has_generics
        }
        ForeignCallableDirection::Export => {
            syntax.expression().is_some()
                && !has_generics
                && syntax
                    .static_directives()
                    .thread_local_directives()
                    .next()
                    .is_none()
                && (syntax.mut_token().is_none()
                    || syntax
                        .static_declaration_modifiers()
                        .trusted_token()
                        .is_some())
        }
    };

    if !valid {
        let kind = match direction {
            ForeignCallableDirection::Import => {
                DiagnosticKind::CheckingExternStaticSurfaceUnsupported
            }
            ForeignCallableDirection::Export => {
                DiagnosticKind::CheckingExportedStaticSurfaceUnsupported
            }
        };

        diagnostics.add(source_diagnostic(anchor, kind));
    }
}

fn native_static_type_is_supported(
    compilation: &Compilation,
    ty: &TypeExpressionTemplate,
    direction: ForeignCallableDirection,
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    if direction == ForeignCallableDirection::Import
        && native_static_type_is_incomplete(compilation, ty, cancellation)?
    {
        return Ok(true);
    }

    foreign_type_is_supported(compilation, ty, CallableAbi::C, cancellation)
}

fn native_static_type_is_incomplete(
    compilation: &Compilation,
    ty: &TypeExpressionTemplate,
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    let definition = match ty {
        TypeExpressionTemplate::Resolved(ty) => {
            let data = compilation
                .semantic_value_store()?
                .type_data(*ty)
                .map_err(FactQueryError::SemanticValueStore)?;

            let TypeData::Named { definition, .. } = data.as_ref() else {
                return Ok(false);
            };

            *definition
        }
        TypeExpressionTemplate::Named { definition, .. } => *definition,
        _ => return Ok(false),
    };

    let representation =
        compilation.declared_type_representation_with_cancellation(definition, cancellation)?;

    Ok(representation.value().is_incomplete())
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_symbols::{StaticSymbolId, SymbolId, SymbolOrigin};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::super::{ForeignDataKind, ForeignQueryContext, ForeignQueryFailure};
    use crate::fact::FactQueryError;
    use crate::test_support::compilation;

    fn foreign_static_diagnostics(source: &str) -> DiagnosticBag {
        let compilation = compilation(source);

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let declaration = symbols
            .statics()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(bray_symbols::StaticSymbol::id)
            .unwrap_or_else(|| panic!("test package must declare one source static"));

        compilation
            .foreign_static_contract(declaration)
            .unwrap_or_else(|error| panic!("foreign static contract must be available: {error:?}"))
            .diagnostics()
            .clone()
    }

    #[test]
    fn invalid_foreign_static_surfaces_publish_exact_diagnostics() {
        let imported = foreign_static_diagnostics(concat!(
            "module app;\n",
            "@symbol(name = \"foreign_value\")\n",
            "extern static FOREIGN_VALUE: i32;\n",
        ));

        assert_goal_state_diagnostic_kind(
            &imported,
            DiagnosticKind::CheckingExternStaticSurfaceUnsupported,
        );

        let exported = foreign_static_diagnostics(concat!(
            "module app;\n",
            "@symbol(name = \"exported_value\")\n",
            "static EXPORTED_VALUE<T>: i32 = 0;\n",
        ));

        assert_goal_state_diagnostic_kind(
            &exported,
            DiagnosticKind::CheckingExportedStaticSurfaceUnsupported,
        );

        let unsupported_type = foreign_static_diagnostics(concat!(
            "module app;\n",
            "@symbol(name = \"callback\")\n",
            "extern trusted static CALLBACK: func() -> i32;\n",
        ));

        assert_goal_state_diagnostic_kind(
            &unsupported_type,
            DiagnosticKind::CheckingNativeStaticTypeUnsupported,
        );
    }

    #[test]
    fn missing_static_records_retain_the_requested_declaration() {
        let compilation = compilation("module app;\n");
        let declaration = StaticSymbolId::from_symbol_id(SymbolId::new(37));

        let error = compilation
            .foreign_static_contract(declaration)
            .expect_err("unknown static declaration must fail its foreign query");

        let FactQueryError::Foreign(error) = error else {
            panic!("missing static declaration must retain the foreign-query boundary")
        };

        assert_eq!(
            error.cause(),
            &ForeignQueryFailure::Missing {
                context: ForeignQueryContext::Static(declaration),
                data: ForeignDataKind::StaticBindingRecord,
            }
        );
    }

    #[test]
    fn exported_static_symbol_name_is_checked_as_text() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@symbol(name = \"foreign_counter\")\n",
            "extern trusted static FOREIGN_COUNTER: i32;\n",
            "@symbol(name = \"exported_value\")\n",
            "static EXPORTED_VALUE: i32 = 41;\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let declaration = symbols
            .statics()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .nth(1)
            .map(bray_symbols::StaticSymbol::id)
            .unwrap_or_else(|| panic!("test package must declare one source static"));

        let contract = compilation
            .foreign_static_contract(declaration)
            .unwrap_or_else(|error| panic!("foreign static contract must be available: {error:?}"));

        assert!(
            contract.diagnostics().is_empty(),
            "{:#?}",
            contract.diagnostics()
        );

        assert!(contract.value().is_some());
    }

    #[test]
    fn imported_statics_reject_weak_binding_without_weak_required_resolution() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@symbol(name = \"foreign_counter\", binding = weak)\n",
            "extern trusted static FOREIGN_COUNTER: i32;\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let declaration = symbols
            .statics()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(bray_symbols::StaticSymbol::id)
            .unwrap_or_else(|| panic!("test package must declare one source static"));

        let contract = compilation
            .foreign_static_contract(declaration)
            .unwrap_or_else(|error| panic!("foreign static contract must be available: {error:?}"));

        assert!(contract.value().is_none());

        assert_goal_state_diagnostic_kind(
            contract.diagnostics(),
            DiagnosticKind::CheckingInvalidNativeSymbolDirective,
        );
    }
}
