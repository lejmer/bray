mod access;
mod assertion;
mod boolean;
mod call;
mod collection;
mod conditional;
mod construction;
mod effect;
mod flow;
mod generator;
mod lambda;
mod literal;
mod looping;
mod r#match;
mod postfix;
mod primary;
mod root;
mod type_form;

pub use access::{
    AccessExpressionSyntax, AccessExpressionSyntaxBuilder, ElementIndexOperationSyntax,
    ElementIndexOperationSyntaxBuilder, MemberAccessOperationSyntax,
    MemberAccessOperationSyntaxBuilder,
};
pub use assertion::{AssertionExpressionSyntax, AssertionExpressionSyntaxBuilder};
pub use boolean::{BooleanFoldExpressionSyntax, BooleanFoldExpressionSyntaxBuilder};
pub use call::{
    ArgumentListSyntax, ArgumentListSyntaxBuilder, ArgumentSyntax, ArgumentSyntaxBuilder,
    CallOperationSyntax, CallOperationSyntaxBuilder,
};
pub use collection::{
    ArrayExpressionSyntax, ArrayExpressionSyntaxBuilder, GroupedExpressionSyntax,
    GroupedExpressionSyntaxBuilder, TupleExpressionSyntax, TupleExpressionSyntaxBuilder,
};
pub use conditional::{
    ConditionalElseSyntax, ConditionalElseSyntaxBuilder, ConditionalExpressionSyntax,
    ConditionalExpressionSyntaxBuilder,
};
pub use construction::{
    StructConstructionBodySyntax, StructConstructionBodySyntaxBuilder,
    StructFieldInitializerSyntax, StructFieldInitializerSyntaxBuilder,
};
pub use effect::{
    AwaitExpressionSyntax, AwaitExpressionSyntaxBuilder, BorrowExpressionSyntax,
    BorrowExpressionSyntaxBuilder, CatchExpressionSyntax, CatchExpressionSyntaxBuilder,
    ResultPropagationExpressionSyntax, ResultPropagationExpressionSyntaxBuilder,
    TrustBoundaryExpressionSyntax, TrustBoundaryExpressionSyntaxBuilder,
};
pub use flow::{
    BreakExpressionSyntax, BreakExpressionSyntaxBuilder, ContinueExpressionSyntax,
    ContinueExpressionSyntaxBuilder, PanicExpressionSyntax, PanicExpressionSyntaxBuilder,
    ReturnExpressionSyntax, ReturnExpressionSyntaxBuilder, WithExpressionSyntax,
    WithExpressionSyntaxBuilder, YieldExpressionSyntax, YieldExpressionSyntaxBuilder,
};
pub use generator::{
    GeneralGeneratorExpressionSyntax, GeneralGeneratorExpressionSyntaxBuilder,
    GeneratorIterationExpressionSyntax, GeneratorIterationExpressionSyntaxBuilder,
};
pub use lambda::{LambdaExpressionSyntax, LambdaExpressionSyntaxBuilder};
pub use literal::{
    AbsenceExpressionSyntax, AbsenceExpressionSyntaxBuilder, LeadingDotVariantExpressionSyntax,
    LeadingDotVariantExpressionSyntaxBuilder, LiteralExpressionSyntax,
    LiteralExpressionSyntaxBuilder, UnitExpressionSyntax, UnitExpressionSyntaxBuilder,
};
pub use looping::{
    ForExpressionSyntax, ForExpressionSyntaxBuilder, IterationSourceSyntax,
    IterationSourceSyntaxBuilder, LoopExpressionSyntax, LoopExpressionSyntaxBuilder,
    WhileExpressionSyntax, WhileExpressionSyntaxBuilder,
};
pub use r#match::{
    MatchArmSyntax, MatchArmSyntaxBuilder, MatchBodySyntax, MatchBodySyntaxBuilder,
    MatchExpressionSyntax, MatchExpressionSyntaxBuilder, MatchSubjectSyntax,
    MatchSubjectSyntaxBuilder,
};
pub use postfix::{
    ConversionOperationSyntax, ConversionOperationSyntaxBuilder,
    NullablePropagationOperationSyntax, NullablePropagationOperationSyntaxBuilder,
    SliceIndexOperationSyntax, SliceIndexOperationSyntaxBuilder,
};
pub use primary::{PrimaryExpressionSyntax, PrimaryExpressionSyntaxBuilder};
pub use root::{ExpressionSyntax, ExpressionSyntaxBuilder};
pub use type_form::{
    TypeFormConstructionExpressionSyntax, TypeFormConstructionExpressionSyntaxBuilder,
};

pub(super) use root::first_expression;
