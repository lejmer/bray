use bray_source::TextSize;
use clap::ValueEnum;
use clap::builder::PossibleValue;

/// Output format selected for command-produced output.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OutputFormat {
    /// Plain text output.
    #[default]
    Text,
    /// Structured JSON output.
    Json,
}

impl OutputFormat {
    /// Returns the stable command-line spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
        }
    }
}

impl ValueEnum for OutputFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Text, Self::Json]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        let help = match self {
            Self::Text => bray_messages::command_help::OUTPUT_TEXT,
            Self::Json => bray_messages::command_help::OUTPUT_JSON,
        };

        Some(PossibleValue::new(self.as_str()).help(help))
    }
}

/// Selects semantic units from one source, optionally at one position.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InspectionTarget {
    source_id: u32,
    position: Option<TextSize>,
}

impl InspectionTarget {
    /// Creates a target covering every independently checked unit in one source.
    pub const fn source(source_id: u32) -> Self {
        Self {
            source_id,
            position: None,
        }
    }

    /// Creates a target for the innermost unit covering one UTF-8 byte offset.
    pub const fn at(source_id: u32, position: TextSize) -> Self {
        Self {
            source_id,
            position: Some(position),
        }
    }

    /// Returns the raw loaded-source identity.
    pub const fn source_id(self) -> u32 {
        self.source_id
    }

    /// Returns the selected UTF-8 byte offset, if inspection is position-filtered.
    pub const fn position(self) -> Option<TextSize> {
        self.position
    }
}
