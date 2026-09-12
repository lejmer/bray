use bray_symbols::ExecutionProperty;
use bray_syntax::{
    ExecutesClauseSyntax, SyntaxNodeView, SyntaxWalkControl, walk_direct_child_nodes,
};

/// Reads directly declared execution properties without certifying the callable.
/// Guarded groups require their own input-domain checking and are not unconditional properties.
pub fn callable_execution_properties(root: SyntaxNodeView<'_>) -> Vec<ExecutionProperty> {
    let mut properties = Vec::new();
    let mut has_requirements = false;

    walk_direct_child_nodes(&root, |child| {
        has_requirements |= child.kind() == bray_syntax::SyntaxKind::RequiresClause;

        if let Some(clause) = child.cast::<ExecutesClauseSyntax>() {
            for property in clause.properties() {
                if let Some(name) = property.identifier_token().text(child.source().text())
                    && let Some(property) = ExecutionProperty::from_name(name)
                {
                    properties.push(property);
                }
            }
        }

        SyntaxWalkControl::Continue
    });

    // A callable type without precondition evidence cannot expose a restricted promise.
    if has_requirements {
        Vec::new()
    } else {
        properties
    }
}
