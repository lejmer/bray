use bray_diagnostics::SeverityKind;
use clap::builder::StyledStr;
use clap::builder::Styles;
use clap::builder::styling::{AnsiColor, Style};

pub(crate) fn clap_styles() -> Styles {
    Styles::styled()
        .header(note_style())
        .usage(note_style())
        .literal(help_style())
        .placeholder(warning_style())
        .error(error_style())
        .valid(help_style())
        .invalid(error_style())
        .context(note_style())
        .context_value(warning_style())
}

pub(crate) fn color_severity_label(severity: SeverityKind, label: &str) -> String {
    color_text(severity_style(severity), label)
}

pub(crate) fn color_note_heading(label: &str) -> String {
    color_text(note_style(), label)
}

pub(crate) fn render_styled_text(text: &StyledStr) -> String {
    text.ansi().to_string()
}

fn color_text(style: Style, text: &str) -> String {
    format!("{style}{text}{style:#}")
}

fn severity_style(severity: SeverityKind) -> Style {
    match severity {
        SeverityKind::Error => error_style(),
        SeverityKind::Warning => warning_style(),
        SeverityKind::Note => note_style(),
        SeverityKind::Help => help_style(),
    }
}

fn error_style() -> Style {
    AnsiColor::Red.on_default()
}

fn warning_style() -> Style {
    AnsiColor::Yellow.on_default()
}

fn note_style() -> Style {
    AnsiColor::Cyan.on_default()
}

fn help_style() -> Style {
    AnsiColor::Green.on_default()
}
