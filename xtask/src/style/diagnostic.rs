use ra_ap_syntax::{TextRange, TextSize};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Severity {
    Warning,
    Error,
}

impl Severity {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Rule {
    BlankLineInList,
    CommentSeparation,
    DestructuringLetSeparation,
    FunctionTooLarge,
    InvalidExemption,
    LegacyModRs,
    LetElseSeparation,
    ModuleTooLarge,
    MultilineStatementSeparation,
    MultipleBlankLines,
    NonThinLibRoot,
    NonThinModuleRoot,
    RepeatedModulePrefix,
    ReturnedExpressionSeparation,
    UnusedExemption,
    WildcardImport,
}

impl Rule {
    pub(super) fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "function-too-large" => Some(Self::FunctionTooLarge),
            "legacy-mod-rs" => Some(Self::LegacyModRs),
            "module-too-large" => Some(Self::ModuleTooLarge),
            "non-thin-lib-root" => Some(Self::NonThinLibRoot),
            "non-thin-module-root" => Some(Self::NonThinModuleRoot),
            "repeated-module-prefix" => Some(Self::RepeatedModulePrefix),
            "wildcard-import" => Some(Self::WildcardImport),
            _ => None,
        }
    }

    pub(super) fn identifier(self) -> &'static str {
        match self {
            Self::BlankLineInList => "blank-line-in-list",
            Self::CommentSeparation => "comment-separation",
            Self::DestructuringLetSeparation => "destructuring-let-separation",
            Self::FunctionTooLarge => "function-too-large",
            Self::InvalidExemption => "invalid-exemption",
            Self::LegacyModRs => "legacy-mod-rs",
            Self::LetElseSeparation => "let-else-separation",
            Self::ModuleTooLarge => "module-too-large",
            Self::MultilineStatementSeparation => "multiline-statement-separation",
            Self::MultipleBlankLines => "multiple-blank-lines",
            Self::NonThinLibRoot => "non-thin-lib-root",
            Self::NonThinModuleRoot => "non-thin-module-root",
            Self::RepeatedModulePrefix => "repeated-module-prefix",
            Self::ReturnedExpressionSeparation => "returned-expression-separation",
            Self::UnusedExemption => "unused-exemption",
            Self::WildcardImport => "wildcard-import",
        }
    }

    pub(super) fn severity(self) -> Severity {
        match self {
            Self::ModuleTooLarge | Self::RepeatedModulePrefix | Self::UnusedExemption => {
                Severity::Warning
            }
            _ => Severity::Error,
        }
    }

    pub(super) fn message(self) -> &'static str {
        match self {
            Self::BlankLineInList => "blank lines are not allowed inside this list",
            Self::CommentSeparation => "a block-level comment must have a blank line above it",
            Self::DestructuringLetSeparation => {
                "a destructuring let statement must be separated from adjacent statements"
            }
            Self::FunctionTooLarge => "function exceeds the production source-line limit",
            Self::InvalidExemption => "style exemption is invalid",
            Self::LegacyModRs => "legacy mod.rs module layout is not allowed",
            Self::LetElseSeparation => {
                "a let-else guard must be separated from adjacent statements"
            }
            Self::ModuleTooLarge => "module exceeds the production source-line warning threshold",
            Self::MultilineStatementSeparation => {
                "a multiline statement must be separated from adjacent statements"
            }
            Self::MultipleBlankLines => "use at most one consecutive blank line",
            Self::NonThinLibRoot => "lib.rs must be a thin crate root",
            Self::NonThinModuleRoot => {
                "a module paired with a same-named directory must be a thin root"
            }
            Self::RepeatedModulePrefix => "submodule filename repeats its parent module name",
            Self::ReturnedExpressionSeparation => {
                "a returned expression must be separated from preceding statements"
            }
            Self::UnusedExemption => "style exemption does not suppress a diagnostic",
            Self::WildcardImport => "wildcard imports and re-exports are not allowed",
        }
    }

    pub(super) fn exemption_scope(self) -> Option<ExemptionScope> {
        match self {
            Self::FunctionTooLarge
            | Self::NonThinLibRoot
            | Self::NonThinModuleRoot
            | Self::WildcardImport => Some(ExemptionScope::Item),
            Self::LegacyModRs | Self::ModuleTooLarge | Self::RepeatedModulePrefix => {
                Some(ExemptionScope::File)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExemptionScope {
    File,
    Item,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Target {
    None,
    File,
    Item(TextRange),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Diagnostic {
    pub(super) rule: Rule,
    pub(super) offset: TextSize,
    pub(super) message: String,
    pub(super) help: Option<String>,
    pub(super) target: Target,
}

impl Diagnostic {
    pub(super) fn new(rule: Rule, offset: TextSize) -> Self {
        Self {
            rule,
            offset,
            message: rule.message().to_owned(),
            help: None,
            target: Target::None,
        }
    }

    pub(super) fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();

        self
    }

    pub(super) fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());

        self
    }

    pub(super) fn for_file(mut self) -> Self {
        self.target = Target::File;

        self
    }

    pub(super) fn for_item(mut self, range: TextRange) -> Self {
        self.target = Target::Item(range);

        self
    }
}
