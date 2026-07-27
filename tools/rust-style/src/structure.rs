//! Check-only structural rules for Rust modules and functions.

use std::path::Path;

use ra_ap_syntax::ast::{HasModuleItem, HasVisibility};
use ra_ap_syntax::{AstNode, TextSize, ast};

use super::diagnostic::{Diagnostic, Rule};
use super::source::{
    line_offsets_in_range, production_line_offsets, range_is_test_only, test_only_ranges,
};

const MODULE_LINE_WARNING: usize = 800;
const FUNCTION_LINE_LIMIT: usize = 250;

pub(super) fn check(path: &Path, source: &str, file: &ast::SourceFile) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let test_ranges = test_only_ranges(file, source);

    if !is_test_source(path) {
        check_module_size(source, &test_ranges, &mut diagnostics);
        check_function_sizes(source, file, &test_ranges, &mut diagnostics);
    }

    check_legacy_layout(path, &mut diagnostics);
    check_repeated_module_prefix(path, &mut diagnostics);
    check_thin_roots(path, file, &mut diagnostics);
    check_wildcard_imports(file, &mut diagnostics);

    diagnostics
}

fn is_test_source(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "tests.rs")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

fn check_module_size(
    source: &str,
    test_ranges: &[ra_ap_syntax::TextRange],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let offsets = production_line_offsets(source, test_ranges);

    if offsets.len() <= MODULE_LINE_WARNING {
        return;
    }

    diagnostics.push(
        Diagnostic::new(Rule::ModuleTooLarge, offsets[MODULE_LINE_WARNING])
            .with_message(format!(
                "module contains {} production lines; warning threshold is {MODULE_LINE_WARNING}",
                offsets.len()
            ))
            .with_help(
                "move implementation into focused files under a same-named directory and keep \
                 the original file as a thin root containing only external module declarations \
                 and visible re-exports",
            )
            .for_file(),
    );
}

fn check_function_sizes(
    source: &str,
    file: &ast::SourceFile,
    test_ranges: &[ra_ap_syntax::TextRange],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for function in file.syntax().descendants().filter_map(ast::Fn::cast) {
        let range = function.syntax().text_range();

        if range_is_test_only(range, test_ranges) {
            continue;
        }

        let offsets = line_offsets_in_range(source, range);

        if offsets.len() <= FUNCTION_LINE_LIMIT {
            continue;
        }

        diagnostics.push(
            Diagnostic::new(Rule::FunctionTooLarge, offsets[FUNCTION_LINE_LIMIT])
                .with_message(format!(
                    "function contains {} production lines; limit is {FUNCTION_LINE_LIMIT}",
                    offsets.len()
                ))
                .with_help(
                    "extract focused helpers or move distinct behavior into the module that owns \
                     it",
                )
                .for_item(range),
        );
    }
}

fn check_legacy_layout(path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    if path.file_name().is_some_and(|name| name == "mod.rs") {
        diagnostics.push(
            Diagnostic::new(Rule::LegacyModRs, TextSize::new(0))
                .with_help(
                    "use the modern layout: store module `foo` in foo.rs and its children under \
                     foo/",
                )
                .for_file(),
        );
    }
}

fn check_repeated_module_prefix(path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let Some(parent) = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
    else {
        return;
    };

    if parent == "src" {
        return;
    }

    let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
        return;
    };

    if stem.starts_with(&format!("{parent}_")) {
        diagnostics.push(
            Diagnostic::new(Rule::RepeatedModulePrefix, TextSize::new(0))
                .with_message(format!(
                    "submodule filename `{stem}` repeats parent module `{parent}`"
                ))
                .with_help(
                    "name the file for its local concept; the containing directory already \
                     supplies the parent module context",
                )
                .for_file(),
        );
    }
}

fn check_thin_roots(path: &Path, file: &ast::SourceFile, diagnostics: &mut Vec<Diagnostic>) {
    if path.file_name().is_some_and(|name| name == "lib.rs") {
        check_thin_items(file, Rule::NonThinLibRoot, diagnostics);
    } else if path.with_extension("").is_dir() {
        check_thin_items(file, Rule::NonThinModuleRoot, diagnostics);
    }
}

fn check_thin_items(file: &ast::SourceFile, rule: Rule, diagnostics: &mut Vec<Diagnostic>) {
    for item in file.items().filter(|item| !is_thin_root_item(item)) {
        let range = item.syntax().text_range();

        let help = match rule {
            Rule::NonThinLibRoot => {
                "move this implementation into its owning module and re-export the public \
                 contract from lib.rs"
            }
            Rule::NonThinModuleRoot => {
                "move this item into a focused file under the same-named directory and re-export \
                 it from the thin module root"
            }
            _ => panic!("thin-root checks require a thin-root rule"),
        };

        diagnostics.push(
            Diagnostic::new(rule, range.start())
                .with_help(help)
                .for_item(range),
        );
    }
}

fn is_thin_root_item(item: &ast::Item) -> bool {
    match item {
        ast::Item::Module(module) => module.semicolon_token().is_some(),
        ast::Item::Use(use_item) => use_item.visibility().is_some(),
        _ => false,
    }
}

fn check_wildcard_imports(file: &ast::SourceFile, diagnostics: &mut Vec<Diagnostic>) {
    for use_item in file.syntax().descendants().filter_map(ast::Use::cast) {
        let range = use_item.syntax().text_range();

        for wildcard in use_item
            .syntax()
            .descendants()
            .filter_map(ast::UseTree::cast)
            .filter_map(|tree| tree.star_token())
        {
            diagnostics.push(
                Diagnostic::new(Rule::WildcardImport, wildcard.text_range().start())
                    .with_help(
                        "name every imported or re-exported symbol explicitly; add a justified \
                         source exemption only when generated or macro-defined names make an \
                         explicit list unreasonable to maintain",
                    )
                    .for_item(range),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ra_ap_syntax::{Edition, SourceFile};

    use super::{FUNCTION_LINE_LIMIT, MODULE_LINE_WARNING, check, check_thin_items};
    use crate::diagnostic::Rule;

    fn rules(path: &Path, source: &str) -> Vec<Rule> {
        let file = SourceFile::parse(source, Edition::Edition2024).tree();

        check(path, source, &file)
            .into_iter()
            .map(|diagnostic| diagnostic.rule)
            .collect()
    }

    #[test]
    fn module_size_uses_a_warning_boundary_and_excludes_tests() {
        let mut source = "const VALUE: usize = 0;\n".repeat(MODULE_LINE_WARNING);
        source.push_str("#[cfg(test)]\nmod tests {\n");
        source.push_str(&"const TEST_VALUE: usize = 0;\n".repeat(20));
        source.push_str("}\n");

        assert!(!rules(Path::new("example.rs"), &source).contains(&Rule::ModuleTooLarge));

        source.insert_str(0, "const EXTRA: usize = 0;\n");

        assert!(rules(Path::new("example.rs"), &source).contains(&Rule::ModuleTooLarge));
    }

    #[test]
    fn production_functions_have_a_hard_line_limit() {
        let boundary_body = "    consume();\n".repeat(FUNCTION_LINE_LIMIT - 2);
        let boundary_source = format!("fn example() {{\n{boundary_body}}}\n");

        assert!(
            !rules(Path::new("example.rs"), &boundary_source).contains(&Rule::FunctionTooLarge)
        );

        let oversized_body = "    consume();\n".repeat(FUNCTION_LINE_LIMIT - 1);
        let source = format!("fn example() {{\n{oversized_body}}}\n");

        assert!(rules(Path::new("example.rs"), &source).contains(&Rule::FunctionTooLarge));

        let test_source = format!("#[cfg(test)]\nfn example() {{\n{oversized_body}}}\n");

        assert!(!rules(Path::new("example.rs"), &test_source).contains(&Rule::FunctionTooLarge));

        assert!(
            !rules(Path::new("tests/integration.rs"), &source).contains(&Rule::FunctionTooLarge)
        );

        assert!(
            !rules(Path::new("src/example/tests.rs"), &source).contains(&Rule::FunctionTooLarge)
        );
    }

    #[test]
    fn thin_roots_allow_only_external_modules_and_visible_reexports() {
        let compliant = "\
/// Child implementation.
#[cfg(any(unix, windows))]
mod child;
pub(crate) mod visible_child;
pub use child::Thing;
pub(crate) use child::Helper;
";

        let invalid = "\
mod child;
use child::Thing;
#[allow(dead_code)]
fn implementation() {}
mod inline {}
macro_rules! helper {
    () => {};
}
";

        let compliant_file = SourceFile::parse(compliant, Edition::Edition2024).tree();
        let invalid_file = SourceFile::parse(invalid, Edition::Edition2024).tree();
        let mut compliant_diagnostics = Vec::new();
        let mut invalid_diagnostics = Vec::new();

        check_thin_items(
            &compliant_file,
            Rule::NonThinModuleRoot,
            &mut compliant_diagnostics,
        );

        check_thin_items(
            &invalid_file,
            Rule::NonThinModuleRoot,
            &mut invalid_diagnostics,
        );

        assert_eq!(compliant_diagnostics, []);
        assert_eq!(invalid_diagnostics.len(), 4);

        assert_eq!(
            rules(Path::new("lib.rs"), invalid)
                .into_iter()
                .filter(|rule| *rule == Rule::NonThinLibRoot)
                .count(),
            4
        );
    }

    #[test]
    fn legacy_layout_and_parent_prefixes_are_diagnosed() {
        assert!(rules(Path::new("foo/mod.rs"), "").contains(&Rule::LegacyModRs));
        assert!(rules(Path::new("foo/foo_parser.rs"), "").contains(&Rule::RepeatedModulePrefix));
        assert!(!rules(Path::new("src/src_parser.rs"), "").contains(&Rule::RepeatedModulePrefix));
    }

    #[test]
    fn wildcard_imports_and_reexports_are_rejected() {
        let source = "\
use private::*;
pub use visible::{self, *};
";

        assert_eq!(
            rules(Path::new("example.rs"), source)
                .into_iter()
                .filter(|rule| *rule == Rule::WildcardImport)
                .count(),
            2
        );
    }
}
