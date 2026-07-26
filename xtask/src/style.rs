use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_source::{LineIndex, TextSize as SourceTextSize};
use ra_ap_syntax::{AstNode, AstToken};
use ra_ap_syntax::{
    Edition, NodeOrToken, SourceFile, SyntaxKind, SyntaxNode, SyntaxToken, TextRange, TextSize, ast,
};

use crate::{command, workspace};

const EXCLUDED_DIRECTORIES: [&str; 2] = [".git", "target"];

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        None => fix_workspace(),
        Some("check") => {
            command::reject_trailing_argument(arguments).and_then(|()| check_workspace())
        }
        Some(action) => Err(format!("unexpected style command: {action}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn fix_workspace() -> Result<(), String> {
    let root = workspace::root()?;
    let paths = rust_source_paths(&root)?;
    let mut fix_count = 0;

    for path in paths {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| workspace::io_error("read", &path, error))?;

        let (fixed, source_fix_count) = fix_source(&source)?;

        if source_fix_count == 0 {
            continue;
        }

        std::fs::write(&path, fixed).map_err(|error| workspace::io_error("write", &path, error))?;

        fix_count += source_fix_count;
    }

    if fix_count > 0 {
        eprintln!("fixed {fix_count} style violation(s)");
    }

    Ok(())
}

fn check_workspace() -> Result<(), String> {
    let root = workspace::root()?;
    let paths = rust_source_paths(&root)?;
    let mut violation_count = 0;

    for path in paths {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| workspace::io_error("read", &path, error))?;

        let relative_path = path.strip_prefix(&root).unwrap_or(&path);

        let line_index = LineIndex::new(&source).map_err(|error| {
            format!(
                "failed to index {}: {} bytes exceed source offset capacity",
                path.display(),
                error.bytes()
            )
        })?;

        for violation in check_source(&source) {
            let Some((line, column)) = source_location(&line_index, violation.offset) else {
                return Err(format!(
                    "style violation offset is outside {}",
                    path.display()
                ));
            };

            eprintln!(
                "{}:{line}:{column}: style/{}: {}",
                relative_path.display(),
                violation.rule.identifier(),
                violation.rule.message()
            );

            violation_count += 1;
        }
    }

    if violation_count == 0 {
        Ok(())
    } else {
        Err(format!(
            "style check failed with {violation_count} violation(s)"
        ))
    }
}

fn rust_source_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    collect_rust_source_paths(root, &mut paths)?;
    paths.sort();

    Ok(paths)
}

fn collect_rust_source_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| workspace::io_error("read directory", directory, error))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| workspace::io_error("read directory", directory, error))?;

        let file_type = entry
            .file_type()
            .map_err(|error| workspace::io_error("inspect", &entry.path(), error))?;

        let path = entry.path();

        if file_type.is_dir() {
            if !is_excluded_directory(&path) {
                collect_rust_source_paths(&path, paths)?;
            }
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            paths.push(path);
        }
    }

    Ok(())
}

fn is_excluded_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| EXCLUDED_DIRECTORIES.contains(&name))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Rule {
    BlankLineInList,
    CommentSeparation,
    DestructuringLetSeparation,
    MultilineStatementSeparation,
    MultipleBlankLines,
    LetElseSeparation,
}

impl Rule {
    fn identifier(self) -> &'static str {
        match self {
            Self::BlankLineInList => "blank-line-in-list",
            Self::CommentSeparation => "comment-separation",
            Self::DestructuringLetSeparation => "destructuring-let-separation",
            Self::LetElseSeparation => "let-else-separation",
            Self::MultilineStatementSeparation => "multiline-statement-separation",
            Self::MultipleBlankLines => "multiple-blank-lines",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::BlankLineInList => "blank lines are not allowed inside this list",
            Self::CommentSeparation => "a block-level comment must have a blank line above it",
            Self::DestructuringLetSeparation => {
                "a destructuring let statement must be separated from adjacent statements"
            }
            Self::LetElseSeparation => {
                "a let-else guard must be separated from adjacent statements"
            }
            Self::MultilineStatementSeparation => {
                "a multiline statement must be separated from adjacent statements"
            }
            Self::MultipleBlankLines => "use at most one consecutive blank line",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Violation {
    rule: Rule,
    offset: TextSize,
}

fn fix_source(source: &str) -> Result<(String, usize), String> {
    let mut fixed = source.to_owned();
    let mut fix_count = 0;

    loop {
        let Some(violation) = check_source(&fixed).into_iter().next() else {
            return Ok((fixed, fix_count));
        };

        apply_fix(&mut fixed, violation)?;
        fix_count += 1;
    }
}

fn apply_fix(source: &mut String, violation: Violation) -> Result<(), String> {
    match violation.rule {
        Rule::BlankLineInList | Rule::MultipleBlankLines => {
            remove_blank_line(source, violation.offset)
        }
        Rule::CommentSeparation
        | Rule::DestructuringLetSeparation
        | Rule::LetElseSeparation
        | Rule::MultilineStatementSeparation => {
            insert_blank_line(source, violation.offset);
            Ok(())
        }
    }
}

fn insert_blank_line(source: &mut String, offset: TextSize) {
    let offset = u32::from(offset) as usize;
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);

    let line_ending = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    source.insert_str(line_start, line_ending);
}

fn remove_blank_line(source: &mut String, offset: TextSize) -> Result<(), String> {
    let start = u32::from(offset) as usize;

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

fn check_source(source: &str) -> Vec<Violation> {
    let parse = SourceFile::parse(source, Edition::Edition2024);
    let mut violations = Vec::new();
    let syntax = parse.syntax_node();

    check_whitespace(&syntax, &mut violations);
    check_comments(&syntax, &mut violations);
    check_statement_lists(source, &syntax, &mut violations);
    violations.sort_by_key(|violation| (violation.offset, violation.rule));
    violations.dedup();

    violations
}

fn check_whitespace(syntax: &SyntaxNode, violations: &mut Vec<Violation>) {
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
            violations.push(Violation {
                rule: Rule::BlankLineInList,
                offset: offset_after_newline(&token, 1),
            });
        } else if newline_count >= 3 {
            violations.push(Violation {
                rule: Rule::MultipleBlankLines,
                offset: offset_after_newline(&token, 2),
            });
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

fn check_comments(syntax: &SyntaxNode, violations: &mut Vec<Violation>) {
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

        violations.push(Violation {
            rule: Rule::CommentSeparation,
            offset: token.text_range().start(),
        });
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
}

impl Separation {
    fn rule(self) -> Option<Rule> {
        match self {
            Self::None => None,
            Self::Multiline => Some(Rule::MultilineStatementSeparation),
            Self::DestructuringLet => Some(Rule::DestructuringLetSeparation),
            Self::LetElse => Some(Rule::LetElseSeparation),
        }
    }
}

#[derive(Clone, Copy)]
struct Statement {
    range: TextRange,
    separation: Separation,
}

fn check_statement_lists(source: &str, syntax: &SyntaxNode, violations: &mut Vec<Violation>) {
    for statement_list in syntax.descendants().filter_map(ast::StmtList::cast) {
        let mut statements = statement_list
            .statements()
            .map(|statement| statement_info(source, &statement))
            .collect::<Vec<_>>();

        if let Some(tail_expression) = statement_list.tail_expr() {
            statements.push(Statement {
                range: tail_expression.syntax().text_range(),
                separation: multiline_separation(source, tail_expression.syntax().text_range()),
            });
        }

        for pair in statements.windows(2) {
            check_statement_boundary(source, pair[0], pair[1], violations);
        }
    }
}

fn statement_info(source: &str, statement: &ast::Stmt) -> Statement {
    let range = statement.syntax().text_range();

    let separation = match statement {
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
    violations: &mut Vec<Violation>,
) {
    let separation = previous.separation.max(next.separation);

    let Some(rule) = separation.rule() else {
        return;
    };

    let gap = TextRange::new(previous.range.end(), next.range.start());

    if count_newlines(source_text(source, gap)) >= 2 {
        return;
    }

    violations.push(Violation {
        rule,
        offset: next.range.start(),
    });
}

fn source_text(source: &str, range: TextRange) -> &str {
    let start = u32::from(range.start()) as usize;
    let end = u32::from(range.end()) as usize;

    match source.get(start..end) {
        Some(text) => text,
        None => panic!("syntax ranges must refer to the parsed source"),
    }
}

fn count_newlines(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count()
}

fn source_location(line_index: &LineIndex, offset: TextSize) -> Option<(u32, u32)> {
    let location = line_index.line_column(SourceTextSize::new(u32::from(offset)))?;

    Some((location.line(), location.column()))
}

#[cfg(test)]
mod tests {
    use bray_source::LineIndex;

    use super::{Rule, check_source, check_workspace, fix_source, source_location};

    fn rules(source: &str) -> Vec<Rule> {
        check_source(source)
            .into_iter()
            .map(|violation| violation.rule)
            .collect()
    }

    #[test]
    fn compliant_source_has_no_violations() {
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
        let tuple_source = r#"
fn example() {
    let value = pair();
    let (first, second) = value;
    consume(first, second);
}
"#;

        assert_eq!(
            rules(tuple_source),
            [
                Rule::DestructuringLetSeparation,
                Rule::DestructuringLetSeparation
            ]
        );

        let slice_source = r#"
fn example() {
    let value = pair();
    let [first, second] = value;
    consume(first, second);
}
"#;

        assert_eq!(
            rules(slice_source),
            [
                Rule::DestructuringLetSeparation,
                Rule::DestructuringLetSeparation
            ]
        );
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
    fn line_endings_do_not_change_results() {
        let source = "fn example() {\n    let value = 1;\n\n    consume(value);\n}\n";
        let windows_source = source.replace('\n', "\r\n");

        assert_eq!(rules(source), rules(&windows_source));
    }

    #[test]
    fn fixes_are_complete_and_idempotent() {
        let source = "\
fn example() {
    let pair = pair();
    let (first, second) = pair;
    let [third, fourth] = array();
    consume(first, second, third, fourth);
}

enum Example {
    First,

    Second,
}
";

        let expected = "\
fn example() {
    let pair = pair();

    let (first, second) = pair;

    let [third, fourth] = array();

    consume(first, second, third, fourth);
}

enum Example {
    First,
    Second,
}
";

        let (fixed, fix_count) = match fix_source(source) {
            Ok(result) => result,
            Err(error) => panic!("source fixes should succeed: {error}"),
        };

        assert_eq!(fixed, expected);
        assert_eq!(fix_count, 4);
        assert_eq!(rules(&fixed), []);

        let (fixed_again, second_fix_count) = match fix_source(&fixed) {
            Ok(result) => result,
            Err(error) => panic!("checking fixed source should succeed: {error}"),
        };

        assert_eq!(fixed_again, fixed);
        assert_eq!(second_fix_count, 0);
    }

    #[test]
    fn fixes_preserve_windows_line_endings() {
        let source =
            "fn example() {\r\n    let pair = pair();\r\n    let (first, second) = pair;\r\n}\r\n";

        let expected = "fn example() {\r\n    let pair = pair();\r\n\r\n    let (first, second) = pair;\r\n}\r\n";

        let (fixed, fix_count) = match fix_source(source) {
            Ok(result) => result,
            Err(error) => panic!("source fixes should succeed: {error}"),
        };

        assert_eq!(fixed, expected);
        assert_eq!(fix_count, 1);
        assert!(!fixed.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn diagnostics_use_exact_source_locations() {
        let source = r#"
fn example() {
    let value = 1;
    // This comment is missing a blank line.
    consume(value);
}
"#;

        let line_index = match LineIndex::new(source) {
            Ok(line_index) => line_index,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        };

        let violations = check_source(source);

        let Some(violation) = violations.first() else {
            panic!("test source should produce a style violation");
        };

        assert_eq!(source_location(&line_index, violation.offset), Some((4, 5)));
    }

    #[test]
    fn workspace_sources_conform() {
        assert_eq!(check_workspace(), Ok(()));
    }
}
