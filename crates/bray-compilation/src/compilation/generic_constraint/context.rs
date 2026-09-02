use bray_symbols::TypeExpressionTemplate;

use crate::compilation::SemanticQueryContext;
use crate::fact::CompilationFactKey;

pub(super) fn type_template_context(template: &TypeExpressionTemplate) -> SemanticQueryContext {
    if let Some(occurrence) = template.constant_expressions().first() {
        return SemanticQueryContext::Symbol(occurrence.key().owner());
    }

    match template {
        TypeExpressionTemplate::Named { definition, .. } => {
            SemanticQueryContext::Symbol(match definition {
                bray_symbols::NamedTypeSymbolId::Struct(definition) => (*definition).into(),
                bray_symbols::NamedTypeSymbolId::Union(definition) => (*definition).into(),
            })
        }
        TypeExpressionTemplate::CallableContract { definition, .. } => {
            SemanticQueryContext::Symbol((*definition).into())
        }
        TypeExpressionTemplate::TypeValuedMemberProjection { member, .. } => {
            SemanticQueryContext::Symbol((*member).into())
        }
        TypeExpressionTemplate::TraitView(application) => {
            SemanticQueryContext::Symbol(application.definition().into())
        }
        TypeExpressionTemplate::Resolved(_)
        | TypeExpressionTemplate::Tuple(_)
        | TypeExpressionTemplate::Array { .. }
        | TypeExpressionTemplate::FlexibleArray(_)
        | TypeExpressionTemplate::Slice(_)
        | TypeExpressionTemplate::Nullable(_)
        | TypeExpressionTemplate::Borrow { .. }
        | TypeExpressionTemplate::OwnedIndirection { .. }
        | TypeExpressionTemplate::Callable(_) => {
            SemanticQueryContext::Fact(CompilationFactKey::CheckDiagnostics)
        }
    }
}
