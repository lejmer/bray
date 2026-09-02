//! Parses and applies narrowly scoped, source-local rule exemptions.

use ra_ap_syntax::ast::HasModuleItem;
use ra_ap_syntax::{AstNode, AstToken, NodeOrToken, SyntaxNode, TextRange, TextSize, ast};

use super::diagnostic::{Diagnostic, ExemptionScope, Rule, Target};
use super::source::source_text;

pub(super) fn apply(
    source: &str,
    file: &ast::SourceFile,
    diagnostics: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    let (mut exemptions, mut exemption_diagnostics) = parse(source, file);

    let mut retained = Vec::new();

    for diagnostic in diagnostics {
        let mut suppressed = false;

        for exemption in &mut exemptions {
            if exemption.matches(&diagnostic) {
                exemption.used = true;
                suppressed = true;
            }
        }

        if !suppressed {
            retained.push(diagnostic);
        }
    }

    exemption_diagnostics.extend(
        exemptions
            .into_iter()
            .filter_map(Exemption::unused_diagnostic),
    );

    retained.extend(exemption_diagnostics);

    retained
}

#[derive(Clone, Copy)]
struct Exemption {
    rule: Rule,
    offset: TextSize,
    target: Target,
    used: bool,
}

impl Exemption {
    fn matches(&self, diagnostic: &Diagnostic) -> bool {
        if self.rule != diagnostic.rule {
            return false;
        }

        match (self.target, diagnostic.target) {
            (Target::Expression(exemption), Target::Expression(diagnostic)) => {
                exemption.contains_range(diagnostic)
            }
            (exemption, diagnostic) => exemption == diagnostic,
        }
    }

    fn unused_diagnostic(self) -> Option<Diagnostic> {
        (!self.used).then(|| {
            Diagnostic::new(Rule::UnusedExemption, self.offset).with_message(format!(
                "style/{} exemption does not suppress a diagnostic",
                self.rule.identifier()
            ))
        })
    }
}

fn parse(source: &str, file: &ast::SourceFile) -> (Vec<Exemption>, Vec<Diagnostic>) {
    let first_item_start = file
        .items()
        .next()
        .and_then(|item| significant_start(item.syntax()));

    let syntax = file.syntax();
    let mut exemptions = Vec::new();
    let mut diagnostics = Vec::new();

    for token in syntax
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
    {
        let Some(comment) = ast::Comment::cast(token.clone()) else {
            continue;
        };

        let Some(directive) = directive_text(comment.text()) else {
            continue;
        };

        if directive.starts_with("broad-failure") {
            continue;
        }

        let offset = token.text_range().start();

        let rule = match parse_rule(directive) {
            Ok(rule) => rule,
            Err(message) => {
                diagnostics
                    .push(Diagnostic::new(Rule::InvalidExemption, offset).with_message(message));

                continue;
            }
        };

        let target = match rule.exemption_scope() {
            Some(ExemptionScope::Expression) => {
                let Some(range) =
                    adjacent_expression_range(source, syntax, token.text_range().end())
                else {
                    diagnostics.push(
                        Diagnostic::new(Rule::InvalidExemption, offset).with_message(format!(
                            "style/{} exemption must be immediately before the affected expression",
                            rule.identifier()
                        )),
                    );

                    continue;
                };

                Target::Expression(range)
            }
            Some(ExemptionScope::File)
                if first_item_start.is_none_or(|start| token.text_range().end() <= start) =>
            {
                Target::File
            }
            Some(ExemptionScope::Item) => {
                let Some(range) =
                    adjacent_item_range(rule, source, syntax, token.text_range().end())
                else {
                    diagnostics.push(
                        Diagnostic::new(Rule::InvalidExemption, offset).with_message(format!(
                            "style/{} exemption must be immediately before the affected item",
                            rule.identifier()
                        )),
                    );

                    continue;
                };

                Target::Item(range)
            }
            Some(ExemptionScope::File) => {
                diagnostics.push(
                    Diagnostic::new(Rule::InvalidExemption, offset).with_message(format!(
                        "style/{} exemption must appear before the first item in the file",
                        rule.identifier()
                    )),
                );

                continue;
            }
            None => {
                diagnostics.push(
                    Diagnostic::new(Rule::InvalidExemption, offset).with_message(format!(
                        "style/{} does not support source exemptions",
                        rule.identifier()
                    )),
                );

                continue;
            }
        };

        exemptions.push(Exemption {
            rule,
            offset,
            target,
            used: false,
        });
    }

    (exemptions, diagnostics)
}

fn adjacent_expression_range(
    source: &str,
    syntax: &SyntaxNode,
    comment_end: TextSize,
) -> Option<TextRange> {
    syntax
        .descendants()
        .filter_map(ast::Expr::cast)
        .map(|expression| expression.syntax().text_range())
        .filter_map(|range| {
            let node = syntax.covering_element(range).into_node()?;
            let start = significant_start(&node)?;

            (start >= comment_end).then_some((range, start))
        })
        .filter(|(_, start)| {
            source_text(source, TextRange::new(comment_end, *start))
                .chars()
                .all(char::is_whitespace)
        })
        .min_by_key(|(range, start)| (*start, std::cmp::Reverse(range.end())))
        .map(|(range, _)| range)
}

fn directive_text(comment: &str) -> Option<&str> {
    let body = comment.strip_prefix("//")?.trim();

    body.strip_prefix("rust-style:").map(str::trim)
}

fn parse_rule(directive: &str) -> Result<Rule, String> {
    let Some(arguments) = directive
        .strip_prefix("allow(")
        .and_then(|arguments| arguments.strip_suffix(')'))
    else {
        return Err("style exemption must use allow(rule, reason = \"justification\")".to_owned());
    };

    let Some((identifier, reason)) = arguments.split_once(", reason = ") else {
        return Err("style exemption must include a reason".to_owned());
    };

    let identifier = identifier.trim();

    let Some(rule) = Rule::from_identifier(identifier) else {
        return Err(format!("unknown style exemption rule: {identifier}"));
    };

    let Some(reason) = reason
        .trim()
        .strip_prefix('"')
        .and_then(|reason| reason.strip_suffix('"'))
    else {
        return Err("style exemption reason must be a quoted string".to_owned());
    };

    if reason.trim().is_empty() {
        return Err("style exemption reason must not be empty".to_owned());
    }

    Ok(rule)
}

fn adjacent_item_range(
    rule: Rule,
    source: &str,
    syntax: &SyntaxNode,
    comment_end: TextSize,
) -> Option<TextRange> {
    let ranges = match rule {
        Rule::FunctionTooLarge => syntax
            .descendants()
            .filter_map(ast::Fn::cast)
            .map(|function| function.syntax().text_range())
            .collect::<Vec<_>>(),
        Rule::WildcardImport => syntax
            .descendants()
            .filter_map(ast::Use::cast)
            .map(|use_item| use_item.syntax().text_range())
            .collect::<Vec<_>>(),
        Rule::NonThinLibRoot | Rule::NonThinModuleRoot => ast::SourceFile::cast(syntax.clone())?
            .items()
            .map(|item| item.syntax().text_range())
            .collect::<Vec<_>>(),
        _ => return None,
    };

    ranges
        .into_iter()
        .filter_map(|range| {
            let node = syntax.covering_element(range).into_node()?;
            let start = significant_start(&node)?;

            (start >= comment_end).then_some((range, start))
        })
        .min_by_key(|(_, start)| *start)
        .filter(|(_, start)| {
            source_text(source, TextRange::new(comment_end, *start))
                .chars()
                .all(char::is_whitespace)
        })
        .map(|(range, _)| range)
}

fn significant_start(node: &SyntaxNode) -> Option<TextSize> {
    node.descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .find(|token| !token.kind().is_trivia())
        .map(|token| token.text_range().start())
}

#[cfg(test)]
mod tests {
    use ra_ap_syntax::{AstNode, Edition, SourceFile, TextSize};

    use super::apply;
    use crate::diagnostic::{Diagnostic, Rule, Target};

    #[test]
    fn file_exemptions_require_reasons_and_suppress_only_the_named_rule() {
        let source = "\
// rust-style: allow(module-too-large, reason = \"flat catalog\")
fn example() {}
";

        let file = SourceFile::parse(source, Edition::Edition2024).tree();

        let diagnostics = vec![
            Diagnostic::new(Rule::ModuleTooLarge, TextSize::new(0)).for_file(),
            Diagnostic::new(Rule::RepeatedModulePrefix, TextSize::new(0)).for_file(),
        ];

        let retained = apply(source, &file, diagnostics);

        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].rule, Rule::RepeatedModulePrefix);
    }

    #[test]
    fn item_exemptions_apply_only_to_the_adjacent_item() {
        let source = "\
// rust-style: allow(wildcard-import, reason = \"macro-defined names\")
pub use generated::*;
pub use ordinary::*;
";

        let file = SourceFile::parse(source, Edition::Edition2024).tree();

        let uses = file
            .syntax()
            .descendants()
            .filter_map(ra_ap_syntax::ast::Use::cast)
            .collect::<Vec<_>>();

        let diagnostics = uses
            .iter()
            .map(|use_item| {
                Diagnostic::new(Rule::WildcardImport, use_item.syntax().text_range().start())
                    .for_item(use_item.syntax().text_range())
            })
            .collect();

        let retained = apply(source, &file, diagnostics);

        assert_eq!(retained.len(), 1);

        assert_eq!(
            retained[0].target,
            Target::Item(uses[1].syntax().text_range())
        );
    }

    #[test]
    fn malformed_and_unused_exemptions_are_reported() {
        let source = "\
// rust-style: allow(module-too-large)
// rust-style: allow(repeated-module-prefix, reason = \"renaming would obscure the concept\")
fn example() {}
";

        let file = SourceFile::parse(source, Edition::Edition2024).tree();
        let retained = apply(source, &file, Vec::new());

        let rules = retained
            .into_iter()
            .map(|diagnostic| diagnostic.rule)
            .collect::<Vec<_>>();

        assert_eq!(rules, [Rule::InvalidExemption, Rule::UnusedExemption]);
    }

    #[test]
    fn unknown_empty_and_misplaced_exemptions_are_errors() {
        let source = "\
// rust-style: allow(unknown-rule, reason = \"unknown\")
// rust-style: allow(module-too-large, reason = \"\")
fn first() {}
// rust-style: allow(module-too-large, reason = \"too late\")
fn second() {}
// rust-style: allow(function-too-large, reason = \"not adjacent\")
const VALUE: usize = 0;
fn third() {}
";

        let file = SourceFile::parse(source, Edition::Edition2024).tree();
        let retained = apply(source, &file, Vec::new());

        assert_eq!(
            retained
                .into_iter()
                .map(|diagnostic| diagnostic.rule)
                .collect::<Vec<_>>(),
            [
                Rule::InvalidExemption,
                Rule::InvalidExemption,
                Rule::InvalidExemption,
                Rule::InvalidExemption
            ]
        );
    }
}
