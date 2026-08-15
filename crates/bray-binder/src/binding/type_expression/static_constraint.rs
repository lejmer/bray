use bray_diagnostics::DiagnosticResult;
use bray_symbols::{TypeData, TypeExpressionTemplate};
use bray_syntax::{
    ExpressionSyntax, PathSyntax, SourceSyntaxNode, StaticTypeOperandSyntax, TypeExpressionSyntax,
};

use super::core::TypeExpressionBinder;
use crate::BindingQueryResult;

impl TypeExpressionBinder<'_> {
    /// Binds an operand that may denote a type in static constraint context.
    pub fn bind_static_type_operand(
        mut self,
        operand: &StaticTypeOperandSyntax,
    ) -> BindingQueryResult<Option<DiagnosticResult<TypeExpressionTemplate>>> {
        let syntax = match operand {
            StaticTypeOperandSyntax::Type(syntax) => syntax.clone(),
            StaticTypeOperandSyntax::Expression(expression) => {
                let Some(syntax) = static_type_expression_syntax(expression) else {
                    return Ok(None);
                };

                syntax
            }
        };

        self.check_cancellation()?;

        let ty = self.bind_type(&syntax)?;

        if ty.resolved_type().is_some_and(|ty| {
            self.semantic_values
                .type_data(ty)
                .is_ok_and(|data| *data == TypeData::Error)
        }) {
            return Ok(None);
        }

        self.check_cancellation()?;

        Ok(Some(DiagnosticResult::new(ty, self.diagnostics)))
    }
}

fn static_type_expression_syntax(expression: &ExpressionSyntax) -> Option<TypeExpressionSyntax> {
    if let Some(path) = static_type_path(expression) {
        let mut builder = TypeExpressionSyntax::builder(
            expression.source().clone(),
            expression.full_range().start(),
        );

        builder.push_path(path);

        return Some(builder.build());
    }

    let mut expressions = expression.expressions();
    let subject = expressions.next()?;

    if expressions.next().is_some() {
        return None;
    }

    let mut operations = expression.trait_qualified_member_operations();
    let operation = operations.next()?;

    if operations.next().is_some() {
        return None;
    }

    let subject = static_type_expression_syntax(&subject)?;
    let member = operation.member_access_operation();

    let mut builder =
        TypeExpressionSyntax::builder(expression.source().clone(), expression.full_range().start());

    builder.push_type_expression(subject);
    builder.push_open_paren_token(operation.open_paren_token());
    builder.push_trait_application(operation.trait_application());
    builder.push_close_paren_token(operation.close_paren_token());
    builder.push_dot_token(member.dot_token());
    builder.push_identifier_token(member.identifier_token()?);

    Some(builder.build())
}

fn static_type_path(expression: &ExpressionSyntax) -> Option<PathSyntax> {
    let mut identifiers = Vec::new();
    let mut dots = Vec::new();

    collect_static_type_path(expression, &mut identifiers, &mut dots)?;

    let mut builder = PathSyntax::builder(expression.source().clone());

    builder.push_identifier_token(identifiers.remove(0));

    for (dot, identifier) in dots.into_iter().zip(identifiers) {
        builder.push_dot_token(dot);
        builder.push_identifier_token(identifier);
    }

    Some(builder.build())
}

fn collect_static_type_path(
    expression: &ExpressionSyntax,
    identifiers: &mut Vec<bray_syntax::SyntaxToken>,
    dots: &mut Vec<bray_syntax::SyntaxToken>,
) -> Option<()> {
    if let Some(primary) = expression.primary_expression() {
        let access = primary.access_expression()?;

        if access.access_expressions().next().is_some()
            || access.member_access_operations().next().is_some()
            || access.element_index_operations().next().is_some()
        {
            return None;
        }

        identifiers.push(access.identifier_token()?);

        return Some(());
    }

    let mut expressions = expression.expressions();
    let subject = expressions.next()?;

    if expressions.next().is_some() {
        return None;
    }

    let mut members = expression.member_access_operations();
    let member = members.next()?;

    if members.next().is_some() {
        return None;
    }

    collect_static_type_path(&subject, identifiers, dots)?;

    dots.push(member.dot_token());
    identifiers.push(member.identifier_token()?);

    Some(())
}
