use crate::SyntaxKind;
use crate::node::define_source_syntax_node;

define_source_syntax_node! {
    /// Body of a braced module declaration.
    pub struct ModuleBodySyntax {
        builder: ModuleBodySyntaxBuilder,
        kind: SyntaxKind::ModuleBody,
        source_slot: "module_body.source",
        node_name: "module body",
        range_description: "module-body",
        debug_name: "ModuleBodySyntax",
        builder_debug_name: "ModuleBodySyntaxBuilder",
        // TODO(syntax): Replace skipped syntax with typed module body items.
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "module_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "module_body.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}
