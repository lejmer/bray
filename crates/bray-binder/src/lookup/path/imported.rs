use bray_symbols::{ImportedSymbolSkeleton, PackageSymbolId};

/// One selected imported package root for a qualified source path.
#[derive(Clone, Copy, Debug)]
pub struct ImportedPathRoot<'symbols> {
    symbols: &'symbols ImportedSymbolSkeleton,
    package: PackageSymbolId,
    consumed_components: usize,
}

impl<'symbols> ImportedPathRoot<'symbols> {
    /// Selects the longest imported package identity that prefixes a source path.
    pub fn select(symbols: &'symbols ImportedSymbolSkeleton, components: &[&str]) -> Option<Self> {
        symbols
            .packages()
            .iter()
            .filter_map(|package| {
                let component_count = package.identity().as_str().split('.').count();

                package
                    .identity()
                    .as_str()
                    .split('.')
                    .eq(components.iter().take(component_count).copied())
                    .then_some((package.id(), component_count))
            })
            .max_by_key(|(_, component_count)| *component_count)
            .and_then(|(package, _)| Self::for_path(symbols, package, components))
    }

    /// Creates a root when the package identity is an exact prefix of the source path.
    pub fn for_path(
        symbols: &'symbols ImportedSymbolSkeleton,
        package: PackageSymbolId,
        components: &[&str],
    ) -> Option<Self> {
        let package_identity = symbols.package(package)?.identity();
        let consumed_components = package_identity.as_str().split('.').count();

        if consumed_components > components.len()
            || !package_identity
                .as_str()
                .split('.')
                .eq(components[..consumed_components].iter().copied())
        {
            return None;
        }

        Some(Self {
            symbols,
            package,
            consumed_components,
        })
    }

    /// Returns the imported identity provider for the selected package.
    pub const fn symbols(self) -> &'symbols ImportedSymbolSkeleton {
        self.symbols
    }

    /// Returns the exact imported package identity selected by the path prefix.
    pub const fn package(self) -> PackageSymbolId {
        self.package
    }

    /// Returns the number of source path components occupied by the package identity.
    pub const fn consumed_components(self) -> usize {
        self.consumed_components
    }
}
