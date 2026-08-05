use unicode_width::UnicodeWidthStr;

const INDENT: &str = "    ";

#[derive(Clone, Copy)]
pub(super) enum BreakKind {
    Required(u8),
    Optional { space_when_flat: bool },
}

pub(super) enum LayoutElement {
    Text(Box<str>),
    Space,
    Break(BreakKind),
    Indent(i8),
    GroupStart,
    GroupEnd,
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
}

impl Renderer<'_> {
    fn render(&mut self) {
        for (index, element) in self.elements.iter().enumerate() {
            match element {
                LayoutElement::Text(text) => self.write_text(text),
                LayoutElement::Space => self.write_space(),
                LayoutElement::Break(kind) => self.write_break(*kind),
                LayoutElement::Indent(change) => self.change_indent(*change),
                LayoutElement::GroupStart => self.begin_group(index),
                LayoutElement::GroupEnd => {
                    self.groups.pop();
                }
            }
        }
    }

    fn begin_group(&mut self, index: usize) {
        let mode = if matches!(self.groups.last(), Some(GroupMode::Flat))
            || flat_width(&self.elements[index + 1..])
                .is_some_and(|width| self.column.saturating_add(width) <= self.maximum_width)
        {
            GroupMode::Flat
        } else {
            GroupMode::Broken
        };

        self.groups.push(mode);
    }

    fn write_text(&mut self, text: &str) {
        self.output.push_str(text);

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

    fn write_break(&mut self, kind: BreakKind) {
        match kind {
            BreakKind::Required(count) => self.write_newlines(count),
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
        for _ in 0..count {
            self.output.push_str(self.line_ending);
        }

        for _ in 0..self.indent {
            self.output.push_str(INDENT);
        }

        self.column = self.indent.saturating_mul(INDENT.len());
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

fn flat_width(elements: &[LayoutElement]) -> Option<usize> {
    let mut width = 0_usize;
    let mut depth = 0_usize;

    for element in elements {
        match element {
            LayoutElement::Text(text) if text.contains(['\r', '\n']) => return None,
            LayoutElement::Text(text) => {
                width = width.saturating_add(UnicodeWidthStr::width(text.as_ref()));
            }
            LayoutElement::Space => width = width.saturating_add(1),
            LayoutElement::Break(BreakKind::Required(_)) => return None,
            LayoutElement::Break(BreakKind::Optional { space_when_flat }) => {
                width = width.saturating_add(usize::from(*space_when_flat));
            }
            LayoutElement::Indent(_) => {}
            LayoutElement::GroupStart => depth = depth.saturating_add(1),
            LayoutElement::GroupEnd if depth == 0 => return Some(width),
            LayoutElement::GroupEnd => depth -= 1,
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
}
