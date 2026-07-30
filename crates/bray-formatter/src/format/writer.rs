const INDENT: &str = "    ";

pub(super) struct FormatWriter {
    output: String,
    line_ending: &'static str,
    indent: usize,
    pending_newlines: u8,
    pending_space: bool,
    line_start: bool,
}

impl FormatWriter {
    pub(super) fn new(line_ending: &'static str) -> Self {
        Self {
            output: String::new(),
            line_ending,
            indent: 0,
            pending_newlines: 0,
            pending_space: false,
            line_start: true,
        }
    }

    pub(super) fn increase_indent(&mut self) {
        self.indent += 1;
    }

    pub(super) fn decrease_indent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    pub(super) fn is_line_start(&self) -> bool {
        self.line_start || self.pending_newlines > 0
    }

    pub(super) fn request_newlines(&mut self, count: u8) {
        self.pending_newlines = self.pending_newlines.max(count);
        self.pending_space = false;
    }

    pub(super) fn set_newlines(&mut self, count: u8) {
        self.pending_newlines = count;
        self.pending_space = false;
    }

    pub(super) fn request_space(&mut self) {
        if !self.is_line_start() {
            self.pending_space = true;
        }
    }

    pub(super) fn clear_space(&mut self) {
        self.pending_space = false;
    }

    pub(super) fn write(&mut self, text: &str) {
        self.flush_layout();
        self.output.push_str(text);
        self.line_start = false;
    }

    pub(super) fn write_exact(&mut self, text: &str) {
        self.flush_layout();
        self.output.push_str(text);
        self.line_start = text.ends_with('\n');
    }

    pub(super) fn finish(mut self) -> String {
        self.pending_space = false;

        while self.output.ends_with('\n') {
            self.output.pop();

            if self.output.ends_with('\r') {
                self.output.pop();
            }
        }

        if !self.output.is_empty() {
            self.output.push_str(self.line_ending);
        }

        self.output
    }

    fn flush_layout(&mut self) {
        if self.pending_newlines > 0 && !self.output.is_empty() {
            for _ in 0..self.pending_newlines {
                self.output.push_str(self.line_ending);
            }

            self.line_start = true;
        }

        if self.line_start {
            for _ in 0..self.indent {
                self.output.push_str(INDENT);
            }

            self.line_start = false;
        } else if self.pending_space {
            self.output.push(' ');
        }

        self.pending_newlines = 0;
        self.pending_space = false;
    }
}
