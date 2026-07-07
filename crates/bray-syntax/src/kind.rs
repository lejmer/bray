/// Stable syntax vocabulary for Bray syntax nodes, tokens, and trivia.
///
/// Syntax kinds are locale-neutral and source-shaped. They describe lexical
/// spellings and syntax tree nodes, not semantic meanings.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum SyntaxKind {
    /// Root syntax node for a compiler compilation unit.
    CompilationUnit,
    /// Root syntax node for one parsed source file.
    SourceUnit,
    /// Module declaration that owns all loose module items in a source unit.
    SourceUnitModuleDeclaration,
    /// Braced top-level module declaration.
    BlockModuleDeclaration,
    /// Module directives in source order.
    ModuleDirectives,
    /// `@target(...)` module directive.
    TargetDirective,
    /// `@test` module directive.
    TestDirective,
    /// `@link(...)` module directive.
    LinkDirective,
    /// Parenthesized directive arguments.
    DirectiveArgumentList,
    /// Optional module modifiers in source order.
    ModuleModifiers,
    /// Braced module declaration body.
    ModuleBody,
    /// Dotted identifier path syntax node.
    Path,
    /// Concrete comma-like identifier list syntax node.
    IdentifierList,
    /// Item node inside an identifier list.
    IdentifierListItem,
    /// Recovery node containing present tokens skipped by the parser.
    SkippedSyntax,

    /// End-of-file marker token.
    EndOfFileToken,
    /// Token representing source text that the lexer could not classify.
    InvalidToken,
    IdentifierToken,
    TupleElementIndexToken,
    DecimalIntegerLiteralToken,
    BinaryIntegerLiteralToken,
    HexadecimalIntegerLiteralToken,
    RealLiteralToken,
    ImaginaryLiteralToken,
    CharacterLiteralToken,
    StringLiteralToken,

    AllKeyword,
    AnyKeyword,
    AsKeyword,
    AssertKeyword,
    AsyncKeyword,
    AwaitKeyword,
    BoxKeyword,
    BreakKeyword,
    CallableKeyword,
    CaseKeyword,
    CatchKeyword,
    ConstKeyword,
    ConstructKeyword,
    ConsumeKeyword,
    ContinueKeyword,
    DestructKeyword,
    DetachedKeyword,
    EachKeyword,
    ElseKeyword,
    EnsuresKeyword,
    EnterKeyword,
    ExitKeyword,
    ExportKeyword,
    ExternKeyword,
    FalseKeyword,
    FinalizeKeyword,
    ForKeyword,
    FuncKeyword,
    IfKeyword,
    ImplKeyword,
    InKeyword,
    InternalKeyword,
    LambdaKeyword,
    LetKeyword,
    LoopKeyword,
    MatchKeyword,
    ModuleKeyword,
    MoveKeyword,
    MutKeyword,
    NoneKeyword,
    OverloadKeyword,
    PanicKeyword,
    PosKeyword,
    PredicateKeyword,
    PublicKeyword,
    RequiresKeyword,
    ReturnKeyword,
    SelfValueKeyword,
    SelfTypeKeyword,
    SpawnKeyword,
    StaticKeyword,
    StructKeyword,
    ThreadKeyword,
    TraitKeyword,
    TrustedKeyword,
    TrueKeyword,
    TryKeyword,
    TypeKeyword,
    UnionKeyword,
    UnitKeyword,
    UsingKeyword,
    UsesKeyword,
    ViewKeyword,
    WhenKeyword,
    WhileKeyword,
    WithKeyword,
    YieldKeyword,

    ArrowToken,
    EqualsEqualsToken,
    BangEqualsToken,
    LessEqualsToken,
    GreaterEqualsToken,
    AmpersandAmpersandToken,
    PipePipeToken,
    LessLessToken,
    GreaterGreaterToken,
    StarStarToken,
    DotDotToken,
    OpenParenToken,
    CloseParenToken,
    OpenBraceToken,
    CloseBraceToken,
    OpenBracketToken,
    CloseBracketToken,
    CommaToken,
    SemicolonToken,
    ColonToken,
    DotToken,
    QuestionToken,
    EqualsToken,
    PlusToken,
    MinusToken,
    StarToken,
    SlashToken,
    PercentToken,
    AtToken,
    AmpersandToken,
    PipeToken,
    CaretToken,
    TildeToken,
    BangToken,
    LessToken,
    GreaterToken,
    UnderscoreToken,

    WhitespaceTrivia,
    LineCommentTrivia,
    BlockCommentTrivia,
    DocumentationLineCommentTrivia,
    DocumentationBlockCommentTrivia,
}

impl SyntaxKind {
    /// Returns whether this kind represents a syntax tree node.
    pub const fn is_node(self) -> bool {
        matches!(
            self,
            Self::SourceUnit
                | Self::CompilationUnit
                | Self::SourceUnitModuleDeclaration
                | Self::BlockModuleDeclaration
                | Self::ModuleDirectives
                | Self::TargetDirective
                | Self::TestDirective
                | Self::LinkDirective
                | Self::DirectiveArgumentList
                | Self::ModuleModifiers
                | Self::ModuleBody
                | Self::Path
                | Self::IdentifierList
                | Self::IdentifierListItem
                | Self::SkippedSyntax
        )
    }

    /// Returns whether this kind represents syntax trivia.
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::WhitespaceTrivia
                | Self::LineCommentTrivia
                | Self::BlockCommentTrivia
                | Self::DocumentationLineCommentTrivia
                | Self::DocumentationBlockCommentTrivia
        )
    }

    /// Returns whether this kind represents a lexical token.
    pub const fn is_token(self) -> bool {
        !self.is_node() && !self.is_trivia()
    }

    /// Returns whether this kind represents a reserved keyword token.
    pub const fn is_keyword(self) -> bool {
        matches!(
            self,
            Self::AllKeyword
                | Self::AnyKeyword
                | Self::AsKeyword
                | Self::AssertKeyword
                | Self::AsyncKeyword
                | Self::AwaitKeyword
                | Self::BoxKeyword
                | Self::BreakKeyword
                | Self::CallableKeyword
                | Self::CaseKeyword
                | Self::CatchKeyword
                | Self::ConstKeyword
                | Self::ConstructKeyword
                | Self::ConsumeKeyword
                | Self::ContinueKeyword
                | Self::DestructKeyword
                | Self::DetachedKeyword
                | Self::EachKeyword
                | Self::ElseKeyword
                | Self::EnsuresKeyword
                | Self::EnterKeyword
                | Self::ExitKeyword
                | Self::ExportKeyword
                | Self::ExternKeyword
                | Self::FalseKeyword
                | Self::FinalizeKeyword
                | Self::ForKeyword
                | Self::FuncKeyword
                | Self::IfKeyword
                | Self::ImplKeyword
                | Self::InKeyword
                | Self::InternalKeyword
                | Self::LambdaKeyword
                | Self::LetKeyword
                | Self::LoopKeyword
                | Self::MatchKeyword
                | Self::ModuleKeyword
                | Self::MoveKeyword
                | Self::MutKeyword
                | Self::NoneKeyword
                | Self::OverloadKeyword
                | Self::PanicKeyword
                | Self::PosKeyword
                | Self::PredicateKeyword
                | Self::PublicKeyword
                | Self::RequiresKeyword
                | Self::ReturnKeyword
                | Self::SelfValueKeyword
                | Self::SelfTypeKeyword
                | Self::SpawnKeyword
                | Self::StaticKeyword
                | Self::StructKeyword
                | Self::ThreadKeyword
                | Self::TraitKeyword
                | Self::TrustedKeyword
                | Self::TrueKeyword
                | Self::TryKeyword
                | Self::TypeKeyword
                | Self::UnionKeyword
                | Self::UnitKeyword
                | Self::UsingKeyword
                | Self::UsesKeyword
                | Self::ViewKeyword
                | Self::WhenKeyword
                | Self::WhileKeyword
                | Self::WithKeyword
                | Self::YieldKeyword
        )
    }

    /// Returns whether this kind represents a literal token.
    pub const fn is_literal(self) -> bool {
        matches!(
            self,
            Self::DecimalIntegerLiteralToken
                | Self::BinaryIntegerLiteralToken
                | Self::HexadecimalIntegerLiteralToken
                | Self::RealLiteralToken
                | Self::ImaginaryLiteralToken
                | Self::CharacterLiteralToken
                | Self::StringLiteralToken
        )
    }

    /// Returns the stable machine key for this syntax kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceUnit => "source_unit",
            Self::CompilationUnit => "compilation_unit",
            Self::SourceUnitModuleDeclaration => "source_unit_module_declaration",
            Self::BlockModuleDeclaration => "block_module_declaration",
            Self::ModuleDirectives => "module_directives",
            Self::TargetDirective => "target_directive",
            Self::TestDirective => "test_directive",
            Self::LinkDirective => "link_directive",
            Self::DirectiveArgumentList => "directive_argument_list",
            Self::ModuleModifiers => "module_modifiers",
            Self::ModuleBody => "module_body",
            Self::Path => "path",
            Self::IdentifierList => "identifier_list",
            Self::IdentifierListItem => "identifier_list_item",
            Self::SkippedSyntax => "skipped_syntax",
            Self::EndOfFileToken => "end_of_file_token",
            Self::InvalidToken => "invalid_token",
            Self::IdentifierToken => "identifier_token",
            Self::TupleElementIndexToken => "tuple_element_index_token",
            Self::DecimalIntegerLiteralToken => "decimal_integer_literal_token",
            Self::BinaryIntegerLiteralToken => "binary_integer_literal_token",
            Self::HexadecimalIntegerLiteralToken => "hexadecimal_integer_literal_token",
            Self::RealLiteralToken => "real_literal_token",
            Self::ImaginaryLiteralToken => "imaginary_literal_token",
            Self::CharacterLiteralToken => "character_literal_token",
            Self::StringLiteralToken => "string_literal_token",
            Self::AllKeyword => "all_keyword",
            Self::AnyKeyword => "any_keyword",
            Self::AsKeyword => "as_keyword",
            Self::AssertKeyword => "assert_keyword",
            Self::AsyncKeyword => "async_keyword",
            Self::AwaitKeyword => "await_keyword",
            Self::BoxKeyword => "box_keyword",
            Self::BreakKeyword => "break_keyword",
            Self::CallableKeyword => "callable_keyword",
            Self::CaseKeyword => "case_keyword",
            Self::CatchKeyword => "catch_keyword",
            Self::ConstKeyword => "const_keyword",
            Self::ConstructKeyword => "construct_keyword",
            Self::ConsumeKeyword => "consume_keyword",
            Self::ContinueKeyword => "continue_keyword",
            Self::DestructKeyword => "destruct_keyword",
            Self::DetachedKeyword => "detached_keyword",
            Self::EachKeyword => "each_keyword",
            Self::ElseKeyword => "else_keyword",
            Self::EnsuresKeyword => "ensures_keyword",
            Self::EnterKeyword => "enter_keyword",
            Self::ExitKeyword => "exit_keyword",
            Self::ExportKeyword => "export_keyword",
            Self::ExternKeyword => "extern_keyword",
            Self::FalseKeyword => "false_keyword",
            Self::FinalizeKeyword => "finalize_keyword",
            Self::ForKeyword => "for_keyword",
            Self::FuncKeyword => "func_keyword",
            Self::IfKeyword => "if_keyword",
            Self::ImplKeyword => "impl_keyword",
            Self::InKeyword => "in_keyword",
            Self::InternalKeyword => "internal_keyword",
            Self::LambdaKeyword => "lambda_keyword",
            Self::LetKeyword => "let_keyword",
            Self::LoopKeyword => "loop_keyword",
            Self::MatchKeyword => "match_keyword",
            Self::ModuleKeyword => "module_keyword",
            Self::MoveKeyword => "move_keyword",
            Self::MutKeyword => "mut_keyword",
            Self::NoneKeyword => "none_keyword",
            Self::OverloadKeyword => "overload_keyword",
            Self::PanicKeyword => "panic_keyword",
            Self::PosKeyword => "pos_keyword",
            Self::PredicateKeyword => "predicate_keyword",
            Self::PublicKeyword => "public_keyword",
            Self::RequiresKeyword => "requires_keyword",
            Self::ReturnKeyword => "return_keyword",
            Self::SelfValueKeyword => "self_value_keyword",
            Self::SelfTypeKeyword => "self_type_keyword",
            Self::SpawnKeyword => "spawn_keyword",
            Self::StaticKeyword => "static_keyword",
            Self::StructKeyword => "struct_keyword",
            Self::ThreadKeyword => "thread_keyword",
            Self::TraitKeyword => "trait_keyword",
            Self::TrustedKeyword => "trusted_keyword",
            Self::TrueKeyword => "true_keyword",
            Self::TryKeyword => "try_keyword",
            Self::TypeKeyword => "type_keyword",
            Self::UnionKeyword => "union_keyword",
            Self::UnitKeyword => "unit_keyword",
            Self::UsingKeyword => "using_keyword",
            Self::UsesKeyword => "uses_keyword",
            Self::ViewKeyword => "view_keyword",
            Self::WhenKeyword => "when_keyword",
            Self::WhileKeyword => "while_keyword",
            Self::WithKeyword => "with_keyword",
            Self::YieldKeyword => "yield_keyword",
            Self::ArrowToken => "arrow_token",
            Self::EqualsEqualsToken => "equals_equals_token",
            Self::BangEqualsToken => "bang_equals_token",
            Self::LessEqualsToken => "less_equals_token",
            Self::GreaterEqualsToken => "greater_equals_token",
            Self::AmpersandAmpersandToken => "ampersand_ampersand_token",
            Self::PipePipeToken => "pipe_pipe_token",
            Self::LessLessToken => "less_less_token",
            Self::GreaterGreaterToken => "greater_greater_token",
            Self::StarStarToken => "star_star_token",
            Self::DotDotToken => "dot_dot_token",
            Self::OpenParenToken => "open_paren_token",
            Self::CloseParenToken => "close_paren_token",
            Self::OpenBraceToken => "open_brace_token",
            Self::CloseBraceToken => "close_brace_token",
            Self::OpenBracketToken => "open_bracket_token",
            Self::CloseBracketToken => "close_bracket_token",
            Self::CommaToken => "comma_token",
            Self::SemicolonToken => "semicolon_token",
            Self::ColonToken => "colon_token",
            Self::DotToken => "dot_token",
            Self::QuestionToken => "question_token",
            Self::EqualsToken => "equals_token",
            Self::PlusToken => "plus_token",
            Self::MinusToken => "minus_token",
            Self::StarToken => "star_token",
            Self::SlashToken => "slash_token",
            Self::PercentToken => "percent_token",
            Self::AtToken => "at_token",
            Self::AmpersandToken => "ampersand_token",
            Self::PipeToken => "pipe_token",
            Self::CaretToken => "caret_token",
            Self::TildeToken => "tilde_token",
            Self::BangToken => "bang_token",
            Self::LessToken => "less_token",
            Self::GreaterToken => "greater_token",
            Self::UnderscoreToken => "underscore_token",
            Self::WhitespaceTrivia => "whitespace_trivia",
            Self::LineCommentTrivia => "line_comment_trivia",
            Self::BlockCommentTrivia => "block_comment_trivia",
            Self::DocumentationLineCommentTrivia => "documentation_line_comment_trivia",
            Self::DocumentationBlockCommentTrivia => "documentation_block_comment_trivia",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SyntaxKind;

    #[test]
    fn syntax_kinds_classify_nodes_tokens_and_trivia() {
        assert!(SyntaxKind::SourceUnit.is_node());
        assert!(!SyntaxKind::SourceUnit.is_token());
        assert!(SyntaxKind::SourceUnitModuleDeclaration.is_node());
        assert!(SyntaxKind::BlockModuleDeclaration.is_node());
        assert!(SyntaxKind::ModuleDirectives.is_node());
        assert!(SyntaxKind::TargetDirective.is_node());
        assert!(SyntaxKind::TestDirective.is_node());
        assert!(SyntaxKind::LinkDirective.is_node());
        assert!(SyntaxKind::DirectiveArgumentList.is_node());
        assert!(SyntaxKind::ModuleModifiers.is_node());
        assert!(SyntaxKind::ModuleBody.is_node());
        assert!(SyntaxKind::Path.is_node());
        assert!(SyntaxKind::IdentifierList.is_node());
        assert!(SyntaxKind::IdentifierListItem.is_node());
        assert!(SyntaxKind::SkippedSyntax.is_node());
        assert!(!SyntaxKind::SkippedSyntax.is_token());

        assert!(SyntaxKind::IdentifierToken.is_token());
        assert!(!SyntaxKind::IdentifierToken.is_trivia());

        assert!(SyntaxKind::WhitespaceTrivia.is_trivia());
        assert!(!SyntaxKind::WhitespaceTrivia.is_token());
    }

    #[test]
    fn syntax_kinds_classify_keywords_and_literals() {
        assert!(SyntaxKind::FuncKeyword.is_keyword());
        assert!(SyntaxKind::SelfValueKeyword.is_keyword());
        assert!(SyntaxKind::SelfTypeKeyword.is_keyword());
        assert!(!SyntaxKind::IdentifierToken.is_keyword());

        assert!(SyntaxKind::RealLiteralToken.is_literal());
        assert!(SyntaxKind::ImaginaryLiteralToken.is_literal());
        assert!(!SyntaxKind::TupleElementIndexToken.is_literal());
    }

    #[test]
    fn syntax_kinds_expose_stable_machine_keys() {
        assert_eq!(SyntaxKind::SourceUnit.as_str(), "source_unit");

        assert_eq!(
            SyntaxKind::SourceUnitModuleDeclaration.as_str(),
            "source_unit_module_declaration"
        );

        assert_eq!(
            SyntaxKind::BlockModuleDeclaration.as_str(),
            "block_module_declaration"
        );

        assert_eq!(SyntaxKind::ModuleDirectives.as_str(), "module_directives");
        assert_eq!(SyntaxKind::TargetDirective.as_str(), "target_directive");
        assert_eq!(SyntaxKind::TestDirective.as_str(), "test_directive");
        assert_eq!(SyntaxKind::LinkDirective.as_str(), "link_directive");

        assert_eq!(
            SyntaxKind::DirectiveArgumentList.as_str(),
            "directive_argument_list"
        );

        assert_eq!(SyntaxKind::ModuleModifiers.as_str(), "module_modifiers");
        assert_eq!(SyntaxKind::ModuleBody.as_str(), "module_body");
        assert_eq!(SyntaxKind::Path.as_str(), "path");
        assert_eq!(SyntaxKind::IdentifierList.as_str(), "identifier_list");

        assert_eq!(
            SyntaxKind::IdentifierListItem.as_str(),
            "identifier_list_item"
        );

        assert_eq!(SyntaxKind::SkippedSyntax.as_str(), "skipped_syntax");
        assert_eq!(SyntaxKind::EndOfFileToken.as_str(), "end_of_file_token");
        assert_eq!(SyntaxKind::SelfTypeKeyword.as_str(), "self_type_keyword");
        assert_eq!(SyntaxKind::ArrowToken.as_str(), "arrow_token");

        assert_eq!(
            SyntaxKind::DocumentationBlockCommentTrivia.as_str(),
            "documentation_block_comment_trivia"
        );
    }
}
