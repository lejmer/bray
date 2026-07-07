use super::item::{ExportDeclarationSyntax, UsingDeclarationSyntax};
use crate::SyntaxKind;
use crate::node::define_source_syntax_node;
use crate::{
    FunctionDeclarationSyntax, StructDeclarationSyntax, TraitDeclarationSyntax,
    UnionDeclarationSyntax,
};

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
        repeated_children: [
            {
                /// Returns direct `using` declaration children in source order.
                using_declarations;
                /// Appends a `using` declaration child in source order.
                push_using_declaration;
                ty: UsingDeclarationSyntax;
                kind: SyntaxKind::UsingDeclaration;
            },
            {
                /// Returns direct `export` declaration children in source order.
                export_declarations;
                /// Appends an `export` declaration child in source order.
                push_export_declaration;
                ty: ExportDeclarationSyntax;
                kind: SyntaxKind::ExportDeclaration;
            },
            {
                /// Returns direct function declaration children in source order.
                function_declarations;
                /// Appends a function declaration child in source order.
                push_function_declaration;
                ty: FunctionDeclarationSyntax;
                kind: SyntaxKind::FunctionDeclaration;
            },
            {
                /// Returns direct struct declaration children in source order.
                struct_declarations;
                /// Appends a struct declaration child in source order.
                push_struct_declaration;
                ty: StructDeclarationSyntax;
                kind: SyntaxKind::StructDeclaration;
            },
            {
                /// Returns direct union declaration children in source order.
                union_declarations;
                /// Appends a union declaration child in source order.
                push_union_declaration;
                ty: UnionDeclarationSyntax;
                kind: SyntaxKind::UnionDeclaration;
            },
            {
                /// Returns direct trait declaration children in source order.
                trait_declarations;
                /// Appends a trait declaration child in source order.
                push_trait_declaration;
                ty: TraitDeclarationSyntax;
                kind: SyntaxKind::TraitDeclaration;
            }
        ],
    }
}
