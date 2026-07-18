use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundLiteralExpression, BoundLiteralKind,
};
use bray_syntax::{LiteralExpressionSyntax, SyntaxKind};

use super::super::BindingResult;
use super::ExpressionBinder;
use crate::BinderFactContext;
use crate::binder::Binder;

impl ExpressionBinder {
    pub(super) fn bind_literal<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        syntax: &LiteralExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let Some(token) = syntax.literal_token() else {
            return self.push_error(binder, Some(syntax));
        };

        let Some(kind) = literal_kind(token.kind()) else {
            return self.push_error(binder, Some(syntax));
        };

        self.push(
            binder,
            BoundExpression::Literal(BoundLiteralExpression::new(
                binder.source_origin(syntax),
                kind,
                None,
                syntax.is_recovered(),
            )),
        )
    }
}

fn literal_kind(kind: SyntaxKind) -> Option<BoundLiteralKind> {
    Some(match kind {
        SyntaxKind::DecimalIntegerLiteralToken
        | SyntaxKind::BinaryIntegerLiteralToken
        | SyntaxKind::HexadecimalIntegerLiteralToken => BoundLiteralKind::Integer,
        SyntaxKind::RealLiteralToken => BoundLiteralKind::Real,
        SyntaxKind::ImaginaryLiteralToken => BoundLiteralKind::Imaginary,
        SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => BoundLiteralKind::Boolean,
        SyntaxKind::CharacterLiteralToken => BoundLiteralKind::Character,
        SyntaxKind::StringLiteralToken => BoundLiteralKind::String,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundLiteralKind;
    use bray_syntax::SyntaxKind;

    use super::literal_kind;

    #[test]
    fn syntax_literal_tokens_map_to_semantic_literal_categories() {
        let cases = [
            (
                SyntaxKind::DecimalIntegerLiteralToken,
                BoundLiteralKind::Integer,
            ),
            (
                SyntaxKind::BinaryIntegerLiteralToken,
                BoundLiteralKind::Integer,
            ),
            (
                SyntaxKind::HexadecimalIntegerLiteralToken,
                BoundLiteralKind::Integer,
            ),
            (SyntaxKind::RealLiteralToken, BoundLiteralKind::Real),
            (
                SyntaxKind::ImaginaryLiteralToken,
                BoundLiteralKind::Imaginary,
            ),
            (SyntaxKind::TrueKeyword, BoundLiteralKind::Boolean),
            (SyntaxKind::FalseKeyword, BoundLiteralKind::Boolean),
            (
                SyntaxKind::CharacterLiteralToken,
                BoundLiteralKind::Character,
            ),
            (SyntaxKind::StringLiteralToken, BoundLiteralKind::String),
        ];

        for (syntax, expected) in cases {
            assert_eq!(literal_kind(syntax), Some(expected));
        }

        assert_eq!(literal_kind(SyntaxKind::IdentifierToken), None);
    }
}
