use std::num::NonZeroU32;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_ir::MirSourceAnchor;

/// Stable file identity used by generated debug information.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenSourceFile {
    path: NonEmptySharedStr,
}

impl CodegenSourceFile {
    /// Creates a source-file identity from a non-empty normalized path.
    pub fn try_new(path: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(path).map(|path| Self { path })
    }

    /// Returns the normalized source path.
    pub fn path(&self) -> &str {
        self.path.as_str()
    }
}

/// Exact source location selected for one MIR source anchor.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenDebugLocation {
    anchor: MirSourceAnchor,
    file: CodegenSourceFile,
    line: NonZeroU32,
    column: NonZeroU32,
}

impl CodegenDebugLocation {
    /// Creates a one-based source location for a MIR anchor.
    pub const fn new(
        anchor: MirSourceAnchor,
        file: CodegenSourceFile,
        line: NonZeroU32,
        column: NonZeroU32,
    ) -> Self {
        Self {
            anchor,
            file,
            line,
            column,
        }
    }

    /// Returns the MIR source anchor represented by this location.
    pub const fn anchor(&self) -> &MirSourceAnchor {
        &self.anchor
    }

    /// Returns the normalized source-file identity.
    pub const fn file(&self) -> &CodegenSourceFile {
        &self.file
    }

    /// Returns the one-based source line.
    pub const fn line(&self) -> NonZeroU32 {
        self.line
    }

    /// Returns the one-based source column.
    pub const fn column(&self) -> NonZeroU32 {
        self.column
    }
}
