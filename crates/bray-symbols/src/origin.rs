/// Describes how a symbol entered a compilation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolOrigin {
    /// The symbol was declared in source compiled as part of the current package.
    Source,
    /// The symbol was loaded from a compiled package interface.
    Imported,
    /// The symbol comes from the compiler-known declaration catalog.
    CompilerKnown,
    /// The compiler provides the symbol as part of the language environment.
    CompilerProvided,
    /// The compiler synthesized the symbol from another semantic declaration.
    Synthesized,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::SymbolOrigin;

    #[test]
    fn origins_are_distinct_semantic_categories() {
        let origins = BTreeSet::from([
            SymbolOrigin::Source,
            SymbolOrigin::Imported,
            SymbolOrigin::CompilerKnown,
            SymbolOrigin::CompilerProvided,
            SymbolOrigin::Synthesized,
        ]);

        assert_eq!(origins.len(), 5);
    }
}
