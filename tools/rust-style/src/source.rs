//! Source discovery, indexing, and production-line classification.

use std::path::{Path, PathBuf};

use ra_ap_syntax::ast::HasAttrs;
use ra_ap_syntax::{AstNode, TextRange, TextSize, ast};

const EXCLUDED_DIRECTORIES: [&str; 2] = [".git", "target"];

pub(super) fn rust_source_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    collect_rust_source_paths(root, &mut paths)?;
    paths.sort();

    Ok(paths)
}

fn collect_rust_source_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| io_error("read directory", directory, error))?;

    for entry in entries {
        let entry = entry.map_err(|error| io_error("read directory", directory, error))?;

        let file_type = entry
            .file_type()
            .map_err(|error| io_error("inspect", &entry.path(), error))?;

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

pub(super) fn source_text(source: &str, range: TextRange) -> &str {
    let start = text_offset(range.start());
    let end = text_offset(range.end());

    match source.get(start..end) {
        Some(text) => text,
        None => panic!("syntax ranges must refer to the parsed source"),
    }
}

pub(super) fn count_newlines(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count()
}

pub(super) fn line_starts(source: &str) -> Vec<usize> {
    if source.is_empty() {
        return Vec::new();
    }

    let mut starts = vec![0];

    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' && index + 1 < source.len() {
            starts.push(index + 1);
        }
    }

    starts
}

pub(super) fn line_index(starts: &[usize], offset: usize) -> Option<usize> {
    if starts.is_empty() {
        return None;
    }

    match starts.binary_search(&offset) {
        Ok(index) => Some(index),
        Err(0) => None,
        Err(index) => Some(index - 1),
    }
}

pub(super) fn text_size(offset: usize) -> TextSize {
    let Ok(offset) = u32::try_from(offset) else {
        panic!("parsed Rust source offsets must fit in TextSize");
    };

    TextSize::new(offset)
}

pub(super) fn text_offset(offset: TextSize) -> usize {
    u32::from(offset) as usize
}

pub(super) fn io_error(action: &str, path: &Path, error: impl std::fmt::Display) -> String {
    format!("failed to {action} {}: {error}", path.display())
}

pub(super) fn test_only_ranges(file: &ast::SourceFile, source: &str) -> Vec<TextRange> {
    let mut ranges = file
        .syntax()
        .descendants()
        .filter_map(ast::Item::cast)
        .filter(|item| has_test_only_attribute(item, source))
        .map(|item| item.syntax().text_range())
        .collect::<Vec<_>>();

    ranges.extend(
        file.syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|function| has_test_only_attribute(function, source))
            .map(|function| function.syntax().text_range()),
    );

    ranges.sort_by_key(|range| range.start());
    remove_nested_ranges(&mut ranges);

    ranges
}

fn has_test_only_attribute(owner: &impl HasAttrs, source: &str) -> bool {
    owner.attrs().any(|attribute| {
        let normalized = source_text(source, attribute.syntax().text_range())
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();

        normalized == "#[test]" || normalized == "#[cfg(test)]"
    })
}

fn remove_nested_ranges(ranges: &mut Vec<TextRange>) {
    let mut outer = Vec::with_capacity(ranges.len());

    for range in ranges.drain(..) {
        if outer
            .last()
            .is_some_and(|previous: &TextRange| previous.contains_range(range))
        {
            continue;
        }

        outer.push(range);
    }

    *ranges = outer;
}

pub(super) fn production_line_offsets(source: &str, test_ranges: &[TextRange]) -> Vec<TextSize> {
    let starts = line_starts(source);
    let mut excluded = vec![false; starts.len()];

    for range in test_ranges {
        let start = text_offset(range.start());
        let end = text_offset(range.end()).saturating_sub(1);

        let Some(start_line) = line_index(&starts, start) else {
            continue;
        };

        let Some(end_line) = line_index(&starts, end) else {
            continue;
        };

        excluded[start_line..=end_line].fill(true);
    }

    starts
        .into_iter()
        .zip(excluded)
        .filter(|(_, is_excluded)| !is_excluded)
        .map(|(offset, _)| text_size(offset))
        .collect()
}

pub(super) fn line_offsets_in_range(source: &str, range: TextRange) -> Vec<TextSize> {
    let starts = line_starts(source);
    let start = text_offset(range.start());
    let end = text_offset(range.end()).saturating_sub(1);

    let Some(start_line) = line_index(&starts, start) else {
        return Vec::new();
    };

    let Some(end_line) = line_index(&starts, end) else {
        return Vec::new();
    };

    starts[start_line..=end_line]
        .iter()
        .copied()
        .map(text_size)
        .collect()
}

pub(super) fn range_is_test_only(range: TextRange, test_ranges: &[TextRange]) -> bool {
    test_ranges
        .iter()
        .any(|test_range| test_range.contains_range(range))
}

#[cfg(test)]
mod tests {
    use ra_ap_syntax::{AstNode, Edition, SourceFile};

    use super::{line_offsets_in_range, production_line_offsets, test_only_ranges};

    #[test]
    fn production_lines_exclude_cfg_test_items_and_test_functions() {
        let source = "\
fn production() {}

#[cfg(test)]
mod tests {
    fn helper() {}
}

#[test]
fn standalone_test() {}
";

        let parsed = SourceFile::parse(source, Edition::Edition2024).tree();
        let ranges = test_only_ranges(&parsed, source);
        let offsets = production_line_offsets(source, &ranges);

        assert_eq!(offsets.len(), 3);
    }

    #[test]
    fn line_offsets_use_physical_lines_inside_ranges() {
        let source = "before\nfn example() {\n    value\n}\nafter\n";
        let parsed = SourceFile::parse(source, Edition::Edition2024).tree();

        let Some(function) = parsed
            .syntax()
            .descendants()
            .find_map(ra_ap_syntax::ast::Fn::cast)
        else {
            panic!("test source should contain a function");
        };

        assert_eq!(
            line_offsets_in_range(source, function.syntax().text_range()).len(),
            3
        );
    }
}
