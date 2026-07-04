use bray_source::{LineColumn, LspPosition, SourceLocation, TextRange};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TextRangeOutput {
    start: u32,
    end: u32,
}

impl TextRangeOutput {
    pub(crate) const fn from_range(range: TextRange) -> Self {
        Self {
            start: range.start().bytes(),
            end: range.end().bytes(),
        }
    }

    pub(crate) const fn start(self) -> u32 {
        self.start
    }

    pub(crate) const fn end(self) -> u32 {
        self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SourceLocationOutput {
    start: HumanPositionOutput,
    end: HumanPositionOutput,
    lsp_start: LspPositionOutput,
    lsp_end: LspPositionOutput,
}

impl SourceLocationOutput {
    pub(crate) const fn from_location(location: SourceLocation<'_>) -> Self {
        Self {
            start: HumanPositionOutput::from_position(location.start()),
            end: HumanPositionOutput::from_position(location.end()),
            lsp_start: LspPositionOutput::from_position(location.lsp_start()),
            lsp_end: LspPositionOutput::from_position(location.lsp_end()),
        }
    }

    pub(crate) const fn start(self) -> HumanPositionOutput {
        self.start
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct HumanPositionOutput {
    line: u32,
    column: u32,
}

impl HumanPositionOutput {
    pub(crate) const fn from_position(position: LineColumn) -> Self {
        Self {
            line: position.line(),
            column: position.column(),
        }
    }

    pub(crate) const fn line(self) -> u32 {
        self.line
    }

    pub(crate) const fn column(self) -> u32 {
        self.column
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct LspPositionOutput {
    line: u32,
    character: u32,
}

impl LspPositionOutput {
    const fn from_position(position: LspPosition) -> Self {
        Self {
            line: position.line(),
            character: position.character(),
        }
    }
}
