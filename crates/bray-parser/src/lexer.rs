mod diagnostic;
mod literal;
mod scan;
mod scanner;
mod stream;
mod text;
mod token_source;
mod trivia;

pub use stream::{LexResult, lex_source_unit};
pub use token_source::{LexerCachePolicy, LexerTokenSource};
