use bray_bound_tree::{
    BoundArgument, BoundCallExpression, BoundConversionExpression, BoundErrorCallExpression,
    BoundErrorConversionExpression, BoundExpression, BoundExpressionId,
    BoundStructuredExpressionKind,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::LocalScopeId;
use bray_syntax::{
    ArgumentListSyntax, CallOperationSyntax, ConversionOperationSyntax, ExpressionSyntax,
    SourceSyntaxNode, SyntaxKind, SyntaxWalkControl,
};

use super::super::BindingResult;
use super::super::name::symbol_name;
use super::ExpressionBinder;
use super::support::visit_direct_nodes;
use crate::BinderFactContext;
use crate::binder::Binder;
use crate::binding::BindingError;

impl ExpressionBinder {
    pub(super) fn bind_postfixes<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &ExpressionSyntax,
        mut current: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let mut failure = None;

        visit_direct_nodes(syntax, |operation| {
            let result = match operation.kind() {
                SyntaxKind::CallOperation => {
                    let Some(call) = operation.cast::<CallOperationSyntax>() else {
                        failure = Some(BindingError::UnsupportedSyntax);

                        return SyntaxWalkControl::Stop;
                    };

                    self.bind_call(binder, scope, &call, current)
                }
                SyntaxKind::ConversionOperation => {
                    let Some(conversion) = operation.cast::<ConversionOperationSyntax>() else {
                        failure = Some(BindingError::UnsupportedSyntax);

                        return SyntaxWalkControl::Stop;
                    };

                    self.bind_conversion(binder, &conversion, current)
                }
                SyntaxKind::MemberAccessOperation => {
                    let Some(member) = operation.cast::<bray_syntax::MemberAccessOperationSyntax>()
                    else {
                        failure = Some(BindingError::UnsupportedSyntax);

                        return SyntaxWalkControl::Stop;
                    };

                    self.bind_member_access(binder, &member, current)
                }
                SyntaxKind::ElementIndexOperation => self.bind_structured_with_operand(
                    binder,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::ElementIndex,
                    current,
                ),
                SyntaxKind::SliceIndexOperation => self.bind_structured_with_operand(
                    binder,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::SliceIndex,
                    current,
                ),
                SyntaxKind::NullablePropagationOperation => self.bind_structured_with_operand(
                    binder,
                    scope,
                    operation,
                    BoundStructuredExpressionKind::NullablePropagation,
                    current,
                ),
                SyntaxKind::TraitQualifiedMemberOperation => {
                    let Some(member) =
                        operation.cast::<bray_syntax::TraitQualifiedMemberOperationSyntax>()
                    else {
                        failure = Some(BindingError::UnsupportedSyntax);

                        return SyntaxWalkControl::Stop;
                    };

                    self.bind_trait_qualified_member(binder, &member, current)
                }
                _ => return SyntaxWalkControl::Continue,
            };

            match result {
                Ok(expression) => current = expression,
                Err(error) => {
                    failure = Some(error);

                    return SyntaxWalkControl::Stop;
                }
            }

            SyntaxWalkControl::Continue
        });

        failure.map_or(Ok(current), Err)
    }

    fn bind_call<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &CallOperationSyntax,
        callee: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let arguments = self.bind_arguments(binder, scope, &syntax.argument_list())?;

        let recovered = syntax.is_recovered()
            || arguments.iter().any(BoundArgument::is_recovered)
            || binder.expression_is_recovered(callee);

        let expression = if recovered {
            BoundExpression::ErrorCall(BoundErrorCallExpression::new(
                binder.source_origin(syntax),
                callee,
                arguments,
                self.error_type,
            ))
        } else {
            BoundExpression::Call(BoundCallExpression::new(
                binder.source_origin(syntax),
                callee,
                arguments,
                None,
                false,
            ))
        };

        self.push(binder, expression)
    }

    pub(in crate::binding::expression) fn bind_arguments<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &ArgumentListSyntax,
    ) -> BindingResult<Vec<BoundArgument>>
    where
        C: BinderFactContext + ?Sized,
    {
        let mut arguments = Vec::new();

        for argument in syntax.arguments() {
            let expression = self.bind_expression(binder, scope, Some(&argument.expression()))?;

            let name = argument
                .identifier_token()
                .and_then(|token| symbol_name(argument.source(), &token));

            arguments.push(BoundArgument::new(
                expression,
                name,
                argument.is_recovered(),
            ));
        }

        Ok(arguments)
    }

    fn bind_conversion<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        syntax: &ConversionOperationSyntax,
        operand: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let recovered = syntax.is_recovered() || binder.expression_is_recovered(operand);

        let target_syntax = SyntaxAnchor::from_node(&syntax.type_expression());

        let expression = if recovered {
            BoundExpression::ErrorConversion(BoundErrorConversionExpression::new(
                binder.source_origin(syntax),
                operand,
                target_syntax,
                None,
                self.error_type,
            ))
        } else {
            BoundExpression::Conversion(BoundConversionExpression::new(
                binder.source_origin(syntax),
                operand,
                target_syntax,
                None,
                None,
                false,
            ))
        };

        self.push(binder, expression)
    }
}
