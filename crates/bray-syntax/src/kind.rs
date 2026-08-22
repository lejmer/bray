// rust-style: allow(module-too-large, reason = "syntax kinds and their exhaustive classifications form one cohesive language vocabulary")

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
    /// `@abi(...)` callable directive.
    AbiDirective,
    /// `@symbol(...)` function directive.
    SymbolDirective,
    /// `@entrypoint` function directive.
    EntrypointDirective,
    /// Parenthesized directive arguments.
    DirectiveArgumentList,
    /// Single directive argument.
    DirectiveArgument,
    /// Optional module modifiers in source order.
    ModuleModifiers,
    /// Braced module declaration body.
    ModuleBody,
    /// Module item that imports names into the current module.
    UsingDeclaration,
    /// Module item that exports a path from the current module.
    ExportDeclaration,
    /// Ordinary constant declaration.
    ConstantDeclaration,
    /// Optional constant declaration modifiers in source order.
    ConstantModifiers,
    /// Module-level static storage declaration.
    StaticDeclaration,
    /// Static declaration directives in source order.
    StaticDirectives,
    /// `@thread_local` static directive.
    ThreadLocalDirective,
    /// Optional static declaration modifiers in source order.
    StaticDeclarationModifiers,
    /// Module-level function declaration.
    FunctionDeclaration,
    /// Function directives in source order.
    FunctionDirectives,
    /// Optional function modifiers in source order.
    FunctionModifiers,
    /// Generic parameter list including delimiters.
    GenericParameterList,
    /// Generic type parameter.
    GenericTypeParameter,
    /// Generic const parameter.
    GenericConstParameter,
    /// Generic argument list including delimiters.
    GenericArgumentList,
    /// Generic argument item.
    GenericArgument,
    /// Type-form argument list including delimiters.
    TypeFormArgumentList,
    /// Type-form argument item.
    TypeFormArgument,
    /// Module-level predicate declaration.
    PredicateDeclaration,
    /// Optional predicate modifiers in source order.
    PredicateModifiers,
    /// Predicate parameter list including delimiters.
    PredicateParameterList,
    /// Predicate parameter item.
    PredicateParameter,
    /// Module-level named callable contract declaration.
    CallableContractDeclaration,
    /// Optional callable contract modifiers in source order.
    CallableContractModifiers,
    /// `requires(...)` callable contract clause.
    RequiresClause,
    /// `ensures(...)` callable contract clause.
    EnsuresClause,
    /// `with(...)` static constraint clause.
    WithClause,
    /// `uses(...)` trusted capability clause.
    UsesClause,
    /// Optional overload declaration modifiers in source order.
    OverloadModifiers,
    /// Callable overload declaration.
    CallableOverloadDeclaration,
    /// Implementation overload declaration.
    ImplementationOverloadDeclaration,
    /// Subject named by an implementation overload declaration.
    ImplementationOverloadSubject,
    /// Braced overload arm list.
    OverloadArmList,
    /// Single overload arm.
    OverloadArm,
    /// Module-level struct declaration.
    StructDeclaration,
    /// Type directives in source order.
    TypeDirectives,
    /// `@layout(...)` type directive.
    LayoutDirective,
    /// `@copy` type directive.
    CopyDirective,
    /// `@tag(...)` union variant directive.
    TagDirective,
    /// Optional type declaration modifiers in source order.
    TypeModifiers,
    /// Braced struct declaration body.
    StructBody,
    /// Optional struct field modifiers in source order.
    FieldModifiers,
    /// Struct field declaration.
    StructFieldDeclaration,
    /// Module-level union declaration.
    UnionDeclaration,
    /// Braced union declaration body.
    UnionBody,
    /// Union variant directives in source order.
    VariantDirectives,
    /// Union variant declaration.
    UnionVariantDeclaration,
    /// Parenthesized union variant payload.
    UnionVariantPayload,
    /// Union variant payload field.
    UnionPayloadField,
    /// Optional union payload field modifiers in source order.
    PayloadFieldModifiers,
    /// Module-level trait declaration.
    TraitDeclaration,
    /// Optional trait modifiers in source order.
    TraitModifiers,
    /// Braced trait declaration body.
    TraitBody,
    /// Trait constant member declaration.
    TraitConstantMemberDeclaration,
    /// Trait type-valued member declaration.
    TraitTypeMemberDeclaration,
    /// Optional trait predicate member modifiers in source order.
    TraitPredicateMemberModifiers,
    /// Trait predicate member declaration.
    TraitPredicateMemberDeclaration,
    /// Optional trait callable member modifiers in source order.
    TraitCallableMemberModifiers,
    /// Trait callable member declaration.
    TraitCallableMemberDeclaration,
    /// Subject type named by an implementation declaration.
    ImplementationSubject,
    /// Trait application named by a trait implementation declaration.
    TraitApplication,
    /// Module-level inherent implementation declaration.
    InherentImplementationDeclaration,
    /// Braced implementation body.
    ImplementationBody,
    /// Type-valued member binding inside an implementation body.
    ImplementationTypeMemberBinding,
    /// Module-level unnamed trait implementation declaration.
    UnnamedTraitImplementationDeclaration,
    /// Module-level named trait implementation declaration.
    NamedTraitImplementationDeclaration,
    /// Optional constructor member modifiers in source order.
    ConstructorMemberModifiers,
    /// Type constructor member declaration.
    TypeConstructorMemberDeclaration,
    /// Optional async-capable lifecycle member modifiers in source order.
    AsyncCapableLifecycleMemberModifiers,
    /// Optional scope-enter member modifiers in source order.
    ScopeEnterMemberModifiers,
    /// Optional synchronous lifecycle member modifiers in source order.
    SyncLifecycleMemberModifiers,
    /// Finalizer lifecycle member declaration.
    FinalizerMemberDeclaration,
    /// Destructor lifecycle member declaration.
    DestructorMemberDeclaration,
    /// Scope-enter lifecycle member declaration.
    ScopeEnterMemberDeclaration,
    /// Scope-exit lifecycle member declaration.
    ScopeExitMemberDeclaration,
    /// Trait finalizer requirement declaration.
    TraitFinalizerRequirementDeclaration,
    /// Trait destructor requirement declaration.
    TraitDestructorRequirementDeclaration,
    /// Trait scope-enter requirement declaration.
    TraitScopeEnterRequirementDeclaration,
    /// Trait scope-exit requirement declaration.
    TraitScopeExitRequirementDeclaration,
    /// Optional type callable member modifiers in source order.
    TypeCallableMemberModifiers,
    /// Type callable member declaration.
    TypeCallableMemberDeclaration,
    /// Callable directives in source order.
    CallableDirectives,
    /// Optional callable modifiers in source order.
    CallableModifiers,
    /// Callable parameter list including delimiters.
    ParameterList,
    /// Callable parameter item.
    Parameter,
    /// Optional callable parameter modifiers in source order.
    ParameterModifiers,
    /// Callable result clause.
    CallableResultClause,
    /// Callable body block expression.
    CallableBodyBlockExpression,
    /// Runtime expression.
    Expression,
    /// Primary expression wrapper.
    PrimaryExpression,
    /// Access expression root and access-only postfix operations.
    AccessExpression,
    /// Member access postfix operation.
    MemberAccessOperation,
    /// Element index postfix operation.
    ElementIndexOperation,
    /// Call postfix operation.
    CallOperation,
    /// Slice index postfix operation.
    SliceIndexOperation,
    /// Nullable propagation postfix operation.
    NullablePropagationOperation,
    /// Conversion postfix operation.
    ConversionOperation,
    /// Trait-qualified member postfix operation.
    TraitQualifiedMemberOperation,
    /// Runtime argument list including delimiters.
    ArgumentList,
    /// Runtime argument entry.
    Argument,
    /// Struct construction body including delimiters.
    StructConstructionBody,
    /// Struct field initializer entry.
    StructFieldInitializer,
    /// Literal expression.
    LiteralExpression,
    /// Unit expression.
    UnitExpression,
    /// Absence expression.
    AbsenceExpression,
    /// Parenthesized grouped expression.
    GroupedExpression,
    /// Tuple expression.
    TupleExpression,
    /// Array expression.
    ArrayExpression,
    /// Braced generator expression.
    GeneralGeneratorExpression,
    /// Generator iteration expression.
    GeneratorIterationExpression,
    /// Leading-dot variant expression.
    LeadingDotVariantExpression,
    /// Braced block expression.
    BlockExpression,
    /// Item inside a block expression.
    BlockItem,
    /// Local binding declaration.
    LocalBindingDeclaration,
    /// Expression sequenced by a semicolon.
    SequencedExpression,
    /// Conditional `if` expression.
    ConditionalExpression,
    /// Conditional else clause.
    ConditionalElse,
    /// Match expression.
    MatchExpression,
    /// Match subject.
    MatchSubject,
    /// Match body.
    MatchBody,
    /// Match arm.
    MatchArm,
    /// While expression.
    WhileExpression,
    /// For expression.
    ForExpression,
    /// Iteration source.
    IterationSource,
    /// Infinite loop expression.
    LoopExpression,
    /// Scoped `with` expression.
    WithExpression,
    /// Lambda expression.
    LambdaExpression,
    /// Borrow expression.
    BorrowExpression,
    /// Trusted boundary expression.
    TrustBoundaryExpression,
    /// Assertion expression.
    AssertionExpression,
    /// Result propagation expression.
    ResultPropagationExpression,
    /// Catch expression.
    CatchExpression,
    /// Await expression.
    AwaitExpression,
    /// Type-form construction expression.
    TypeFormConstructionExpression,
    /// Boolean fold expression.
    BooleanFoldExpression,
    /// Yield expression.
    YieldExpression,
    /// Return expression.
    ReturnExpression,
    /// Panic expression.
    PanicExpression,
    /// Break expression.
    BreakExpression,
    /// Continue expression.
    ContinueExpression,
    /// Pattern used by binding-like positions.
    IrrefutablePattern,
    /// Entry inside an irrefutable pattern body.
    IrrefutablePatternEntry,
    /// Pattern used by match case positions.
    CasePattern,
    /// Entry inside a case pattern body.
    CasePatternEntry,
    /// Type expression.
    TypeExpression,
    /// Required type annotation on a named declaration item.
    TypeAnnotation,
    /// Identifier with a required type annotation.
    TypedIdentifier,
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
    StaticKeyword,
    StructKeyword,
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
    PlusEqualsToken,
    MinusEqualsToken,
    StarEqualsToken,
    SlashEqualsToken,
    PercentEqualsToken,
    AtEqualsToken,
    AmpersandEqualsToken,
    PipeEqualsToken,
    CaretEqualsToken,
    LessLessEqualsToken,
    GreaterGreaterEqualsToken,
    StarStarEqualsToken,
    EllipsisToken,
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
                | Self::AbiDirective
                | Self::SymbolDirective
                | Self::EntrypointDirective
                | Self::DirectiveArgumentList
                | Self::DirectiveArgument
                | Self::ModuleModifiers
                | Self::ModuleBody
                | Self::UsingDeclaration
                | Self::ExportDeclaration
                | Self::ConstantDeclaration
                | Self::ConstantModifiers
                | Self::StaticDeclaration
                | Self::StaticDirectives
                | Self::ThreadLocalDirective
                | Self::StaticDeclarationModifiers
                | Self::FunctionDeclaration
                | Self::FunctionDirectives
                | Self::FunctionModifiers
                | Self::GenericParameterList
                | Self::GenericTypeParameter
                | Self::GenericConstParameter
                | Self::GenericArgumentList
                | Self::GenericArgument
                | Self::TypeFormArgumentList
                | Self::TypeFormArgument
                | Self::PredicateDeclaration
                | Self::PredicateModifiers
                | Self::PredicateParameterList
                | Self::PredicateParameter
                | Self::CallableContractDeclaration
                | Self::CallableContractModifiers
                | Self::RequiresClause
                | Self::EnsuresClause
                | Self::WithClause
                | Self::UsesClause
                | Self::OverloadModifiers
                | Self::CallableOverloadDeclaration
                | Self::ImplementationOverloadDeclaration
                | Self::ImplementationOverloadSubject
                | Self::OverloadArmList
                | Self::OverloadArm
                | Self::StructDeclaration
                | Self::TypeDirectives
                | Self::LayoutDirective
                | Self::CopyDirective
                | Self::TagDirective
                | Self::TypeModifiers
                | Self::StructBody
                | Self::FieldModifiers
                | Self::StructFieldDeclaration
                | Self::UnionDeclaration
                | Self::UnionBody
                | Self::VariantDirectives
                | Self::UnionVariantDeclaration
                | Self::UnionVariantPayload
                | Self::UnionPayloadField
                | Self::PayloadFieldModifiers
                | Self::TraitDeclaration
                | Self::TraitModifiers
                | Self::TraitBody
                | Self::TraitConstantMemberDeclaration
                | Self::TraitTypeMemberDeclaration
                | Self::TraitPredicateMemberModifiers
                | Self::TraitPredicateMemberDeclaration
                | Self::TraitCallableMemberModifiers
                | Self::TraitCallableMemberDeclaration
                | Self::ImplementationSubject
                | Self::TraitApplication
                | Self::InherentImplementationDeclaration
                | Self::ImplementationBody
                | Self::ImplementationTypeMemberBinding
                | Self::UnnamedTraitImplementationDeclaration
                | Self::NamedTraitImplementationDeclaration
                | Self::ConstructorMemberModifiers
                | Self::TypeConstructorMemberDeclaration
                | Self::AsyncCapableLifecycleMemberModifiers
                | Self::ScopeEnterMemberModifiers
                | Self::SyncLifecycleMemberModifiers
                | Self::FinalizerMemberDeclaration
                | Self::DestructorMemberDeclaration
                | Self::ScopeEnterMemberDeclaration
                | Self::ScopeExitMemberDeclaration
                | Self::TraitFinalizerRequirementDeclaration
                | Self::TraitDestructorRequirementDeclaration
                | Self::TraitScopeEnterRequirementDeclaration
                | Self::TraitScopeExitRequirementDeclaration
                | Self::TypeCallableMemberModifiers
                | Self::TypeCallableMemberDeclaration
                | Self::CallableDirectives
                | Self::CallableModifiers
                | Self::ParameterList
                | Self::Parameter
                | Self::ParameterModifiers
                | Self::CallableResultClause
                | Self::CallableBodyBlockExpression
                | Self::Expression
                | Self::PrimaryExpression
                | Self::AccessExpression
                | Self::MemberAccessOperation
                | Self::ElementIndexOperation
                | Self::CallOperation
                | Self::SliceIndexOperation
                | Self::NullablePropagationOperation
                | Self::ConversionOperation
                | Self::TraitQualifiedMemberOperation
                | Self::ArgumentList
                | Self::Argument
                | Self::StructConstructionBody
                | Self::StructFieldInitializer
                | Self::LiteralExpression
                | Self::UnitExpression
                | Self::AbsenceExpression
                | Self::GroupedExpression
                | Self::TupleExpression
                | Self::ArrayExpression
                | Self::GeneralGeneratorExpression
                | Self::GeneratorIterationExpression
                | Self::LeadingDotVariantExpression
                | Self::BlockExpression
                | Self::BlockItem
                | Self::LocalBindingDeclaration
                | Self::SequencedExpression
                | Self::ConditionalExpression
                | Self::ConditionalElse
                | Self::MatchExpression
                | Self::MatchSubject
                | Self::MatchBody
                | Self::MatchArm
                | Self::WhileExpression
                | Self::ForExpression
                | Self::IterationSource
                | Self::LoopExpression
                | Self::WithExpression
                | Self::LambdaExpression
                | Self::BorrowExpression
                | Self::TrustBoundaryExpression
                | Self::AssertionExpression
                | Self::ResultPropagationExpression
                | Self::CatchExpression
                | Self::AwaitExpression
                | Self::TypeFormConstructionExpression
                | Self::BooleanFoldExpression
                | Self::YieldExpression
                | Self::ReturnExpression
                | Self::PanicExpression
                | Self::BreakExpression
                | Self::ContinueExpression
                | Self::IrrefutablePattern
                | Self::IrrefutablePatternEntry
                | Self::CasePattern
                | Self::CasePatternEntry
                | Self::TypeExpression
                | Self::TypeAnnotation
                | Self::TypedIdentifier
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
                | Self::StaticKeyword
                | Self::StructKeyword
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

    /// Returns whether this kind represents a visibility modifier token.
    pub const fn is_visibility_modifier(self) -> bool {
        matches!(self, Self::PublicKeyword | Self::InternalKeyword)
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

    /// Returns whether this kind represents an expression operator token.
    pub const fn is_expression_operator(self) -> bool {
        matches!(
            self,
            Self::EqualsToken
                | Self::PlusEqualsToken
                | Self::MinusEqualsToken
                | Self::StarEqualsToken
                | Self::SlashEqualsToken
                | Self::PercentEqualsToken
                | Self::AtEqualsToken
                | Self::AmpersandEqualsToken
                | Self::PipeEqualsToken
                | Self::CaretEqualsToken
                | Self::LessLessEqualsToken
                | Self::GreaterGreaterEqualsToken
                | Self::StarStarEqualsToken
                | Self::EllipsisToken
                | Self::DotDotToken
                | Self::PipePipeToken
                | Self::AmpersandAmpersandToken
                | Self::EqualsEqualsToken
                | Self::BangEqualsToken
                | Self::LessToken
                | Self::LessEqualsToken
                | Self::GreaterToken
                | Self::GreaterEqualsToken
                | Self::PipeToken
                | Self::CaretToken
                | Self::AmpersandToken
                | Self::LessLessToken
                | Self::GreaterGreaterToken
                | Self::PlusToken
                | Self::MinusToken
                | Self::StarToken
                | Self::SlashToken
                | Self::PercentToken
                | Self::AtToken
                | Self::StarStarToken
                | Self::TildeToken
                | Self::BangToken
                | Self::ColonToken
        )
    }

    /// Returns whether this kind is a self-delimiting block-shaped expression root.
    pub const fn is_block_shaped_expression(self) -> bool {
        matches!(
            self,
            Self::BlockExpression
                | Self::ConditionalExpression
                | Self::MatchExpression
                | Self::WhileExpression
                | Self::ForExpression
                | Self::LoopExpression
                | Self::WithExpression
        )
    }

    /// Returns whether this kind represents a literal-pattern token.
    pub const fn is_pattern_literal(self) -> bool {
        self.is_literal() || matches!(self, Self::TrueKeyword | Self::FalseKeyword)
    }

    /// Returns the stable machine key for this syntax kind.
    // rust-style: allow(function-too-large, reason = "syntax kind spellings form one exhaustive flat mapping")
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
            Self::AbiDirective => "abi_directive",
            Self::SymbolDirective => "symbol_directive",
            Self::EntrypointDirective => "entrypoint_directive",
            Self::DirectiveArgumentList => "directive_argument_list",
            Self::DirectiveArgument => "directive_argument",
            Self::ModuleModifiers => "module_modifiers",
            Self::ModuleBody => "module_body",
            Self::UsingDeclaration => "using_declaration",
            Self::ExportDeclaration => "export_declaration",
            Self::ConstantDeclaration => "constant_declaration",
            Self::ConstantModifiers => "constant_modifiers",
            Self::StaticDeclaration => "static_declaration",
            Self::StaticDirectives => "static_directives",
            Self::ThreadLocalDirective => "thread_local_directive",
            Self::StaticDeclarationModifiers => "static_declaration_modifiers",
            Self::FunctionDeclaration => "function_declaration",
            Self::FunctionDirectives => "function_directives",
            Self::FunctionModifiers => "function_modifiers",
            Self::GenericParameterList => "generic_parameter_list",
            Self::GenericTypeParameter => "generic_type_parameter",
            Self::GenericConstParameter => "generic_const_parameter",
            Self::GenericArgumentList => "generic_argument_list",
            Self::GenericArgument => "generic_argument",
            Self::TypeFormArgumentList => "type_form_argument_list",
            Self::TypeFormArgument => "type_form_argument",
            Self::PredicateDeclaration => "predicate_declaration",
            Self::PredicateModifiers => "predicate_modifiers",
            Self::PredicateParameterList => "predicate_parameter_list",
            Self::PredicateParameter => "predicate_parameter",
            Self::CallableContractDeclaration => "callable_contract_declaration",
            Self::CallableContractModifiers => "callable_contract_modifiers",
            Self::RequiresClause => "requires_clause",
            Self::EnsuresClause => "ensures_clause",
            Self::WithClause => "with_clause",
            Self::UsesClause => "uses_clause",
            Self::OverloadModifiers => "overload_modifiers",
            Self::CallableOverloadDeclaration => "callable_overload_declaration",
            Self::ImplementationOverloadDeclaration => "implementation_overload_declaration",
            Self::ImplementationOverloadSubject => "implementation_overload_subject",
            Self::OverloadArmList => "overload_arm_list",
            Self::OverloadArm => "overload_arm",
            Self::StructDeclaration => "struct_declaration",
            Self::TypeDirectives => "type_directives",
            Self::LayoutDirective => "layout_directive",
            Self::CopyDirective => "copy_directive",
            Self::TagDirective => "tag_directive",
            Self::TypeModifiers => "type_modifiers",
            Self::StructBody => "struct_body",
            Self::FieldModifiers => "field_modifiers",
            Self::StructFieldDeclaration => "struct_field_declaration",
            Self::UnionDeclaration => "union_declaration",
            Self::UnionBody => "union_body",
            Self::VariantDirectives => "variant_directives",
            Self::UnionVariantDeclaration => "union_variant_declaration",
            Self::UnionVariantPayload => "union_variant_payload",
            Self::UnionPayloadField => "union_payload_field",
            Self::PayloadFieldModifiers => "payload_field_modifiers",
            Self::TraitDeclaration => "trait_declaration",
            Self::TraitModifiers => "trait_modifiers",
            Self::TraitBody => "trait_body",
            Self::TraitConstantMemberDeclaration => "trait_constant_member_declaration",
            Self::TraitTypeMemberDeclaration => "trait_type_member_declaration",
            Self::TraitPredicateMemberModifiers => "trait_predicate_member_modifiers",
            Self::TraitPredicateMemberDeclaration => "trait_predicate_member_declaration",
            Self::TraitCallableMemberModifiers => "trait_callable_member_modifiers",
            Self::TraitCallableMemberDeclaration => "trait_callable_member_declaration",
            Self::ImplementationSubject => "implementation_subject",
            Self::TraitApplication => "trait_application",
            Self::InherentImplementationDeclaration => "inherent_implementation_declaration",
            Self::ImplementationBody => "implementation_body",
            Self::ImplementationTypeMemberBinding => "implementation_type_member_binding",
            Self::UnnamedTraitImplementationDeclaration => {
                "unnamed_trait_implementation_declaration"
            }
            Self::NamedTraitImplementationDeclaration => "named_trait_implementation_declaration",
            Self::ConstructorMemberModifiers => "constructor_member_modifiers",
            Self::TypeConstructorMemberDeclaration => "type_constructor_member_declaration",
            Self::AsyncCapableLifecycleMemberModifiers => {
                "async_capable_lifecycle_member_modifiers"
            }
            Self::ScopeEnterMemberModifiers => "scope_enter_member_modifiers",
            Self::SyncLifecycleMemberModifiers => "sync_lifecycle_member_modifiers",
            Self::FinalizerMemberDeclaration => "finalizer_member_declaration",
            Self::DestructorMemberDeclaration => "destructor_member_declaration",
            Self::ScopeEnterMemberDeclaration => "scope_enter_member_declaration",
            Self::ScopeExitMemberDeclaration => "scope_exit_member_declaration",
            Self::TraitFinalizerRequirementDeclaration => "trait_finalizer_requirement_declaration",
            Self::TraitDestructorRequirementDeclaration => {
                "trait_destructor_requirement_declaration"
            }
            Self::TraitScopeEnterRequirementDeclaration => {
                "trait_scope_enter_requirement_declaration"
            }
            Self::TraitScopeExitRequirementDeclaration => {
                "trait_scope_exit_requirement_declaration"
            }
            Self::TypeCallableMemberModifiers => "type_callable_member_modifiers",
            Self::TypeCallableMemberDeclaration => "type_callable_member_declaration",
            Self::CallableDirectives => "callable_directives",
            Self::CallableModifiers => "callable_modifiers",
            Self::ParameterList => "parameter_list",
            Self::Parameter => "parameter",
            Self::ParameterModifiers => "parameter_modifiers",
            Self::CallableResultClause => "callable_result_clause",
            Self::CallableBodyBlockExpression => "callable_body_block_expression",
            Self::Expression => "expression",
            Self::PrimaryExpression => "primary_expression",
            Self::AccessExpression => "access_expression",
            Self::MemberAccessOperation => "member_access_operation",
            Self::ElementIndexOperation => "element_index_operation",
            Self::CallOperation => "call_operation",
            Self::SliceIndexOperation => "slice_index_operation",
            Self::NullablePropagationOperation => "nullable_propagation_operation",
            Self::ConversionOperation => "conversion_operation",
            Self::TraitQualifiedMemberOperation => "trait_qualified_member_operation",
            Self::ArgumentList => "argument_list",
            Self::Argument => "argument",
            Self::StructConstructionBody => "struct_construction_body",
            Self::StructFieldInitializer => "struct_field_initializer",
            Self::LiteralExpression => "literal_expression",
            Self::UnitExpression => "unit_expression",
            Self::AbsenceExpression => "absence_expression",
            Self::GroupedExpression => "grouped_expression",
            Self::TupleExpression => "tuple_expression",
            Self::ArrayExpression => "array_expression",
            Self::GeneralGeneratorExpression => "general_generator_expression",
            Self::GeneratorIterationExpression => "generator_iteration_expression",
            Self::LeadingDotVariantExpression => "leading_dot_variant_expression",
            Self::BlockExpression => "block_expression",
            Self::BlockItem => "block_item",
            Self::LocalBindingDeclaration => "local_binding_declaration",
            Self::SequencedExpression => "sequenced_expression",
            Self::ConditionalExpression => "conditional_expression",
            Self::ConditionalElse => "conditional_else",
            Self::MatchExpression => "match_expression",
            Self::MatchSubject => "match_subject",
            Self::MatchBody => "match_body",
            Self::MatchArm => "match_arm",
            Self::WhileExpression => "while_expression",
            Self::ForExpression => "for_expression",
            Self::IterationSource => "iteration_source",
            Self::LoopExpression => "loop_expression",
            Self::WithExpression => "with_expression",
            Self::LambdaExpression => "lambda_expression",
            Self::BorrowExpression => "borrow_expression",
            Self::TrustBoundaryExpression => "trust_boundary_expression",
            Self::AssertionExpression => "assertion_expression",
            Self::ResultPropagationExpression => "result_propagation_expression",
            Self::CatchExpression => "catch_expression",
            Self::AwaitExpression => "await_expression",
            Self::TypeFormConstructionExpression => "type_form_construction_expression",
            Self::BooleanFoldExpression => "boolean_fold_expression",
            Self::YieldExpression => "yield_expression",
            Self::ReturnExpression => "return_expression",
            Self::PanicExpression => "panic_expression",
            Self::BreakExpression => "break_expression",
            Self::ContinueExpression => "continue_expression",
            Self::IrrefutablePattern => "irrefutable_pattern",
            Self::IrrefutablePatternEntry => "irrefutable_pattern_entry",
            Self::CasePattern => "case_pattern",
            Self::CasePatternEntry => "case_pattern_entry",
            Self::TypeExpression => "type_expression",
            Self::TypeAnnotation => "type_annotation",
            Self::TypedIdentifier => "typed_identifier",
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
            Self::StaticKeyword => "static_keyword",
            Self::StructKeyword => "struct_keyword",
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
            Self::PlusEqualsToken => "plus_equals_token",
            Self::MinusEqualsToken => "minus_equals_token",
            Self::StarEqualsToken => "star_equals_token",
            Self::SlashEqualsToken => "slash_equals_token",
            Self::PercentEqualsToken => "percent_equals_token",
            Self::AtEqualsToken => "at_equals_token",
            Self::AmpersandEqualsToken => "ampersand_equals_token",
            Self::PipeEqualsToken => "pipe_equals_token",
            Self::CaretEqualsToken => "caret_equals_token",
            Self::LessLessEqualsToken => "less_less_equals_token",
            Self::GreaterGreaterEqualsToken => "greater_greater_equals_token",
            Self::StarStarEqualsToken => "star_star_equals_token",
            Self::EllipsisToken => "ellipsis_token",
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
        assert!(SyntaxKind::AbiDirective.is_node());
        assert!(SyntaxKind::SymbolDirective.is_node());
        assert!(SyntaxKind::EntrypointDirective.is_node());
        assert!(SyntaxKind::DirectiveArgumentList.is_node());
        assert!(SyntaxKind::DirectiveArgument.is_node());

        assert!(SyntaxKind::ModuleModifiers.is_node());
        assert!(SyntaxKind::ModuleBody.is_node());

        assert!(SyntaxKind::UsingDeclaration.is_node());
        assert!(SyntaxKind::ExportDeclaration.is_node());
        assert!(SyntaxKind::ConstantDeclaration.is_node());
        assert!(SyntaxKind::ConstantModifiers.is_node());

        assert!(SyntaxKind::FunctionDeclaration.is_node());
        assert!(SyntaxKind::FunctionDirectives.is_node());
        assert!(SyntaxKind::FunctionModifiers.is_node());

        assert!(SyntaxKind::GenericParameterList.is_node());
        assert!(SyntaxKind::GenericTypeParameter.is_node());
        assert!(SyntaxKind::GenericConstParameter.is_node());
        assert!(SyntaxKind::GenericArgumentList.is_node());
        assert!(SyntaxKind::GenericArgument.is_node());

        assert!(SyntaxKind::TypeFormArgumentList.is_node());
        assert!(SyntaxKind::TypeFormArgument.is_node());

        assert!(SyntaxKind::PredicateDeclaration.is_node());
        assert!(SyntaxKind::PredicateModifiers.is_node());
        assert!(SyntaxKind::PredicateParameterList.is_node());
        assert!(SyntaxKind::PredicateParameter.is_node());

        assert!(SyntaxKind::CallableContractDeclaration.is_node());
        assert!(SyntaxKind::CallableContractModifiers.is_node());
        assert!(SyntaxKind::RequiresClause.is_node());
        assert!(SyntaxKind::EnsuresClause.is_node());
        assert!(SyntaxKind::WithClause.is_node());
        assert!(SyntaxKind::UsesClause.is_node());
        assert!(SyntaxKind::OverloadModifiers.is_node());
        assert!(SyntaxKind::CallableOverloadDeclaration.is_node());
        assert!(SyntaxKind::ImplementationOverloadDeclaration.is_node());
        assert!(SyntaxKind::ImplementationOverloadSubject.is_node());
        assert!(SyntaxKind::OverloadArmList.is_node());
        assert!(SyntaxKind::OverloadArm.is_node());

        assert!(SyntaxKind::StructDeclaration.is_node());
        assert!(SyntaxKind::TypeDirectives.is_node());
        assert!(SyntaxKind::LayoutDirective.is_node());
        assert!(SyntaxKind::CopyDirective.is_node());
        assert!(SyntaxKind::TagDirective.is_node());
        assert!(SyntaxKind::TypeModifiers.is_node());
        assert!(SyntaxKind::StructBody.is_node());
        assert!(SyntaxKind::FieldModifiers.is_node());
        assert!(SyntaxKind::StructFieldDeclaration.is_node());
        assert!(SyntaxKind::UnionDeclaration.is_node());
        assert!(SyntaxKind::UnionBody.is_node());
        assert!(SyntaxKind::VariantDirectives.is_node());
        assert!(SyntaxKind::UnionVariantDeclaration.is_node());
        assert!(SyntaxKind::UnionVariantPayload.is_node());
        assert!(SyntaxKind::UnionPayloadField.is_node());
        assert!(SyntaxKind::PayloadFieldModifiers.is_node());

        assert!(SyntaxKind::TraitDeclaration.is_node());
        assert!(SyntaxKind::TraitModifiers.is_node());
        assert!(SyntaxKind::TraitBody.is_node());
        assert!(SyntaxKind::TraitConstantMemberDeclaration.is_node());
        assert!(SyntaxKind::TraitTypeMemberDeclaration.is_node());
        assert!(SyntaxKind::TraitPredicateMemberModifiers.is_node());
        assert!(SyntaxKind::TraitPredicateMemberDeclaration.is_node());
        assert!(SyntaxKind::TraitCallableMemberModifiers.is_node());
        assert!(SyntaxKind::TraitCallableMemberDeclaration.is_node());

        assert!(SyntaxKind::ImplementationSubject.is_node());
        assert!(SyntaxKind::TraitApplication.is_node());
        assert!(SyntaxKind::InherentImplementationDeclaration.is_node());
        assert!(SyntaxKind::ImplementationBody.is_node());
        assert!(SyntaxKind::ImplementationTypeMemberBinding.is_node());
        assert!(SyntaxKind::UnnamedTraitImplementationDeclaration.is_node());
        assert!(SyntaxKind::NamedTraitImplementationDeclaration.is_node());
        assert!(SyntaxKind::ConstructorMemberModifiers.is_node());
        assert!(SyntaxKind::TypeConstructorMemberDeclaration.is_node());
        assert!(SyntaxKind::AsyncCapableLifecycleMemberModifiers.is_node());
        assert!(SyntaxKind::SyncLifecycleMemberModifiers.is_node());
        assert!(SyntaxKind::FinalizerMemberDeclaration.is_node());
        assert!(SyntaxKind::DestructorMemberDeclaration.is_node());
        assert!(SyntaxKind::ScopeEnterMemberDeclaration.is_node());
        assert!(SyntaxKind::ScopeExitMemberDeclaration.is_node());
        assert!(SyntaxKind::TraitFinalizerRequirementDeclaration.is_node());
        assert!(SyntaxKind::TraitDestructorRequirementDeclaration.is_node());
        assert!(SyntaxKind::TraitScopeEnterRequirementDeclaration.is_node());
        assert!(SyntaxKind::TraitScopeExitRequirementDeclaration.is_node());
        assert!(SyntaxKind::TypeCallableMemberModifiers.is_node());
        assert!(SyntaxKind::TypeCallableMemberDeclaration.is_node());

        assert!(SyntaxKind::CallableDirectives.is_node());
        assert!(SyntaxKind::CallableModifiers.is_node());
        assert!(SyntaxKind::ParameterList.is_node());
        assert!(SyntaxKind::Parameter.is_node());
        assert!(SyntaxKind::ParameterModifiers.is_node());

        assert!(SyntaxKind::CallableResultClause.is_node());
        assert!(SyntaxKind::CallableBodyBlockExpression.is_node());
        assert!(SyntaxKind::Expression.is_node());
        assert!(SyntaxKind::PrimaryExpression.is_node());
        assert!(SyntaxKind::AccessExpression.is_node());
        assert!(SyntaxKind::MemberAccessOperation.is_node());
        assert!(SyntaxKind::ElementIndexOperation.is_node());
        assert!(SyntaxKind::CallOperation.is_node());
        assert!(SyntaxKind::SliceIndexOperation.is_node());
        assert!(SyntaxKind::NullablePropagationOperation.is_node());
        assert!(SyntaxKind::ConversionOperation.is_node());
        assert!(SyntaxKind::TraitQualifiedMemberOperation.is_node());
        assert!(SyntaxKind::ArgumentList.is_node());
        assert!(SyntaxKind::Argument.is_node());
        assert!(SyntaxKind::StructConstructionBody.is_node());
        assert!(SyntaxKind::StructFieldInitializer.is_node());
        assert!(SyntaxKind::LiteralExpression.is_node());
        assert!(SyntaxKind::UnitExpression.is_node());
        assert!(SyntaxKind::AbsenceExpression.is_node());
        assert!(SyntaxKind::GroupedExpression.is_node());
        assert!(SyntaxKind::TupleExpression.is_node());
        assert!(SyntaxKind::ArrayExpression.is_node());
        assert!(SyntaxKind::GeneralGeneratorExpression.is_node());
        assert!(SyntaxKind::GeneratorIterationExpression.is_node());
        assert!(SyntaxKind::LeadingDotVariantExpression.is_node());
        assert!(SyntaxKind::BlockExpression.is_node());
        assert!(SyntaxKind::BlockItem.is_node());
        assert!(SyntaxKind::LocalBindingDeclaration.is_node());
        assert!(SyntaxKind::SequencedExpression.is_node());
        assert!(SyntaxKind::ConditionalExpression.is_node());
        assert!(SyntaxKind::ConditionalElse.is_node());
        assert!(SyntaxKind::MatchExpression.is_node());
        assert!(SyntaxKind::MatchSubject.is_node());
        assert!(SyntaxKind::MatchBody.is_node());
        assert!(SyntaxKind::MatchArm.is_node());
        assert!(SyntaxKind::WhileExpression.is_node());
        assert!(SyntaxKind::ForExpression.is_node());
        assert!(SyntaxKind::IterationSource.is_node());
        assert!(SyntaxKind::LoopExpression.is_node());
        assert!(SyntaxKind::WithExpression.is_node());
        assert!(SyntaxKind::LambdaExpression.is_node());
        assert!(SyntaxKind::BorrowExpression.is_node());
        assert!(SyntaxKind::TrustBoundaryExpression.is_node());
        assert!(SyntaxKind::AssertionExpression.is_node());
        assert!(SyntaxKind::ResultPropagationExpression.is_node());
        assert!(SyntaxKind::CatchExpression.is_node());
        assert!(SyntaxKind::AwaitExpression.is_node());
        assert!(SyntaxKind::TypeFormConstructionExpression.is_node());
        assert!(SyntaxKind::BooleanFoldExpression.is_node());
        assert!(SyntaxKind::YieldExpression.is_node());
        assert!(SyntaxKind::ReturnExpression.is_node());
        assert!(SyntaxKind::PanicExpression.is_node());
        assert!(SyntaxKind::BreakExpression.is_node());
        assert!(SyntaxKind::ContinueExpression.is_node());
        assert!(SyntaxKind::IrrefutablePattern.is_node());
        assert!(SyntaxKind::IrrefutablePatternEntry.is_node());
        assert!(SyntaxKind::CasePattern.is_node());
        assert!(SyntaxKind::CasePatternEntry.is_node());
        assert!(SyntaxKind::TypeExpression.is_node());
        assert!(SyntaxKind::TypeAnnotation.is_node());
        assert!(SyntaxKind::TypedIdentifier.is_node());

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

        assert!(SyntaxKind::PublicKeyword.is_visibility_modifier());
        assert!(SyntaxKind::InternalKeyword.is_visibility_modifier());
        assert!(!SyntaxKind::TrustedKeyword.is_visibility_modifier());

        assert!(SyntaxKind::RealLiteralToken.is_literal());
        assert!(SyntaxKind::ImaginaryLiteralToken.is_literal());
        assert!(!SyntaxKind::TupleElementIndexToken.is_literal());

        assert!(SyntaxKind::TrueKeyword.is_pattern_literal());
        assert!(SyntaxKind::FalseKeyword.is_pattern_literal());
        assert!(SyntaxKind::StringLiteralToken.is_pattern_literal());
        assert!(!SyntaxKind::NoneKeyword.is_pattern_literal());

        assert!(SyntaxKind::EqualsToken.is_expression_operator());
        assert!(SyntaxKind::PlusToken.is_expression_operator());
        assert!(!SyntaxKind::ArrowToken.is_expression_operator());
        assert!(SyntaxKind::DotDotToken.is_expression_operator());

        assert!(SyntaxKind::BlockExpression.is_block_shaped_expression());
        assert!(SyntaxKind::ConditionalExpression.is_block_shaped_expression());
        assert!(SyntaxKind::MatchExpression.is_block_shaped_expression());
        assert!(SyntaxKind::WhileExpression.is_block_shaped_expression());
        assert!(SyntaxKind::ForExpression.is_block_shaped_expression());
        assert!(SyntaxKind::LoopExpression.is_block_shaped_expression());
        assert!(SyntaxKind::WithExpression.is_block_shaped_expression());
        assert!(!SyntaxKind::LambdaExpression.is_block_shaped_expression());
        assert!(!SyntaxKind::GeneralGeneratorExpression.is_block_shaped_expression());
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
        assert_eq!(SyntaxKind::AbiDirective.as_str(), "abi_directive");
        assert_eq!(SyntaxKind::SymbolDirective.as_str(), "symbol_directive");

        assert_eq!(
            SyntaxKind::EntrypointDirective.as_str(),
            "entrypoint_directive"
        );

        assert_eq!(
            SyntaxKind::DirectiveArgumentList.as_str(),
            "directive_argument_list"
        );

        assert_eq!(SyntaxKind::DirectiveArgument.as_str(), "directive_argument");

        assert_eq!(SyntaxKind::ModuleModifiers.as_str(), "module_modifiers");
        assert_eq!(SyntaxKind::ModuleBody.as_str(), "module_body");
        assert_eq!(SyntaxKind::UsingDeclaration.as_str(), "using_declaration");
        assert_eq!(SyntaxKind::ExportDeclaration.as_str(), "export_declaration");

        assert_eq!(
            SyntaxKind::ConstantDeclaration.as_str(),
            "constant_declaration"
        );

        assert_eq!(SyntaxKind::ConstantModifiers.as_str(), "constant_modifiers");

        assert_eq!(
            SyntaxKind::FunctionDeclaration.as_str(),
            "function_declaration"
        );

        assert_eq!(
            SyntaxKind::FunctionDirectives.as_str(),
            "function_directives"
        );

        assert_eq!(SyntaxKind::FunctionModifiers.as_str(), "function_modifiers");

        assert_eq!(
            SyntaxKind::GenericParameterList.as_str(),
            "generic_parameter_list"
        );

        assert_eq!(
            SyntaxKind::GenericTypeParameter.as_str(),
            "generic_type_parameter"
        );

        assert_eq!(
            SyntaxKind::GenericConstParameter.as_str(),
            "generic_const_parameter"
        );

        assert_eq!(
            SyntaxKind::GenericArgumentList.as_str(),
            "generic_argument_list"
        );

        assert_eq!(SyntaxKind::GenericArgument.as_str(), "generic_argument");

        assert_eq!(
            SyntaxKind::TypeFormArgumentList.as_str(),
            "type_form_argument_list"
        );

        assert_eq!(SyntaxKind::TypeFormArgument.as_str(), "type_form_argument");

        assert_eq!(
            SyntaxKind::PredicateDeclaration.as_str(),
            "predicate_declaration"
        );

        assert_eq!(
            SyntaxKind::PredicateModifiers.as_str(),
            "predicate_modifiers"
        );

        assert_eq!(
            SyntaxKind::PredicateParameterList.as_str(),
            "predicate_parameter_list"
        );

        assert_eq!(
            SyntaxKind::PredicateParameter.as_str(),
            "predicate_parameter"
        );

        assert_eq!(
            SyntaxKind::CallableContractDeclaration.as_str(),
            "callable_contract_declaration"
        );

        assert_eq!(
            SyntaxKind::CallableContractModifiers.as_str(),
            "callable_contract_modifiers"
        );

        assert_eq!(SyntaxKind::RequiresClause.as_str(), "requires_clause");
        assert_eq!(SyntaxKind::EnsuresClause.as_str(), "ensures_clause");
        assert_eq!(SyntaxKind::WithClause.as_str(), "with_clause");
        assert_eq!(SyntaxKind::UsesClause.as_str(), "uses_clause");

        assert_eq!(SyntaxKind::OverloadModifiers.as_str(), "overload_modifiers");

        assert_eq!(
            SyntaxKind::CallableOverloadDeclaration.as_str(),
            "callable_overload_declaration"
        );

        assert_eq!(
            SyntaxKind::ImplementationOverloadDeclaration.as_str(),
            "implementation_overload_declaration"
        );

        assert_eq!(
            SyntaxKind::ImplementationOverloadSubject.as_str(),
            "implementation_overload_subject"
        );

        assert_eq!(SyntaxKind::OverloadArmList.as_str(), "overload_arm_list");
        assert_eq!(SyntaxKind::OverloadArm.as_str(), "overload_arm");

        assert_eq!(SyntaxKind::StructDeclaration.as_str(), "struct_declaration");
        assert_eq!(SyntaxKind::TypeDirectives.as_str(), "type_directives");
        assert_eq!(SyntaxKind::LayoutDirective.as_str(), "layout_directive");
        assert_eq!(SyntaxKind::CopyDirective.as_str(), "copy_directive");
        assert_eq!(SyntaxKind::TagDirective.as_str(), "tag_directive");
        assert_eq!(SyntaxKind::TypeModifiers.as_str(), "type_modifiers");

        assert_eq!(SyntaxKind::StructBody.as_str(), "struct_body");
        assert_eq!(SyntaxKind::FieldModifiers.as_str(), "field_modifiers");

        assert_eq!(
            SyntaxKind::StructFieldDeclaration.as_str(),
            "struct_field_declaration"
        );

        assert_eq!(SyntaxKind::UnionDeclaration.as_str(), "union_declaration");
        assert_eq!(SyntaxKind::UnionBody.as_str(), "union_body");

        assert_eq!(SyntaxKind::VariantDirectives.as_str(), "variant_directives");

        assert_eq!(
            SyntaxKind::UnionVariantDeclaration.as_str(),
            "union_variant_declaration"
        );

        assert_eq!(
            SyntaxKind::UnionVariantPayload.as_str(),
            "union_variant_payload"
        );

        assert_eq!(
            SyntaxKind::UnionPayloadField.as_str(),
            "union_payload_field"
        );

        assert_eq!(
            SyntaxKind::PayloadFieldModifiers.as_str(),
            "payload_field_modifiers"
        );

        assert_eq!(SyntaxKind::TraitDeclaration.as_str(), "trait_declaration");
        assert_eq!(SyntaxKind::TraitModifiers.as_str(), "trait_modifiers");
        assert_eq!(SyntaxKind::TraitBody.as_str(), "trait_body");

        assert_eq!(
            SyntaxKind::TraitConstantMemberDeclaration.as_str(),
            "trait_constant_member_declaration"
        );

        assert_eq!(
            SyntaxKind::TraitTypeMemberDeclaration.as_str(),
            "trait_type_member_declaration"
        );

        assert_eq!(
            SyntaxKind::TraitPredicateMemberModifiers.as_str(),
            "trait_predicate_member_modifiers"
        );

        assert_eq!(
            SyntaxKind::TraitPredicateMemberDeclaration.as_str(),
            "trait_predicate_member_declaration"
        );

        assert_eq!(
            SyntaxKind::TraitCallableMemberModifiers.as_str(),
            "trait_callable_member_modifiers"
        );

        assert_eq!(
            SyntaxKind::TraitCallableMemberDeclaration.as_str(),
            "trait_callable_member_declaration"
        );

        assert_eq!(
            SyntaxKind::ImplementationSubject.as_str(),
            "implementation_subject"
        );

        assert_eq!(SyntaxKind::TraitApplication.as_str(), "trait_application");

        assert_eq!(
            SyntaxKind::InherentImplementationDeclaration.as_str(),
            "inherent_implementation_declaration"
        );

        assert_eq!(
            SyntaxKind::ImplementationBody.as_str(),
            "implementation_body"
        );

        assert_eq!(
            SyntaxKind::ImplementationTypeMemberBinding.as_str(),
            "implementation_type_member_binding"
        );

        assert_eq!(
            SyntaxKind::UnnamedTraitImplementationDeclaration.as_str(),
            "unnamed_trait_implementation_declaration"
        );

        assert_eq!(
            SyntaxKind::NamedTraitImplementationDeclaration.as_str(),
            "named_trait_implementation_declaration"
        );

        assert_eq!(
            SyntaxKind::ConstructorMemberModifiers.as_str(),
            "constructor_member_modifiers"
        );

        assert_eq!(
            SyntaxKind::TypeConstructorMemberDeclaration.as_str(),
            "type_constructor_member_declaration"
        );

        assert_eq!(
            SyntaxKind::AsyncCapableLifecycleMemberModifiers.as_str(),
            "async_capable_lifecycle_member_modifiers"
        );

        assert_eq!(
            SyntaxKind::SyncLifecycleMemberModifiers.as_str(),
            "sync_lifecycle_member_modifiers"
        );

        assert_eq!(
            SyntaxKind::FinalizerMemberDeclaration.as_str(),
            "finalizer_member_declaration"
        );

        assert_eq!(
            SyntaxKind::DestructorMemberDeclaration.as_str(),
            "destructor_member_declaration"
        );

        assert_eq!(
            SyntaxKind::ScopeEnterMemberDeclaration.as_str(),
            "scope_enter_member_declaration"
        );

        assert_eq!(
            SyntaxKind::ScopeExitMemberDeclaration.as_str(),
            "scope_exit_member_declaration"
        );

        assert_eq!(
            SyntaxKind::TraitFinalizerRequirementDeclaration.as_str(),
            "trait_finalizer_requirement_declaration"
        );

        assert_eq!(
            SyntaxKind::TraitDestructorRequirementDeclaration.as_str(),
            "trait_destructor_requirement_declaration"
        );

        assert_eq!(
            SyntaxKind::TraitScopeEnterRequirementDeclaration.as_str(),
            "trait_scope_enter_requirement_declaration"
        );

        assert_eq!(
            SyntaxKind::TraitScopeExitRequirementDeclaration.as_str(),
            "trait_scope_exit_requirement_declaration"
        );

        assert_eq!(
            SyntaxKind::TypeCallableMemberModifiers.as_str(),
            "type_callable_member_modifiers"
        );

        assert_eq!(
            SyntaxKind::TypeCallableMemberDeclaration.as_str(),
            "type_callable_member_declaration"
        );

        assert_eq!(
            SyntaxKind::CallableDirectives.as_str(),
            "callable_directives"
        );

        assert_eq!(SyntaxKind::CallableModifiers.as_str(), "callable_modifiers");
        assert_eq!(SyntaxKind::ParameterList.as_str(), "parameter_list");
        assert_eq!(SyntaxKind::Parameter.as_str(), "parameter");

        assert_eq!(
            SyntaxKind::ParameterModifiers.as_str(),
            "parameter_modifiers"
        );

        assert_eq!(
            SyntaxKind::CallableResultClause.as_str(),
            "callable_result_clause"
        );

        assert_eq!(
            SyntaxKind::CallableBodyBlockExpression.as_str(),
            "callable_body_block_expression"
        );

        assert_eq!(SyntaxKind::Expression.as_str(), "expression");
        assert_eq!(SyntaxKind::PrimaryExpression.as_str(), "primary_expression");
        assert_eq!(SyntaxKind::AccessExpression.as_str(), "access_expression");

        assert_eq!(
            SyntaxKind::MemberAccessOperation.as_str(),
            "member_access_operation"
        );

        assert_eq!(
            SyntaxKind::ElementIndexOperation.as_str(),
            "element_index_operation"
        );

        assert_eq!(SyntaxKind::CallOperation.as_str(), "call_operation");

        assert_eq!(
            SyntaxKind::SliceIndexOperation.as_str(),
            "slice_index_operation"
        );

        assert_eq!(
            SyntaxKind::NullablePropagationOperation.as_str(),
            "nullable_propagation_operation"
        );

        assert_eq!(
            SyntaxKind::ConversionOperation.as_str(),
            "conversion_operation"
        );

        assert_eq!(
            SyntaxKind::TraitQualifiedMemberOperation.as_str(),
            "trait_qualified_member_operation"
        );

        assert_eq!(SyntaxKind::ArgumentList.as_str(), "argument_list");
        assert_eq!(SyntaxKind::Argument.as_str(), "argument");

        assert_eq!(
            SyntaxKind::StructConstructionBody.as_str(),
            "struct_construction_body"
        );

        assert_eq!(
            SyntaxKind::StructFieldInitializer.as_str(),
            "struct_field_initializer"
        );

        assert_eq!(SyntaxKind::LiteralExpression.as_str(), "literal_expression");

        assert_eq!(SyntaxKind::UnitExpression.as_str(), "unit_expression");
        assert_eq!(SyntaxKind::AbsenceExpression.as_str(), "absence_expression");

        assert_eq!(SyntaxKind::GroupedExpression.as_str(), "grouped_expression");

        assert_eq!(SyntaxKind::TupleExpression.as_str(), "tuple_expression");
        assert_eq!(SyntaxKind::ArrayExpression.as_str(), "array_expression");

        assert_eq!(
            SyntaxKind::GeneralGeneratorExpression.as_str(),
            "general_generator_expression"
        );

        assert_eq!(
            SyntaxKind::GeneratorIterationExpression.as_str(),
            "generator_iteration_expression"
        );

        assert_eq!(
            SyntaxKind::LeadingDotVariantExpression.as_str(),
            "leading_dot_variant_expression"
        );

        assert_eq!(SyntaxKind::BlockExpression.as_str(), "block_expression");
        assert_eq!(SyntaxKind::BlockItem.as_str(), "block_item");

        assert_eq!(
            SyntaxKind::LocalBindingDeclaration.as_str(),
            "local_binding_declaration"
        );

        assert_eq!(
            SyntaxKind::SequencedExpression.as_str(),
            "sequenced_expression"
        );

        assert_eq!(
            SyntaxKind::ConditionalExpression.as_str(),
            "conditional_expression"
        );

        assert_eq!(SyntaxKind::ConditionalElse.as_str(), "conditional_else");
        assert_eq!(SyntaxKind::MatchExpression.as_str(), "match_expression");
        assert_eq!(SyntaxKind::MatchSubject.as_str(), "match_subject");
        assert_eq!(SyntaxKind::MatchBody.as_str(), "match_body");
        assert_eq!(SyntaxKind::MatchArm.as_str(), "match_arm");
        assert_eq!(SyntaxKind::WhileExpression.as_str(), "while_expression");
        assert_eq!(SyntaxKind::ForExpression.as_str(), "for_expression");
        assert_eq!(SyntaxKind::IterationSource.as_str(), "iteration_source");
        assert_eq!(SyntaxKind::LoopExpression.as_str(), "loop_expression");
        assert_eq!(SyntaxKind::WithExpression.as_str(), "with_expression");
        assert_eq!(SyntaxKind::LambdaExpression.as_str(), "lambda_expression");
        assert_eq!(SyntaxKind::BorrowExpression.as_str(), "borrow_expression");

        assert_eq!(
            SyntaxKind::TrustBoundaryExpression.as_str(),
            "trust_boundary_expression"
        );

        assert_eq!(
            SyntaxKind::AssertionExpression.as_str(),
            "assertion_expression"
        );

        assert_eq!(
            SyntaxKind::ResultPropagationExpression.as_str(),
            "result_propagation_expression"
        );

        assert_eq!(SyntaxKind::CatchExpression.as_str(), "catch_expression");
        assert_eq!(SyntaxKind::AwaitExpression.as_str(), "await_expression");

        assert_eq!(
            SyntaxKind::TypeFormConstructionExpression.as_str(),
            "type_form_construction_expression"
        );

        assert_eq!(
            SyntaxKind::BooleanFoldExpression.as_str(),
            "boolean_fold_expression"
        );

        assert_eq!(SyntaxKind::YieldExpression.as_str(), "yield_expression");
        assert_eq!(SyntaxKind::ReturnExpression.as_str(), "return_expression");
        assert_eq!(SyntaxKind::PanicExpression.as_str(), "panic_expression");
        assert_eq!(SyntaxKind::BreakExpression.as_str(), "break_expression");

        assert_eq!(
            SyntaxKind::ContinueExpression.as_str(),
            "continue_expression"
        );

        assert_eq!(
            SyntaxKind::IrrefutablePattern.as_str(),
            "irrefutable_pattern"
        );

        assert_eq!(
            SyntaxKind::IrrefutablePatternEntry.as_str(),
            "irrefutable_pattern_entry"
        );

        assert_eq!(SyntaxKind::CasePattern.as_str(), "case_pattern");
        assert_eq!(SyntaxKind::CasePatternEntry.as_str(), "case_pattern_entry");
        assert_eq!(SyntaxKind::TypeExpression.as_str(), "type_expression");
        assert_eq!(SyntaxKind::TypeAnnotation.as_str(), "type_annotation");
        assert_eq!(SyntaxKind::TypedIdentifier.as_str(), "typed_identifier");
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
