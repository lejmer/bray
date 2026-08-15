use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    ConstantExpressionExpectedType, ConstantExpressionOccurrence, ConstantExpressionOccurrenceKey,
    GenericConstParameterSymbolId,
};
use bray_syntax::{ExpressionSyntax, GenericArgumentSyntax, SourceSyntaxNode};

use super::core::TypeExpressionBinder;
use crate::{BindingQueryError, BindingQueryResult};

impl TypeExpressionBinder<'_> {
    pub(super) fn bind_array_length(
        &mut self,
        expression: &ExpressionSyntax,
    ) -> BindingQueryResult<ConstantExpressionOccurrence> {
        let expected = self.bind_compiler_known_type_id(RepresentationRole::ScalarUsize)?;

        Ok(self.constant_expression_occurrence(
            expression,
            ConstantExpressionExpectedType::Resolved(expected),
        ))
    }

    pub(super) fn bind_generic_constant_argument(
        &self,
        argument: &GenericArgumentSyntax,
        parameter: GenericConstParameterSymbolId,
    ) -> BindingQueryResult<ConstantExpressionOccurrence> {
        let expected = ConstantExpressionExpectedType::GenericParameter(parameter);

        if let Some(expression) = argument.expressions().next() {
            return Ok(self.constant_expression_occurrence(&expression, expected));
        }

        if let Some(ty) = argument.type_expressions().next() {
            return Ok(self.constant_expression_occurrence(&ty, expected));
        }

        if argument.is_recovered() {
            return Ok(self.constant_expression_occurrence(argument, expected));
        }

        Err(BindingQueryError::DependencyUnavailable)
    }

    fn constant_expression_occurrence(
        &self,
        syntax: &impl SourceSyntaxNode,
        expected: ConstantExpressionExpectedType,
    ) -> ConstantExpressionOccurrence {
        ConstantExpressionOccurrence::new(
            ConstantExpressionOccurrenceKey::new(self.owner, SyntaxAnchor::from_node(syntax)),
            expected,
        )
    }
}
