use unicode_width::UnicodeWidthStr;

use super::line_ending;

const INDENT: &str = "    ";

#[derive(Clone, Copy)]
pub(super) enum BreakKind {
    Required(u8),
    Optional {
        space_when_flat: bool,
    },
    Fill {
        space_when_flat: bool,
        continuation_indent: u8,
    },
}

pub(super) enum LayoutElement {
    Text(Box<str>),
    TrailingComma,
    Space,
    Break(BreakKind),
    Indent(i8),
    GroupStart,
    GroupEnd,
    BlockItemStart {
        item: usize,
        previous: Option<usize>,
        category_boundary: bool,
    },
    BlockItemEnd(usize),
}

struct ActiveBlockItem {
    item: usize,
    start_line: usize,
    insertion_offset: usize,
    started: bool,
    separated: bool,
    has_previous: bool,
    previous_multiline: bool,
    category_boundary: bool,
}

#[derive(Clone, Copy)]
enum GroupMode {
    Flat,
    Broken,
}

pub(super) fn render(
    elements: &[LayoutElement],
    line_ending: &str,
    maximum_width: usize,
    final_newline: bool,
) -> String {
    let mut renderer = Renderer {
        elements,
        line_ending,
        maximum_width,
        output: String::new(),
        column: 0,
        indent: 0,
        groups: Vec::new(),
        final_newline,
        line: 0,
        line_start_offset: 0,
        multiline_block_items: Vec::new(),
        active_block_items: Vec::new(),
    };

    renderer.render();

    renderer.finish()
}

struct Renderer<'layout> {
    elements: &'layout [LayoutElement],
    line_ending: &'layout str,
    maximum_width: usize,
    output: String,
    column: usize,
    indent: usize,
    groups: Vec<GroupMode>,
    final_newline: bool,
    line: usize,
    line_start_offset: usize,
    multiline_block_items: Vec<bool>,
    active_block_items: Vec<ActiveBlockItem>,
}

impl Renderer<'_> {
    fn render(&mut self) {
        for (index, element) in self.elements.iter().enumerate() {
            match element {
                LayoutElement::Text(text) => self.write_text(text),
                LayoutElement::TrailingComma
                    if matches!(self.groups.last(), Some(GroupMode::Broken)) =>
                {
                    self.write_text(",");
                }
                LayoutElement::TrailingComma => {}
                LayoutElement::Space => self.write_space(),
                LayoutElement::Break(kind) => self.write_break(index, *kind),
                LayoutElement::Indent(change) => self.change_indent(*change),
                LayoutElement::GroupStart => self.begin_group(index),
                LayoutElement::GroupEnd => {
                    self.groups.pop();
                }
                LayoutElement::BlockItemStart {
                    item,
                    previous,
                    category_boundary,
                } => {
                    self.begin_block_item(*item, *previous, *category_boundary);
                }
                LayoutElement::BlockItemEnd(item) => self.end_block_item(*item),
            }
        }
    }

    fn begin_block_item(
        &mut self,
        item: usize,
        previous: Option<usize>,
        category_boundary: bool,
    ) {
        let previous_multiline = previous.is_some_and(|previous| {
            self.multiline_block_items
                .get(previous)
                .copied()
                .unwrap_or(false)
        });

        self.active_block_items.push(ActiveBlockItem {
            item,
            start_line: 0,
            insertion_offset: 0,
            started: false,
            separated: false,
            has_previous: previous.is_some(),
            previous_multiline,
            category_boundary,
        });
    }

    fn end_block_item(&mut self, item: usize) {
        let active = self
            .active_block_items
            .pop()
            .unwrap_or_else(|| panic!("block item layout must remain balanced"));

        assert_eq!(active.item, item, "block item layout must remain balanced");

        let multiline = active.started && self.line > active.start_line;

        if self.multiline_block_items.len() <= item {
            self.multiline_block_items.resize(item + 1, false);
        }

        self.multiline_block_items[item] = multiline;

        if active.has_previous && multiline && !active.separated {
            self.output
                .insert_str(active.insertion_offset, self.line_ending);

            self.line = self.line.saturating_add(1);

            self.line_start_offset = self
                .line_start_offset
                .saturating_add(self.line_ending.len());
        }
    }

    fn start_block_item(&mut self) {
        let Some(index) = self.active_block_items.len().checked_sub(1) else {
            return;
        };

        if self.active_block_items[index].started {
            return;
        }

        let insertion_offset = self.line_start_offset;
        let already_separated = trailing_line_breaks(&self.output[..insertion_offset]) >= 2;

        let separate = self.active_block_items[index].has_previous
            && (self.active_block_items[index].previous_multiline
                || self.active_block_items[index].category_boundary)
            && !already_separated;

        if separate {
            self.output.insert_str(insertion_offset, self.line_ending);

            self.line = self.line.saturating_add(1);

            self.line_start_offset = self
                .line_start_offset
                .saturating_add(self.line_ending.len());
        }

        let active = &mut self.active_block_items[index];

        active.start_line = self.line;
        active.insertion_offset = insertion_offset;
        active.started = true;
        active.separated = already_separated || separate;
    }

    fn begin_group(&mut self, index: usize) {
        let mode = if flat_width(&self.elements[index + 1..])
            .is_some_and(|width| self.column.saturating_add(width) <= self.maximum_width)
        {
            GroupMode::Flat
        } else {
            GroupMode::Broken
        };

        self.groups.push(mode);
    }

    fn write_text(&mut self, text: &str) {
        self.start_block_item();
        self.output.push_str(text);

        let line_breaks = line_ending::count(text);

        self.line = self.line.saturating_add(usize::from(line_breaks));

        if line_breaks > 0 {
            self.line_start_offset = self.output.rfind(['\r', '\n']).map_or(0, |index| index + 1);
        }

        if let Some(last_line) = text.rsplit(['\r', '\n']).next()
            && text.contains(['\r', '\n'])
        {
            self.column = UnicodeWidthStr::width(last_line);
        } else {
            self.column = self.column.saturating_add(UnicodeWidthStr::width(text));
        }
    }

    fn write_space(&mut self) {
        self.output.push(' ');
        self.column = self.column.saturating_add(1);
    }

    fn write_break(&mut self, index: usize, kind: BreakKind) {
        match kind {
            BreakKind::Required(count) => self.write_newlines(count),
            BreakKind::Fill {
                space_when_flat,
                continuation_indent,
            } => {
                let following_width = flat_width_until_break(&self.elements[index + 1..]);
                let flat_space = usize::from(space_when_flat);

                if following_width.is_some_and(|width| {
                    self.column.saturating_add(flat_space).saturating_add(width)
                        <= self.maximum_width
                }) {
                    if space_when_flat {
                        self.write_space();
                    }
                } else {
                    self.write_newlines_with_indent(1, continuation_indent.into());
                }
            }
            BreakKind::Optional { space_when_flat }
                if matches!(self.groups.last(), Some(GroupMode::Flat)) =>
            {
                if space_when_flat {
                    self.write_space();
                }
            }
            BreakKind::Optional { .. } => self.write_newlines(1),
        }
    }

    fn write_newlines(&mut self, count: u8) {
        self.write_newlines_with_indent(count, 0);
    }

    fn write_newlines_with_indent(&mut self, count: u8, additional_indent: usize) {
        let count = if let Some(line_start) = self.output.rfind(['\r', '\n']).map(|index| index + 1)
            && self.output[line_start..].bytes().all(|byte| byte == b' ')
        {
            self.output.truncate(line_start);

            count.saturating_sub(1)
        } else {
            count
        };

        for _ in 0..count {
            self.output.push_str(self.line_ending);
        }

        self.line = self.line.saturating_add(usize::from(count));
        self.line_start_offset = self.output.len();

        let indent = self.indent.saturating_add(additional_indent);

        for _ in 0..indent {
            self.output.push_str(INDENT);
        }

        self.column = indent.saturating_mul(INDENT.len());
    }

    fn change_indent(&mut self, change: i8) {
        if change.is_positive() {
            self.indent = self.indent.saturating_add(change.unsigned_abs().into());
        } else {
            self.indent = self.indent.saturating_sub(change.unsigned_abs().into());
        }
    }

    fn finish(mut self) -> String {
        while self.output.ends_with('\n') {
            self.output.pop();

            if self.output.ends_with('\r') {
                self.output.pop();
            }
        }

        if self.final_newline && !self.output.is_empty() {
            self.output.push_str(self.line_ending);
        }

        self.output
    }
}

fn trailing_line_breaks(text: &str) -> u8 {
    text.bytes()
        .rev()
        .take_while(|byte| matches!(byte, b'\r' | b'\n'))
        .filter(|byte| *byte == b'\n')
        .count()
        .try_into()
        .unwrap_or(u8::MAX)
}

fn flat_width_until_break(elements: &[LayoutElement]) -> Option<usize> {
    let mut width = 0_usize;

    for element in elements {
        match element {
            LayoutElement::Text(text) if text.contains(['\r', '\n']) => return None,
            LayoutElement::Text(text) => {
                width = width.saturating_add(UnicodeWidthStr::width(text.as_ref()));
            }
            LayoutElement::TrailingComma => {}
            LayoutElement::Space => width = width.saturating_add(1),
            LayoutElement::Break(_) => return Some(width),
            LayoutElement::Indent(_)
            | LayoutElement::GroupStart
            | LayoutElement::GroupEnd
            | LayoutElement::BlockItemStart { .. }
            | LayoutElement::BlockItemEnd(_) => {}
        }
    }

    Some(width)
}

fn flat_width(elements: &[LayoutElement]) -> Option<usize> {
    let mut width = 0_usize;
    let mut depth = 0_usize;

    for (index, element) in elements.iter().enumerate() {
        match element {
            LayoutElement::Text(text) if text.contains(['\r', '\n']) => return None,
            LayoutElement::Text(text) => {
                width = width.saturating_add(UnicodeWidthStr::width(text.as_ref()));
            }
            LayoutElement::TrailingComma => {}
            LayoutElement::Space => width = width.saturating_add(1),
            LayoutElement::Break(BreakKind::Required(_)) => return None,
            LayoutElement::Break(BreakKind::Optional { space_when_flat }) => {
                width = width.saturating_add(usize::from(*space_when_flat));
            }
            LayoutElement::Break(BreakKind::Fill {
                space_when_flat, ..
            }) => width = width.saturating_add(usize::from(*space_when_flat)),
            LayoutElement::Indent(_) => {}
            LayoutElement::GroupStart => depth = depth.saturating_add(1),
            LayoutElement::GroupEnd if depth == 0 => {
                let suffix = flat_width_until_break(&elements[index + 1..]).unwrap_or(0);

                return Some(width.saturating_add(suffix));
            }
            LayoutElement::GroupEnd => depth -= 1,
            LayoutElement::BlockItemStart { .. } | LayoutElement::BlockItemEnd(_) => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{BreakKind, LayoutElement, render};

    #[test]
    fn groups_select_flat_or_nested_layout_by_display_width() {
        let elements = [
            LayoutElement::Text("call".into()),
            LayoutElement::GroupStart,
            LayoutElement::Text("(".into()),
            LayoutElement::Indent(1),
            LayoutElement::Break(BreakKind::Optional {
                space_when_flat: false,
            }),
            LayoutElement::Text("first".into()),
            LayoutElement::Text(",".into()),
            LayoutElement::Break(BreakKind::Optional {
                space_when_flat: true,
            }),
            LayoutElement::Text("second".into()),
            LayoutElement::Indent(-1),
            LayoutElement::Break(BreakKind::Optional {
                space_when_flat: false,
            }),
            LayoutElement::Text(")".into()),
            LayoutElement::GroupEnd,
        ];

        assert_eq!(render(&elements, "\n", 20, true), "call(first, second)\n");

        assert_eq!(
            render(&elements, "\r\n", 10, true),
            "call(\r\n    first,\r\n    second\r\n)\r\n"
        );
    }

    #[test]
    fn consecutive_required_breaks_do_not_create_whitespace_only_lines() {
        let elements = [
            LayoutElement::Text("first".into()),
            LayoutElement::Indent(1),
            LayoutElement::Break(BreakKind::Required(1)),
            LayoutElement::GroupStart,
            LayoutElement::Break(BreakKind::Required(1)),
            LayoutElement::Text("second".into()),
            LayoutElement::GroupEnd,
        ];

        assert_eq!(render(&elements, "\n", 120, true), "first\n    second\n");
    }
}
