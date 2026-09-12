use super::SyntaxKind;

macro_rules! fixed_syntax_text {
    ($( $text:literal => $kind:ident, )+) => {
        impl SyntaxKind {
            /// Returns this fixed token's exact Bray source spelling.
            pub const fn fixed_text(self) -> Option<&'static str> {
                match self {
                    $(Self::$kind => Some($text),)+
                    _ => None,
                }
            }

            /// Classifies an exact fixed Bray token spelling.
            pub fn from_fixed_text(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$kind),)+
                    _ => None,
                }
            }

            /// Classifies one fixed token character without allocating source text.
            pub fn from_fixed_character(character: char) -> Option<Self> {
                let mut encoded = [0_u8; 4];

                Self::from_fixed_text(character.encode_utf8(&mut encoded))
            }
        }
    };
}

fixed_syntax_text! {
    "all" => AllKeyword,
    "any" => AnyKeyword,
    "as" => AsKeyword,
    "assert" => AssertKeyword,
    "async" => AsyncKeyword,
    "await" => AwaitKeyword,
    "box" => BoxKeyword,
    "break" => BreakKeyword,
    "callable" => CallableKeyword,
    "case" => CaseKeyword,
    "catch" => CatchKeyword,
    "const" => ConstKeyword,
    "construct" => ConstructKeyword,
    "consume" => ConsumeKeyword,
    "continue" => ContinueKeyword,
    "destruct" => DestructKeyword,
    "each" => EachKeyword,
    "else" => ElseKeyword,
    "ensures" => EnsuresKeyword,
    "executes" => ExecutesKeyword,
    "enter" => EnterKeyword,
    "exit" => ExitKeyword,
    "export" => ExportKeyword,
    "extern" => ExternKeyword,
    "false" => FalseKeyword,
    "finalize" => FinalizeKeyword,
    "for" => ForKeyword,
    "func" => FuncKeyword,
    "if" => IfKeyword,
    "impl" => ImplKeyword,
    "in" => InKeyword,
    "internal" => InternalKeyword,
    "lambda" => LambdaKeyword,
    "let" => LetKeyword,
    "loop" => LoopKeyword,
    "match" => MatchKeyword,
    "matches" => MatchesKeyword,
    "module" => ModuleKeyword,
    "move" => MoveKeyword,
    "mut" => MutKeyword,
    "none" => NoneKeyword,
    "overload" => OverloadKeyword,
    "panic" => PanicKeyword,
    "pos" => PosKeyword,
    "predicate" => PredicateKeyword,
    "public" => PublicKeyword,
    "requires" => RequiresKeyword,
    "return" => ReturnKeyword,
    "self" => SelfValueKeyword,
    "Self" => SelfTypeKeyword,
    "static" => StaticKeyword,
    "struct" => StructKeyword,
    "trait" => TraitKeyword,
    "trusted" => TrustedKeyword,
    "true" => TrueKeyword,
    "try" => TryKeyword,
    "type" => TypeKeyword,
    "union" => UnionKeyword,
    "unit" => UnitKeyword,
    "using" => UsingKeyword,
    "uses" => UsesKeyword,
    "view" => ViewKeyword,
    "when" => WhenKeyword,
    "while" => WhileKeyword,
    "with" => WithKeyword,
    "yield" => YieldKeyword,
    "->" => ArrowToken,
    "==" => EqualsEqualsToken,
    "!=" => BangEqualsToken,
    "<=" => LessEqualsToken,
    ">=" => GreaterEqualsToken,
    "&&" => AmpersandAmpersandToken,
    "||" => PipePipeToken,
    "<<" => LessLessToken,
    ">>" => GreaterGreaterToken,
    "**" => StarStarToken,
    "+=" => PlusEqualsToken,
    "-=" => MinusEqualsToken,
    "*=" => StarEqualsToken,
    "/=" => SlashEqualsToken,
    "%=" => PercentEqualsToken,
    "@=" => AtEqualsToken,
    "&=" => AmpersandEqualsToken,
    "|=" => PipeEqualsToken,
    "^=" => CaretEqualsToken,
    "<<=" => LessLessEqualsToken,
    ">>=" => GreaterGreaterEqualsToken,
    "**=" => StarStarEqualsToken,
    "..." => EllipsisToken,
    ".." => DotDotToken,
    ":" => ColonToken,
    "." => DotToken,
    "?" => QuestionToken,
    "=" => EqualsToken,
    "+" => PlusToken,
    "-" => MinusToken,
    "*" => StarToken,
    "/" => SlashToken,
    "%" => PercentToken,
    "@" => AtToken,
    "&" => AmpersandToken,
    "|" => PipeToken,
    "^" => CaretToken,
    "~" => TildeToken,
    "!" => BangToken,
    "<" => LessToken,
    ">" => GreaterToken,
    "(" => OpenParenToken,
    ")" => CloseParenToken,
    "{" => OpenBraceToken,
    "}" => CloseBraceToken,
    "[" => OpenBracketToken,
    "]" => CloseBracketToken,
    "," => CommaToken,
    ";" => SemicolonToken,
    "_" => UnderscoreToken,
}

#[cfg(test)]
mod tests {
    use super::SyntaxKind;

    #[test]
    fn fixed_syntax_text_round_trips() {
        let cases = [
            (SyntaxKind::FuncKeyword, "func"),
            (SyntaxKind::CloseParenToken, ")"),
            (SyntaxKind::ArrowToken, "->"),
            (SyntaxKind::UnderscoreToken, "_"),
        ];

        for (kind, text) in cases {
            assert_eq!(kind.fixed_text(), Some(text));
            assert_eq!(SyntaxKind::from_fixed_text(text), Some(kind));
        }

        assert_eq!(SyntaxKind::IdentifierToken.fixed_text(), None);
        assert_eq!(SyntaxKind::from_fixed_text("identifier"), None);
    }
}
