use bray_checker::{ConstantLiteralError, check_constant_literal, normalize_integer_literal};
use bray_compiler_known::IntegerRepresentation;
use bray_diagnostics::DiagnosticKind;
use bray_symbols::{
    ConstantTermData, ConstantTermId, ConstantValueData, ConstantValueKind,
    GenericConstParameterDeclaredTypeFact, GenericConstParameterSymbolId, NamedTypeSymbolId,
    SymbolFactRequest, TypeData, TypeId,
};
use bray_syntax::{
    ExpressionSyntax, GenericArgumentSyntax, SourceSyntaxNode, TypeExpressionSyntax,
};

use super::core::{TypeExpressionBinder, token_text};
use super::diagnostic::source_diagnostic;
use crate::binding::expression::literal_kind;
use crate::{BinderFactError, BinderFactResult};

impl TypeExpressionBinder<'_> {
    pub(super) fn bind_array_length(
        &mut self,
        expression: &ExpressionSyntax,
    ) -> BinderFactResult<ConstantTermId> {
        if expression.is_recovered() {
            let expected =
                self.bind_compiler_known_type(bray_compiler_known::RepresentationRole::ScalarI32)?;

            return self.error_constant_term(expected);
        }

        let expected = match self.constant_parameter_reference(expression) {
            Some(reference) => reference?.1,
            None => {
                self.bind_compiler_known_type(bray_compiler_known::RepresentationRole::ScalarI32)?
            }
        };

        self.bind_constant_expression(expression, expected)
    }

    pub(super) fn bind_generic_constant_argument(
        &mut self,
        argument: &GenericArgumentSyntax,
        parameter: GenericConstParameterSymbolId,
    ) -> BinderFactResult<ConstantTermId> {
        let expected = self.constant_parameter_type(parameter)?;

        if let Some(expression) = argument.expressions().next() {
            return self.bind_constant_expression(&expression, expected);
        }

        if let Some(ty) = argument.type_expressions().next() {
            return self.bind_constant_type_argument(&ty, expected);
        }

        self.error_constant_term(expected)
    }

    pub(super) fn bind_constant_expression(
        &mut self,
        expression: &ExpressionSyntax,
        expected: TypeId,
    ) -> BinderFactResult<ConstantTermId> {
        self.check_cancellation()?;

        if expression.is_recovered() {
            return self.error_constant_term(expected);
        }

        let Some(primary) = expression.primary_expression() else {
            return self.invalid_constant_expression(expression, expected);
        };

        if let Some(literal) = primary.literal_expressions().next() {
            return self.bind_constant_literal(&literal, expected);
        }

        if let Some(access) = primary.access_expression() {
            let Some(token) = access.identifier_token() else {
                return self.invalid_constant_expression(expression, expected);
            };

            if access.access_expressions().next().is_some()
                || access.member_access_operations().next().is_some()
                || access.element_index_operations().next().is_some()
            {
                return self.invalid_constant_expression(expression, expected);
            }

            let Some(name) = token_text(expression.source(), &token) else {
                return self.error_constant_term(expected);
            };

            return self.bind_constant_parameter(name, expected, expression);
        }

        if let Some(grouped) = primary.grouped_expressions().next() {
            return self.bind_constant_expression(&grouped.expression(), expected);
        }

        self.invalid_constant_expression(expression, expected)
    }

    fn constant_parameter_reference(
        &mut self,
        expression: &ExpressionSyntax,
    ) -> Option<BinderFactResult<(GenericConstParameterSymbolId, TypeId)>> {
        let primary = expression.primary_expression()?;
        let access = primary.access_expression()?;
        let token = access.identifier_token()?;

        if access.access_expressions().next().is_some()
            || access.member_access_operations().next().is_some()
            || access.element_index_operations().next().is_some()
        {
            return None;
        }

        let name = token_text(expression.source(), &token)?;

        let parameter = self.const_parameters.get(name).copied()?;

        Some(
            self.constant_parameter_type(parameter)
                .map(|ty| (parameter, ty)),
        )
    }

    fn bind_constant_type_argument(
        &mut self,
        syntax: &TypeExpressionSyntax,
        expected: TypeId,
    ) -> BinderFactResult<ConstantTermId> {
        if syntax.is_recovered() {
            return self.error_constant_term(expected);
        }

        let Some(path) = syntax.path() else {
            return self.invalid_constant_type_argument(syntax, expected);
        };

        let mut tokens = path.identifier_tokens();
        let Some(token) = tokens.next() else {
            return self.error_constant_term(expected);
        };

        if tokens.next().is_some() {
            return self.invalid_constant_type_argument(syntax, expected);
        }

        let Some(name) = token_text(syntax.source(), &token) else {
            return self.error_constant_term(expected);
        };

        let Some(parameter) = self.const_parameters.get(name).copied() else {
            return self.invalid_constant_type_argument(syntax, expected);
        };

        let ty = self.constant_parameter_type(parameter)?;

        if ty != expected {
            return self.invalid_constant_type_argument(syntax, expected);
        }

        self.intern_constant_term(ConstantTermData::Parameter(parameter))
    }

    fn bind_constant_parameter(
        &mut self,
        name: &str,
        expected: TypeId,
        expression: &ExpressionSyntax,
    ) -> BinderFactResult<ConstantTermId> {
        let Some(parameter) = self.const_parameters.get(name).copied() else {
            return self.invalid_constant_expression(expression, expected);
        };

        let ty = self.constant_parameter_type(parameter)?;

        if ty != expected {
            return self.invalid_constant_expression(expression, expected);
        }

        self.intern_constant_term(ConstantTermData::Parameter(parameter))
    }

    fn constant_parameter_type(
        &mut self,
        parameter: GenericConstParameterSymbolId,
    ) -> BinderFactResult<TypeId> {
        let declared_type = self.const_parameter_types.symbol_fact(SymbolFactRequest::<
            GenericConstParameterDeclaredTypeFact,
        >::new(parameter))?;

        self.diagnostics
            .add_range(declared_type.diagnostics().iter().cloned());

        Ok(*declared_type.value())
    }

    fn bind_constant_literal(
        &mut self,
        literal: &bray_syntax::LiteralExpressionSyntax,
        expected: TypeId,
    ) -> BinderFactResult<ConstantTermId> {
        let Some(token) = literal.literal_token() else {
            return self.error_constant_term(expected);
        };

        let Some(kind) = literal_kind(token.kind()) else {
            return self.invalid_constant_literal(literal, expected);
        };

        let Some(text) = token_text(literal.source(), &token) else {
            return self.error_constant_term(expected);
        };

        let Some(representation) = self.type_representation(expected)? else {
            return self.invalid_constant_literal(literal, expected);
        };

        if matches!(
            representation.integer_representation(),
            Some(IntegerRepresentation::TargetSigned | IntegerRepresentation::TargetUnsigned)
        ) {
            let value = match normalize_integer_literal(text) {
                Ok(value) => value,
                Err(error) => {
                    self.report_constant_literal_error(literal, error);

                    return self.error_constant_term(expected);
                }
            };

            return self.intern_constant_term(ConstantTermData::IntegerLiteral {
                ty: expected,
                value,
            });
        }

        let value = check_constant_literal(kind, text, representation, None);

        let value = match value {
            Ok(value) => value,
            Err(error) => {
                self.report_constant_literal_error(literal, error);

                return self.error_constant_term(expected);
            }
        };

        let value = self
            .semantic_values
            .intern_constant_value(ConstantValueData::new(expected, value))
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        self.intern_constant_term(ConstantTermData::Value(value))
    }

    fn type_representation(
        &self,
        ty: TypeId,
    ) -> BinderFactResult<Option<bray_compiler_known::RepresentationRole>> {
        let data = self
            .semantic_values
            .type_data(ty)
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        let TypeData::Named { definition, .. } = data.as_ref() else {
            return Ok(None);
        };

        let roles = self.symbols.compiler_known_provider().role_registry();
        let role = match definition {
            NamedTypeSymbolId::Struct(definition) => roles.symbol_representation(*definition),
            NamedTypeSymbolId::Union(definition) => roles.symbol_representation(*definition),
        };

        Ok(role)
    }

    fn invalid_constant_expression(
        &mut self,
        syntax: &ExpressionSyntax,
        expected: TypeId,
    ) -> BinderFactResult<ConstantTermId> {
        self.report_invalid_constant(syntax);
        self.error_constant_term(expected)
    }

    fn invalid_constant_type_argument(
        &mut self,
        syntax: &TypeExpressionSyntax,
        expected: TypeId,
    ) -> BinderFactResult<ConstantTermId> {
        self.report_invalid_constant(syntax);
        self.error_constant_term(expected)
    }

    fn invalid_constant_literal(
        &mut self,
        syntax: &bray_syntax::LiteralExpressionSyntax,
        expected: TypeId,
    ) -> BinderFactResult<ConstantTermId> {
        self.report_invalid_constant(syntax);
        self.error_constant_term(expected)
    }

    fn report_constant_literal_error(
        &mut self,
        syntax: &bray_syntax::LiteralExpressionSyntax,
        error: ConstantLiteralError,
    ) {
        let kind = match error {
            ConstantLiteralError::Invalid | ConstantLiteralError::TargetIntegerWidthRequired => {
                DiagnosticKind::CheckingInvalidConstantExpression
            }
            ConstantLiteralError::NotRepresentable => {
                DiagnosticKind::CheckingConstantLiteralNotRepresentable
            }
            ConstantLiteralError::SizeLimitExceeded => {
                DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded
            }
        };

        self.report_diagnostic(syntax, kind);
    }

    fn report_invalid_constant(&mut self, syntax: &impl SourceSyntaxNode) {
        self.report_diagnostic(syntax, DiagnosticKind::CheckingInvalidConstantExpression);
    }

    fn report_diagnostic(&mut self, syntax: &impl SourceSyntaxNode, kind: DiagnosticKind) {
        self.diagnostics.add(source_diagnostic(syntax, kind));
    }

    pub(super) fn error_constant_term(&self, expected: TypeId) -> BinderFactResult<ConstantTermId> {
        let value = self
            .semantic_values
            .intern_constant_value(ConstantValueData::new(expected, ConstantValueKind::Error))
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        self.intern_constant_term(ConstantTermData::Value(value))
    }

    fn intern_constant_term(&self, data: ConstantTermData) -> BinderFactResult<ConstantTermId> {
        self.semantic_values
            .intern_constant_term(data)
            .map_err(|_| BinderFactError::DependencyUnavailable)
    }
}
