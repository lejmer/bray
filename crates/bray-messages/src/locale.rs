/// Locale selected for user-facing diagnostic rendering.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLocale {
    /// English diagnostic output.
    #[default]
    English,
}
