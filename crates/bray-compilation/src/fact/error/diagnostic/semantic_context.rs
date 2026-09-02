use bray_diagnostics::DiagnosticEvaluationFailureDetail;

use super::diagnostic_context::{identity_field, push_symbol, text_field};

pub(crate) fn diagnostic_semantic_context_failure(
    error: &bray_binder::SemanticUnitContextError,
) -> DiagnosticEvaluationFailureDetail {
    use bray_binder::SemanticUnitContextError as Error;

    let mut context = vec![identity_field("unit", error.unit())];

    let reason = match error {
        Error::MissingAnonymousCallable { callable, .. } => {
            context.push(identity_field("callable", callable));

            "semantic_context_missing_anonymous_callable"
        }
        Error::RootKindMismatch { root, .. } => {
            let root_kind = match root {
                bray_bound_tree::BoundUnitRoot::CallableBody { .. } => "callable_body",
                bray_bound_tree::BoundUnitRoot::AnonymousCallable { .. } => "anonymous_callable",
                bray_bound_tree::BoundUnitRoot::Expression(_) => "expression",
                bray_bound_tree::BoundUnitRoot::ExpressionSequence(_) => "expression_sequence",
            };

            context.push(text_field("root_kind", root_kind));
            context.push(identity_field("root", root));

            "semantic_context_root_kind_mismatch"
        }
        Error::MissingOwner { .. } => "semantic_context_missing_owner",
        Error::MissingDeclaration { owner, .. } => {
            push_symbol(&mut context, "owner_kind", "owner", *owner);

            "semantic_context_missing_declaration"
        }
        Error::InvalidContractClauseKind { actual, .. } => {
            context.push(text_field("actual", actual.as_str()));

            "semantic_context_invalid_contract_clause_kind"
        }
    };

    DiagnosticEvaluationFailureDetail::new(reason, context)
}
