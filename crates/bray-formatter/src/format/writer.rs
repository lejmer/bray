use super::layout::{self, BreakKind, LayoutElement};

enum PendingWhitespace {
    None,
    Space,
    OptionalBreak {
        space_when_flat: bool,
    },
    FillBreak {
        space_when_flat: bool,
        continuation_indent: u8,
    },
    RequiredBreak(u8),
}

pub(super) struct FormatWriter {
    elements: Vec<LayoutElement>,
    line_ending: &'static str,
    maximum_width: usize,
    final_newline: bool,
    pending: PendingWhitespace,
    line_start: bool,
}

impl FormatWriter {
    pub(super) fn new(
        line_ending: &'static str,
        maximum_width: usize,
        final_newline: bool,
    ) -> Self {
        Self {
            elements: Vec::new(),
            line_ending,
            maximum_width,
            final_newline,
            pending: PendingWhitespace::None,
            line_start: true,
        }
    }

    pub(super) fn begin_group(&mut self) {
        self.flush_layout();
        self.elements.push(LayoutElement::GroupStart);
    }

    pub(super) fn end_group(&mut self) {
        self.flush_layout();
        self.elements.push(LayoutElement::GroupEnd);
    }

    pub(super) fn increase_indent(&mut self) {
        self.elements.push(LayoutElement::Indent(1));
    }

    pub(super) fn decrease_indent(&mut self) {
        self.elements.push(LayoutElement::Indent(-1));
    }

    pub(super) fn is_line_start(&self) -> bool {
        self.line_start || matches!(self.pending, PendingWhitespace::RequiredBreak(_))
    }

    pub(super) fn request_newlines(&mut self, count: u8) {
        let count = match self.pending {
            PendingWhitespace::RequiredBreak(pending) => pending.max(count),
            _ => count,
        };

        self.pending = PendingWhitespace::RequiredBreak(count);
    }

    pub(super) fn set_newlines(&mut self, count: u8) {
        self.pending = PendingWhitespace::RequiredBreak(count);
    }

    pub(super) fn request_space(&mut self) {
        if !self.is_line_start() && matches!(self.pending, PendingWhitespace::None) {
            self.pending = PendingWhitespace::Space;
        }
    }

    pub(super) fn request_optional_break(&mut self, space_when_flat: bool) {
        if !matches!(self.pending, PendingWhitespace::RequiredBreak(_)) {
            self.pending = PendingWhitespace::OptionalBreak { space_when_flat };
        }
    }

    pub(super) fn request_fill_break(&mut self, space_when_flat: bool, continuation_indent: u8) {
        if !matches!(self.pending, PendingWhitespace::RequiredBreak(_)) {
            self.pending = PendingWhitespace::FillBreak {
                space_when_flat,
                continuation_indent,
            };
        }
    }

    pub(super) fn clear_space(&mut self) {
        if matches!(self.pending, PendingWhitespace::Space) {
            self.pending = PendingWhitespace::None;
        }
    }

    pub(super) fn omit_trailing_comma_when_flat(&mut self) {
        for element in self.elements.iter_mut().rev() {
            if let LayoutElement::Text(text) = element {
                if text.as_ref() == "," {
                    *element = LayoutElement::TrailingComma;
                }

                break;
            }
        }
    }

    pub(super) fn write(&mut self, text: &str) {
        self.flush_layout();
        self.elements.push(LayoutElement::Text(text.into()));
        self.line_start = false;
    }

    pub(super) fn write_exact(&mut self, text: &str) {
        self.flush_layout();
        self.elements.push(LayoutElement::Text(text.into()));
        self.line_start = text.ends_with('\n');
    }

    pub(super) fn finish(self) -> String {
        layout::render(
            &self.elements,
            self.line_ending,
            self.maximum_width,
            self.final_newline,
        )
    }

    fn flush_layout(&mut self) {
        let pending = std::mem::replace(&mut self.pending, PendingWhitespace::None);

        match pending {
            PendingWhitespace::None => {}
            PendingWhitespace::Space => self.elements.push(LayoutElement::Space),
            PendingWhitespace::OptionalBreak { space_when_flat } => {
                self.elements
                    .push(LayoutElement::Break(BreakKind::Optional {
                        space_when_flat,
                    }));
            }
            PendingWhitespace::FillBreak {
                space_when_flat,
                continuation_indent,
            } => self.elements.push(LayoutElement::Break(BreakKind::Fill {
                space_when_flat,
                continuation_indent,
            })),
            PendingWhitespace::RequiredBreak(count) if !self.elements.is_empty() => {
                self.elements
                    .push(LayoutElement::Break(BreakKind::Required(count)));

                self.line_start = true;
            }
            PendingWhitespace::RequiredBreak(_) => {}
        }
    }
}
