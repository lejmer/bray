use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{CallableConditions, CallableTypeTemplate, TypeExpressionTemplate};
use bray_syntax::{
    LambdaExpressionSyntax, SourceSyntaxNode, SyntaxKind, SyntaxNodeView, SyntaxWalkControl,
    SyntaxWalkRoot, TypeExpressionSyntax, walk_direct_child_nodes,
};

use super::core::TypeExpressionBinder;
use crate::{BindingError, BindingQueryError, BindingQueryResult};

impl<Upstream> TypeExpressionBinder<'_, Upstream> {
    /// Binds the formal input types of one callable occurrence independently of its conditions.
    pub fn bind_callable_contract_input_signature(
        mut self,
        syntax: SyntaxNodeView<'_>,
    ) -> BindingQueryResult<DiagnosticResult<CallableTypeTemplate>, Upstream> {
        let signature = if let Some(lambda) = syntax.cast::<LambdaExpressionSyntax>() {
            let result = lambda
                .callable_result_clause()
                .map(|clause| clause.type_expression());

            self.bind_callable_type_surface(
                Some(&lambda.parameter_list()),
                result.as_ref(),
                Some(&lambda.callable_modifiers()),
                Some(&lambda.callable_directives()),
            )?
        } else if let Some(ty) = syntax.cast::<TypeExpressionSyntax>()
            && ty.func_keyword().is_some()
        {
            let result = ty
                .callable_result_clauses()
                .next()
                .map(|clause| clause.type_expression());

            self.bind_callable_type_surface(
                ty.parameter_lists().next().as_ref(),
                result.as_ref(),
                ty.callable_modifiers().next().as_ref(),
                ty.callable_directives().next().as_ref(),
            )?
        } else {
            return Err(BindingQueryError::Binding(BindingError::SyntaxContract(
                SyntaxAnchor::from_node(&syntax),
            )));
        };

        Ok(DiagnosticResult::new(signature, self.diagnostics))
    }

    pub(super) fn bind_occurrence_conditions(
        &mut self,
        mut signature: CallableTypeTemplate,
        syntax: &(impl SourceSyntaxNode + SyntaxWalkRoot),
    ) -> BindingQueryResult<TypeExpressionTemplate, Upstream> {
        let mut has_conditions = false;

        walk_direct_child_nodes(syntax, |child| {
            has_conditions = matches!(
                child.kind(),
                SyntaxKind::RequiresClause
                    | SyntaxKind::EnsuresClause
                    | SyntaxKind::WithClause
                    | SyntaxKind::WhenClause
                    | SyntaxKind::ExecutesClause
                    | SyntaxKind::UsesClause
            );

            if has_conditions {
                SyntaxWalkControl::Stop
            } else {
                SyntaxWalkControl::Continue
            }
        });

        if has_conditions {
            let (contract, diagnostics) = self
                .imports
                .callable_type_contract(self.owner, SyntaxAnchor::from_node(syntax), &signature)?
                .into_parts();

            self.diagnostics.add_range(diagnostics);

            // The signature retains the checked contract's immutable conditions and phase behavior.
            signature = signature
                .with_conditions(contract.conditions().clone())
                .with_phase_behaviors(contract.phase_behaviors().clone());
        }

        self.finish_callable_type(signature)
    }
}
