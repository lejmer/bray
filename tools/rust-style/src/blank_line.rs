//! Checks and automatic fixes for mechanically decidable blank-line rules.

use ra_ap_syntax::{AstNode, AstToken};
use ra_ap_syntax::{NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, TextRange, TextSize, ast};

use super::diagnostic::{Diagnostic, Rule};
use super::source::{count_newlines, source_text, text_offset};

pub(super) fn fix_source(source: &str) -> Result<(String, usize), String> {
    let mut fixed = source.to_owned();
    let mut fix_count = 0;

    loop {
        let Some(diagnostic) = check_source(&fixed).into_iter().next() else {
            return Ok((fixed, fix_count));
        };

        apply_fix(&mut fixed, diagnostic)?;
        fix_count += 1;
    }
}

pub(super) fn check_source(source: &str) -> Vec<Diagnostic> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::Edition2024);
    let mut diagnostics = Vec::new();
    let syntax = parse.syntax_node();

    check_whitespace(&syntax, &mut diagnostics);
    check_comments(&syntax, &mut diagnostics);
    check_statement_lists(source, &syntax, &mut diagnostics);

    diagnostics.sort_by_key(|diagnostic| (diagnostic.offset, diagnostic.rule));
    diagnostics.dedup();

    diagnostics
}

fn apply_fix(source: &mut String, diagnostic: Diagnostic) -> Result<(), String> {
    match diagnostic.rule {
        Rule::BlankLineInList | Rule::MultipleBlankLines => {
            remove_blank_line(source, diagnostic.offset)
        }
        Rule::CommentSeparation
        | Rule::DestructuringLetSeparation
        | Rule::LetElseSeparation
        | Rule::MultilineStatementSeparation
        | Rule::ReturnedExpressionSeparation => {
            insert_blank_line(source, diagnostic.offset);

            Ok(())
        }
        _ => Err(format!(
            "style/{} cannot be fixed automatically",
            diagnostic.rule.identifier()
        )),
    }
}

fn insert_blank_line(source: &mut String, offset: TextSize) {
    let offset = text_offset(offset);
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);

    let line_ending = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    source.insert_str(line_start, line_ending);
}

fn remove_blank_line(source: &mut String, offset: TextSize) -> Result<(), String> {
    let start = text_offset(offset);

    let Some(relative_end) = source[start..].find('\n') else {
        return Err("blank-line violation has no terminating newline".to_owned());
    };

    let end = start + relative_end + 1;

    if !source[start..end].chars().all(char::is_whitespace) {
        return Err("blank-line violation does not cover a whitespace-only line".to_owned());
    }

    source.replace_range(start..end, "");

    Ok(())
}

fn check_whitespace(syntax: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    for token in syntax
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|token| token.kind() == SyntaxKind::WHITESPACE)
    {
        if token
            .parent_ancestors()
            .any(|parent| parent.kind() == SyntaxKind::TOKEN_TREE)
        {
            continue;
        }

        let newline_count = count_newlines(token.text());

        if newline_count >= 2 && is_disallowed_list_whitespace(&token) {
            diagnostics.push(Diagnostic::new(
                Rule::BlankLineInList,
                offset_after_newline(&token, 1),
            ));
        } else if newline_count >= 3 {
            diagnostics.push(Diagnostic::new(
                Rule::MultipleBlankLines,
                offset_after_newline(&token, 2),
            ));
        }
    }
}

fn is_disallowed_list_whitespace(token: &SyntaxToken) -> bool {
    token.parent().is_some_and(|parent| {
        matches!(
            parent.kind(),
            SyntaxKind::ARG_LIST
                | SyntaxKind::MATCH_ARM_LIST
                | SyntaxKind::PARAM_LIST
                | SyntaxKind::RECORD_EXPR_FIELD_LIST
                | SyntaxKind::VARIANT_LIST
        )
    })
}

fn offset_after_newline(token: &SyntaxToken, target: usize) -> TextSize {
    let mut newline_count = 0;

    for (index, byte) in token.text().bytes().enumerate() {
        if byte != b'\n' {
            continue;
        }

        newline_count += 1;

        if newline_count == target {
            let Ok(delta) = u32::try_from(index + 1) else {
                panic!("syntax token offsets must fit in TextSize");
            };

            return token.text_range().start() + TextSize::new(delta);
        }
    }

    token.text_range().start()
}

fn check_comments(syntax: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    for token in syntax
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
    {
        let Some(_comment) = ast::Comment::cast(token.clone()) else {
            continue;
        };

        if token
            .parent()
            .is_none_or(|parent| parent.kind() != SyntaxKind::STMT_LIST)
            || comment_is_separated(&token)
        {
            continue;
        }

        diagnostics.push(Diagnostic::new(
            Rule::CommentSeparation,
            token.text_range().start(),
        ));
    }
}

fn comment_is_separated(comment: &SyntaxToken) -> bool {
    let Some(previous) = comment.prev_token() else {
        return true;
    };

    if previous.text() == "{" {
        return true;
    }

    if previous.kind() != SyntaxKind::WHITESPACE {
        return true;
    }

    let newline_count = count_newlines(previous.text());

    if newline_count == 0 || newline_count >= 2 {
        return true;
    }

    previous
        .prev_token()
        .is_some_and(|token| token.text() == "{" || ast::Comment::cast(token).is_some())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Separation {
    None,
    Multiline,
    DestructuringLet,
    LetElse,
    ReturnedExpression,
}

impl Separation {
    fn rule(self) -> Option<Rule> {
        match self {
            Self::None => None,
            Self::Multiline => Some(Rule::MultilineStatementSeparation),
            Self::DestructuringLet => Some(Rule::DestructuringLetSeparation),
            Self::LetElse => Some(Rule::LetElseSeparation),
            Self::ReturnedExpression => Some(Rule::ReturnedExpressionSeparation),
        }
    }
}

#[derive(Clone, Copy)]
struct Statement {
    range: TextRange,
    separation: Separation,
}

fn check_statement_lists(source: &str, syntax: &SyntaxNode, diagnostics: &mut Vec<Diagnostic>) {
    for statement_list in syntax.descendants().filter_map(ast::StmtList::cast) {
        let mut statements = statement_list
            .statements()
            .map(|statement| statement_info(source, &statement))
            .collect::<Vec<_>>();

        if let Some(tail_expression) = statement_list.tail_expr() {
            statements.push(Statement {
                range: tail_expression.syntax().text_range(),
                separation: Separation::ReturnedExpression,
            });
        }

        for pair in statements.windows(2) {
            check_statement_boundary(source, pair[0], pair[1], diagnostics);
        }
    }
}

fn statement_info(source: &str, statement: &ast::Stmt) -> Statement {
    let range = statement.syntax().text_range();

    let separation = match statement {
        ast::Stmt::ExprStmt(expression_statement)
            if expression_statement.expr().is_some_and(|expression| {
                matches!(
                    expression,
                    ast::Expr::ReturnExpr(return_expression)
                        if return_expression.expr().is_some()
                )
            }) =>
        {
            Separation::ReturnedExpression
        }
        ast::Stmt::LetStmt(let_statement) if let_statement.let_else().is_some() => {
            Separation::LetElse
        }
        ast::Stmt::LetStmt(let_statement)
            if let_statement.pat().is_some_and(|pattern| {
                matches!(pattern, ast::Pat::SlicePat(_) | ast::Pat::TuplePat(_))
            }) =>
        {
            Separation::DestructuringLet
        }
        _ => multiline_separation(source, range),
    };

    Statement { range, separation }
}

fn multiline_separation(source: &str, range: TextRange) -> Separation {
    if source_text(source, range).contains('\n') {
        Separation::Multiline
    } else {
        Separation::None
    }
}

fn check_statement_boundary(
    source: &str,
    previous: Statement,
    next: Statement,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let separation = match (previous.separation, next.separation) {
        (_, Separation::ReturnedExpression) => Separation::ReturnedExpression,
        (Separation::ReturnedExpression, next) => next,
        (previous, next) => previous.max(next),
    };

    let Some(rule) = separation.rule() else {
        return;
    };

    let gap = TextRange::new(previous.range.end(), next.range.start());

    if count_newlines(source_text(source, gap)) >= 2 {
        return;
    }

    diagnostics.push(Diagnostic::new(rule, next.range.start()));
}

#[cfg(test)]
mod tests {
    use super::{check_source, fix_source};
    use crate::diagnostic::Rule;

    fn rules(source: &str) -> Vec<Rule> {
        check_source(source)
            .into_iter()
            .map(|diagnostic| diagnostic.rule)
            .collect()
    }

    #[test]
    fn compliant_source_has_no_diagnostics() {
        let source = r#"
fn example() {
    // The first comment starts the block.
    // This line continues the comment.
    let first = 1;
    let second = 2;

    let value = combine(
        first,
        second,
    );

    consume(value); // Inline comments do not need a paragraph boundary.
}
"#;

        assert_eq!(rules(source), []);
    }

    #[test]
    fn consecutive_blank_lines_are_rejected() {
        let source = "fn first() {}\n\n\nfn second() {}\n";

        assert_eq!(rules(source), [Rule::MultipleBlankLines]);
    }

    #[test]
    fn blank_lines_in_delimited_lists_are_rejected() {
        let cases = [
            "fn example(\n    first: usize,\n\n    second: usize,\n) {}",
            "fn example() { call(\n    first,\n\n    second,\n); }",
            "fn example() { let _ = Value {\n    first: 1,\n\n    second: 2,\n}; }",
            "enum Example {\n    First,\n\n    Second,\n}",
            "fn example(value: usize) { match value {\n    1 => first(),\n\n    _ => second(),\n} }",
        ];

        for source in cases {
            assert!(rules(source).contains(&Rule::BlankLineInList));
        }
    }

    #[test]
    fn block_level_comments_require_separation() {
        let source = r#"
fn example() {
    let value = 1;
    // This comment starts a new paragraph.
    consume(value);
}
"#;

        assert_eq!(rules(source), [Rule::CommentSeparation]);
    }

    #[test]
    fn multiline_statements_require_separation() {
        let source = r#"
fn example() {
    let value = combine(
        first,
        second,
    );
    consume(value);
}
"#;

        assert_eq!(rules(source), [Rule::MultilineStatementSeparation]);
    }

    #[test]
    fn destructuring_let_statements_require_separation() {
        let cases = [
            r#"
fn example() {
    let value = pair();
    let (first, second) = value;
    consume(first, second);
}
"#,
            r#"
fn example() {
    let value = pair();
    let [first, second] = value;
    consume(first, second);
}
"#,
        ];

        for source in cases {
            assert_eq!(
                rules(source),
                [
                    Rule::DestructuringLetSeparation,
                    Rule::DestructuringLetSeparation
                ]
            );
        }
    }

    #[test]
    fn let_else_guards_require_separation() {
        let source = r#"
fn example(value: Option<usize>) {
    let input = value;
    let Some(output) = input else {
        return;
    };
    consume(output);
}
"#;

        assert_eq!(
            rules(source),
            [Rule::LetElseSeparation, Rule::LetElseSeparation]
        );
    }

    #[test]
    fn explicit_and_implicit_returns_require_separation() {
        let explicit = "fn example() -> usize {\n    let value = 1;\n    return value;\n}\n";
        let implicit = "fn example() -> usize {\n    let value = 1;\n    value\n}\n";

        assert_eq!(rules(explicit), [Rule::ReturnedExpressionSeparation]);
        assert_eq!(rules(implicit), [Rule::ReturnedExpressionSeparation]);
    }

    #[test]
    fn a_single_returned_expression_needs_no_boundary() {
        let explicit = "fn example() -> usize {\n    return 1;\n}\n";
        let implicit = "fn example() -> usize {\n    1\n}\n";

        assert_eq!(rules(explicit), []);
        assert_eq!(rules(implicit), []);
    }

    #[test]
    fn returned_expression_boundaries_apply_inside_nested_blocks() {
        let source = "\
fn example(condition: bool) -> usize {
    if condition {
        prepare();
        1
    } else {
        prepare();
        return 2;
    }
}
";

        assert_eq!(
            rules(source),
            [
                Rule::ReturnedExpressionSeparation,
                Rule::ReturnedExpressionSeparation
            ]
        );
    }

    #[test]
    fn returned_expression_comments_remain_attached() {
        let implicit_source = "\
fn example() -> usize {
    let value = 1;
    // Return the prepared value.
    value
}
";

        let implicit_expected = "\
fn example() -> usize {
    let value = 1;

    // Return the prepared value.
    value
}
";

        let explicit_source = "\
fn example() -> usize {
    let value = 1;
    // Return the prepared value.
    return value;
}
";

        let explicit_expected = "\
fn example() -> usize {
    let value = 1;

    // Return the prepared value.
    return value;
}
";

        let (implicit_fixed, _) = fixed_source(implicit_source);

        let (explicit_fixed, _) = fixed_source(explicit_source);

        assert_eq!(implicit_fixed, implicit_expected);
        assert_eq!(explicit_fixed, explicit_expected);
    }

    #[test]
    fn explicit_returns_only_require_a_preceding_boundary() {
        let source = "\
fn example() -> usize {
    prepare();
    return 1;
    unreachable();
}
";

        assert_eq!(rules(source), [Rule::ReturnedExpressionSeparation]);
    }

    #[test]
    fn macro_bodies_and_raw_strings_are_not_treated_as_source_trivia() {
        let source = r##"
macro_rules! example {
    () => {
        first();


        second();
    };
}

fn use_macro() {
    let text = r#"


// not a source comment
"#;

    example!();
    consume(text);
}
"##;

        assert_eq!(rules(source), []);
    }

    #[test]
    fn fixes_are_complete_and_idempotent() {
        let source = "\
fn example() -> usize {
    let pair = pair();
    let (first, second) = pair;
    consume(first, second);
    first
}

enum Example {
    First,

    Second,
}
";

        let expected = "\
fn example() -> usize {
    let pair = pair();

    let (first, second) = pair;

    consume(first, second);

    first
}

enum Example {
    First,
    Second,
}
";

        let (fixed, fix_count) = fixed_source(source);

        assert_eq!(fixed, expected);
        assert_eq!(fix_count, 4);
        assert_eq!(rules(&fixed), []);

        let (fixed_again, second_fix_count) = fixed_source(&fixed);

        assert_eq!(fixed_again, fixed);
        assert_eq!(second_fix_count, 0);
    }

    #[test]
    fn fixes_preserve_windows_line_endings() {
        let source = "fn example() -> usize {\r\n    let value = 1;\r\n    value\r\n}\r\n";
        let expected = "fn example() -> usize {\r\n    let value = 1;\r\n\r\n    value\r\n}\r\n";

        let (fixed, fix_count) = fixed_source(source);

        assert_eq!(fixed, expected);
        assert_eq!(fix_count, 1);
        assert!(!fixed.replace("\r\n", "").contains('\n'));
    }

    fn fixed_source(source: &str) -> (String, usize) {
        match fix_source(source) {
            Ok(result) => result,
            Err(error) => panic!("source fixes should succeed: {error}"),
        }
    }
}
