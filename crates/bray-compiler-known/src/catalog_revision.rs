/// Exact revision of the private compiler-known catalog source grammar.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogGrammarRevision(u32);

impl CatalogGrammarRevision {
    /// The exact grammar revision implemented by this compiler.
    pub const SUPPORTED: Self = Self::new(1);

    /// Creates a revision from its canonical decimal value.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the canonical decimal value.
    pub const fn raw(self) -> u32 {
        self.0
    }
}
