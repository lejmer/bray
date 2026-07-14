/// Backend-neutral optimization intent for one code generation request.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OptimizationLevel {
    /// Preserve the most direct translation and minimize optimization work.
    #[default]
    None,
    /// Apply inexpensive optimizations suitable for development builds.
    Basic,
    /// Apply the production optimization pipeline.
    Full,
}

/// Relative preference for smaller generated code.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SizePreference {
    /// Do not prefer size over other optimization goals.
    #[default]
    None,
    /// Prefer smaller code when the performance tradeoff is moderate.
    Size,
    /// Minimize generated size even when the performance tradeoff is larger.
    MinimumSize,
}

/// Amount of source-correlated debug information requested from a backend.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DebugInformationMode {
    /// Do not generate debug information.
    #[default]
    None,
    /// Generate source line tables without full variable information.
    LineTables,
    /// Generate full debug information supported by the selected backend and target.
    Full,
}

/// Immutable backend-neutral generation policy for one codegen unit.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenOptions {
    optimization: OptimizationLevel,
    size_preference: SizePreference,
    debug_information: DebugInformationMode,
}

impl CodegenOptions {
    /// Creates code generation policy from typed optimization and debug intent.
    pub const fn new(
        optimization: OptimizationLevel,
        size_preference: SizePreference,
        debug_information: DebugInformationMode,
    ) -> Self {
        Self {
            optimization,
            size_preference,
            debug_information,
        }
    }

    /// Returns the requested optimization level.
    pub const fn optimization(self) -> OptimizationLevel {
        self.optimization
    }

    /// Returns the requested code-size preference.
    pub const fn size_preference(self) -> SizePreference {
        self.size_preference
    }

    /// Returns the requested debug-information mode.
    pub const fn debug_information(self) -> DebugInformationMode {
        self.debug_information
    }
}
