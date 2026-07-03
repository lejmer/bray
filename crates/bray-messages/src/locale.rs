/// Locale selected for user-facing diagnostic rendering.
///
/// English is the initial supported locale. Additional locales should add
/// catalogs and locale-specific formatting rules without changing compiler
/// diagnostic records.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLocale {
    /// English diagnostic output.
    #[default]
    English,
}
