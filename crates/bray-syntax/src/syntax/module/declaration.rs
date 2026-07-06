use super::body::ModuleBodySyntax;
use super::modifiers::ModuleModifiersSyntax;
use crate::SyntaxKind;
use crate::node::define_source_syntax_node;
use crate::syntax::path::PathSyntax;

define_source_syntax_node! {
    /// Module declaration that owns loose source-unit module items.
    pub struct SourceUnitModuleDeclarationSyntax {
        builder: SourceUnitModuleDeclarationSyntaxBuilder,
        kind: SyntaxKind::SourceUnitModuleDeclaration,
        source_slot: "source_unit_module_declaration.source",
        node_name: "source-unit module declaration",
        range_description: "source-unit module declaration",
        debug_name: "SourceUnitModuleDeclarationSyntax",
        builder_debug_name: "SourceUnitModuleDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `module` keyword token.
                module_keyword;
                /// Appends the module keyword token.
                push_module_keyword;
                kind: SyntaxKind::ModuleKeyword;
                slot: "source_unit_module_declaration.module_keyword";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "source_unit_module_declaration.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the module-modifiers child.
                module_modifiers;
                /// Appends the module-modifiers child.
                push_module_modifiers;
                ty: ModuleModifiersSyntax;
                kind: SyntaxKind::ModuleModifiers;
            },
            {
                /// Returns the required module path child.
                module_path;
                /// Appends the module path child.
                push_module_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Braced top-level module declaration.
    pub struct BlockModuleDeclarationSyntax {
        builder: BlockModuleDeclarationSyntaxBuilder,
        kind: SyntaxKind::BlockModuleDeclaration,
        source_slot: "block_module_declaration.source",
        node_name: "block module declaration",
        range_description: "block module declaration",
        debug_name: "BlockModuleDeclarationSyntax",
        builder_debug_name: "BlockModuleDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `module` keyword token.
                module_keyword;
                /// Appends the module keyword token.
                push_module_keyword;
                kind: SyntaxKind::ModuleKeyword;
                slot: "block_module_declaration.module_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the module-modifiers child.
                module_modifiers;
                /// Appends the module-modifiers child.
                push_module_modifiers;
                ty: ModuleModifiersSyntax;
                kind: SyntaxKind::ModuleModifiers;
            },
            {
                /// Returns the required module path child.
                module_path;
                /// Appends the module path child.
                push_module_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            },
            {
                /// Returns the required module-body child.
                module_body;
                /// Appends the module-body child.
                push_module_body;
                ty: ModuleBodySyntax;
                kind: SyntaxKind::ModuleBody;
            }
        ],
    }
}
