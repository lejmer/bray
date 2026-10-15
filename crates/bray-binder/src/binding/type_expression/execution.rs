use bray_symbols::ExecutionProperty;
use bray_syntax::{
    ExecutesClauseSyntax, SyntaxNodeView, SyntaxWalkControl, walk_direct_child_nodes,
};

/// Reads directly declared execution properties without certifying the callable.
/// Properties apply on the callable's required input domain. Guarded groups need
/// separate domain proofs and are not part of this directly declared surface.
pub fn callable_execution_properties(root: SyntaxNodeView<'_>) -> Vec<ExecutionProperty> {
    let mut properties = Vec::new();

    walk_direct_child_nodes(&root, |child| {
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

    // The callable type carries its requirements alongside these promises. Calls and
    // conversions must establish or preserve those requirements before using a promise.
    properties
}
