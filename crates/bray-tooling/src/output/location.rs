#[cfg(feature = "analysis")]
use bray_source::TextRange;
use bray_source::{LineColumn, LspPosition, SourceLocation};
use serde::Serialize;

#[cfg(feature = "analysis")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TextRangeOutput {
    start: u32,
    end: u32,
}

#[cfg(feature = "analysis")]
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
    start: LineColumnOutput,
    end: LineColumnOutput,
    lsp_start: LspPositionOutput,
    lsp_end: LspPositionOutput,
}

impl SourceLocationOutput {
    pub(crate) const fn from_location(location: SourceLocation<'_>) -> Self {
        Self {
            start: LineColumnOutput::from_position(location.start()),
            end: LineColumnOutput::from_position(location.end()),
            lsp_start: LspPositionOutput::from_position(location.lsp_start()),
            lsp_end: LspPositionOutput::from_position(location.lsp_end()),
        }
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn start(self) -> LineColumnOutput {
        self.start
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn end(self) -> LineColumnOutput {
        self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct LineColumnOutput {
    line: u32,
    column: u32,
}

impl LineColumnOutput {
    pub(crate) const fn from_position(position: LineColumn) -> Self {
        Self {
            line: position.line(),
            column: position.column(),
        }
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn line(self) -> u32 {
        self.line
    }

    #[cfg(feature = "analysis")]
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
