use super::item::{ExportDeclarationSyntax, UsingDeclarationSyntax};
use crate::SyntaxKind;
use crate::node::define_source_syntax_node;
use crate::{
    CallableContractDeclarationSyntax, CallableOverloadDeclarationSyntax,
    ConstantDeclarationSyntax, FunctionDeclarationSyntax, ImplementationOverloadDeclarationSyntax,
    InherentImplementationDeclarationSyntax, NamedTraitImplementationDeclarationSyntax,
    PredicateDeclarationSyntax, StaticDeclarationSyntax, StructDeclarationSyntax,
    TraitDeclarationSyntax, UnionDeclarationSyntax, UnnamedTraitImplementationDeclarationSyntax,
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
                /// Returns direct constant declaration children in source order.
                constant_declarations;
                /// Appends a constant declaration child in source order.
                push_constant_declaration;
                ty: ConstantDeclarationSyntax;
                kind: SyntaxKind::ConstantDeclaration;
            },
            {
                /// Returns direct static declaration children in source order.
                static_declarations;
                /// Appends a static declaration child in source order.
                push_static_declaration;
                ty: StaticDeclarationSyntax;
                kind: SyntaxKind::StaticDeclaration;
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
                /// Returns direct predicate declaration children in source order.
                predicate_declarations;
                /// Appends a predicate declaration child in source order.
                push_predicate_declaration;
                ty: PredicateDeclarationSyntax;
                kind: SyntaxKind::PredicateDeclaration;
            },
            {
                /// Returns direct callable contract declaration children in source order.
                callable_contract_declarations;
                /// Appends a callable contract declaration child in source order.
                push_callable_contract_declaration;
                ty: CallableContractDeclarationSyntax;
                kind: SyntaxKind::CallableContractDeclaration;
            },
            {
                /// Returns direct callable overload declaration children in source order.
                callable_overload_declarations;
                /// Appends a callable overload declaration child in source order.
                push_callable_overload_declaration;
                ty: CallableOverloadDeclarationSyntax;
                kind: SyntaxKind::CallableOverloadDeclaration;
            },
            {
                /// Returns direct implementation overload declaration children in source order.
                implementation_overload_declarations;
                /// Appends an implementation overload declaration child in source order.
                push_implementation_overload_declaration;
                ty: ImplementationOverloadDeclarationSyntax;
                kind: SyntaxKind::ImplementationOverloadDeclaration;
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
            },
            {
                /// Returns direct inherent implementation declaration children in source order.
                inherent_implementation_declarations;
                /// Appends an inherent implementation declaration child in source order.
                push_inherent_implementation_declaration;
                ty: InherentImplementationDeclarationSyntax;
                kind: SyntaxKind::InherentImplementationDeclaration;
            },
            {
                /// Returns direct unnamed trait implementation declaration children in source order.
                unnamed_trait_implementation_declarations;
                /// Appends an unnamed trait implementation declaration child in source order.
                push_unnamed_trait_implementation_declaration;
                ty: UnnamedTraitImplementationDeclarationSyntax;
                kind: SyntaxKind::UnnamedTraitImplementationDeclaration;
            },
            {
                /// Returns direct named trait implementation declaration children in source order.
                named_trait_implementation_declarations;
                /// Appends a named trait implementation declaration child in source order.
                push_named_trait_implementation_declaration;
                ty: NamedTraitImplementationDeclarationSyntax;
                kind: SyntaxKind::NamedTraitImplementationDeclaration;
            }
        ],
    }
}
