use crate::node::define_source_syntax_node;
use crate::{GenericArgumentListSyntax, PathSyntax, SyntaxKind};

define_source_syntax_node! {
    /// Trait application used by type expressions and implementation declarations.
    pub struct TraitApplicationSyntax {
        builder: TraitApplicationSyntaxBuilder,
        kind: SyntaxKind::TraitApplication,
        source_slot: "trait_application.source",
        node_name: "trait application",
        range_description: "trait-application",
        debug_name: "TraitApplicationSyntax",
        builder_debug_name: "TraitApplicationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the trait path child.
                path;
                /// Appends the trait path child.
                push_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            }
        ],
        repeated_children: [
            {
                /// Returns generic argument lists in source order.
                generic_argument_lists;
                /// Appends a generic argument list child.
                push_generic_argument_list;
                ty: GenericArgumentListSyntax;
                kind: SyntaxKind::GenericArgumentList;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{identifier_path, snapshot as test_snapshot};
    use crate::{SyntaxKind, SyntaxNode, SyntaxText, TraitApplicationSyntax};

    #[test]
    fn trait_applications_store_path_children() {
        let snapshot = test_snapshot("syntax-trait-application-test", "Display");
        let mut builder = TraitApplicationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_path(identifier_path(snapshot, 0, 7));

        let application = builder.build();

        assert_eq!(application.kind(), SyntaxKind::TraitApplication);
        assert_eq!(application.full_text(), "Display");
        assert_eq!(application.path().full_text(), "Display");
    }
}
