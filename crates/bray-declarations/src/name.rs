use std::sync::Arc;

use bray_syntax::SyntaxKind;

/// Syntactic dotted module path discovered before name binding.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModulePath {
    segments: Arc<[String]>,
}

impl ModulePath {
    /// Creates a module path from source-order path segments.
    pub fn new(segments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let segments = segments
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            segments: Arc::from(segments),
        }
    }

    /// Returns source-order path segments.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Returns whether this path has no present source segments.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns the path as a dotted string.
    pub fn dotted(&self) -> String {
        self.segments.join(".")
    }
}

/// Syntax-level declaration name recorded before binding.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclarationName {
    /// Single identifier declaration name.
    Identifier(String),
    /// Fixed keyword-shaped declaration name.
    Keyword(SyntaxKind),
    /// Dotted path used by path-shaped declarations.
    Path(ModulePath),
    /// Subject and optional trait path for implementation-shaped declarations.
    Implementation(ImplementationDeclarationName),
}

/// Syntax-level implementation declaration name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationDeclarationName {
    subject: ModulePath,
    trait_path: Option<ModulePath>,
}

impl ImplementationDeclarationName {
    /// Creates an implementation name from its subject and optional trait path.
    pub const fn new(subject: ModulePath, trait_path: Option<ModulePath>) -> Self {
        Self {
            subject,
            trait_path,
        }
    }

    /// Returns the implementation subject path.
    pub const fn subject(&self) -> &ModulePath {
        &self.subject
    }

    /// Returns the implemented trait path when this is trait-shaped.
    pub const fn trait_path(&self) -> Option<&ModulePath> {
        self.trait_path.as_ref()
    }
}

impl DeclarationName {
    /// Returns the identifier name when this is an identifier.
    pub fn as_identifier(&self) -> Option<&str> {
        match self {
            Self::Identifier(identifier) => Some(identifier),
            Self::Keyword(_) | Self::Path(_) | Self::Implementation(_) => None,
        }
    }

    /// Returns the keyword kind when this name is keyword-shaped.
    pub const fn as_keyword(&self) -> Option<SyntaxKind> {
        match self {
            Self::Keyword(kind) => Some(*kind),
            Self::Identifier(_) | Self::Path(_) | Self::Implementation(_) => None,
        }
    }

    /// Returns the path name when this is a path.
    pub const fn as_path(&self) -> Option<&ModulePath> {
        match self {
            Self::Identifier(_) | Self::Keyword(_) | Self::Implementation(_) => None,
            Self::Path(path) => Some(path),
        }
    }

    /// Returns the implementation name when this is implementation-shaped.
    pub const fn as_implementation(&self) -> Option<&ImplementationDeclarationName> {
        match self {
            Self::Identifier(_) | Self::Keyword(_) | Self::Path(_) => None,
            Self::Implementation(name) => Some(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::SyntaxKind;

    use super::{DeclarationName, ImplementationDeclarationName, ModulePath};

    #[test]
    fn module_paths_store_segments_in_source_order() {
        let path = ModulePath::new(["core", "io"]);

        assert_eq!(path.segments(), ["core", "io"]);
        assert_eq!(path.dotted(), "core.io");
        assert!(!path.is_empty());
    }

    #[test]
    fn declaration_names_expose_their_shape() {
        let identifier = DeclarationName::Identifier(String::from("main"));
        let keyword = DeclarationName::Keyword(SyntaxKind::ConstructKeyword);
        let path = DeclarationName::Path(ModulePath::new(["core"]));

        let implementation = DeclarationName::Implementation(ImplementationDeclarationName::new(
            ModulePath::new(["Point"]),
            Some(ModulePath::new(["Display"])),
        ));

        assert_eq!(identifier.as_identifier(), Some("main"));
        assert_eq!(identifier.as_keyword(), None);
        assert_eq!(identifier.as_path(), None);
        assert_eq!(identifier.as_implementation(), None);

        assert_eq!(keyword.as_identifier(), None);
        assert_eq!(keyword.as_keyword(), Some(SyntaxKind::ConstructKeyword));
        assert_eq!(keyword.as_path(), None);
        assert_eq!(keyword.as_implementation(), None);

        assert_eq!(path.as_identifier(), None);
        assert_eq!(path.as_keyword(), None);

        assert_eq!(
            path.as_path().map(ModulePath::dotted),
            Some(String::from("core"))
        );

        assert_eq!(path.as_implementation(), None);

        let implementation_name = match implementation.as_implementation() {
            Some(implementation_name) => implementation_name,
            None => panic!("expected implementation name"),
        };

        assert_eq!(implementation.as_identifier(), None);
        assert_eq!(implementation.as_keyword(), None);
        assert_eq!(implementation.as_path(), None);
        assert_eq!(implementation_name.subject().dotted(), "Point");

        assert_eq!(
            implementation_name.trait_path().map(ModulePath::dotted),
            Some(String::from("Display"))
        );
    }
}
