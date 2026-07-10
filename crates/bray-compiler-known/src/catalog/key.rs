use std::sync::Arc;

use bray_base::{shared_slice, shared_str};

macro_rules! define_catalog_key {
    ($(#[$meta:meta])* pub struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Arc<str>);

        impl $name {
            /// Creates a key from one valid catalog identifier.
            pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
                let value = shared_str(value);

                if is_catalog_identifier(&value) {
                    Some(Self(value))
                } else {
                    None
                }
            }

            /// Returns the exact stable catalog identifier.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

define_catalog_key! {
    /// Stable identity for one compiler-known scope across source layout changes.
    pub struct CompilerKnownScopeKey;
}

define_catalog_key! {
    /// Stable identity for one recognized standard-library scope.
    pub struct RecognizedStandardLibraryScopeKey;
}

define_catalog_key! {
    /// Stable identity for one compiler-known declaration.
    pub struct CompilerKnownDeclarationKey;
}

define_catalog_key! {
    /// Stable identity for one compiler-known special value.
    pub struct CompilerKnownValueKey;
}

define_catalog_key! {
    /// Stable recognition identity for one standard-library declaration.
    pub struct RecognizedStandardLibraryDeclarationKey;
}

/// A validated non-empty catalog scope path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogPath(Arc<[Arc<str>]>);

impl CatalogPath {
    /// Creates a path from valid catalog identifier segments.
    pub fn try_new<T>(segments: impl IntoIterator<Item = T>) -> Option<Self>
    where
        T: Into<Arc<str>>,
    {
        let segments = segments.into_iter().map(shared_str).collect::<Vec<_>>();

        if segments.is_empty()
            || !segments
                .iter()
                .all(|segment| is_catalog_identifier(segment))
        {
            return None;
        }

        Some(Self(shared_slice(segments)))
    }

    /// Iterates over path segments in source order.
    pub fn segments(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0.iter().map(AsRef::as_ref)
    }
}

fn is_catalog_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();

    let Some(first) = bytes.next() else {
        return false;
    };

    first.is_ascii_alphabetic() && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogPath, CompilerKnownDeclarationKey, CompilerKnownScopeKey, CompilerKnownValueKey,
        RecognizedStandardLibraryDeclarationKey, RecognizedStandardLibraryScopeKey,
    };

    #[test]
    fn stable_keys_accept_only_catalog_identifiers() {
        assert_eq!(key("RawPointer").as_str(), "RawPointer");

        assert_eq!(CompilerKnownDeclarationKey::try_new(""), None);
        assert_eq!(CompilerKnownDeclarationKey::try_new("_RawPointer"), None);
        assert_eq!(CompilerKnownDeclarationKey::try_new("raw-pointer"), None);
        assert_eq!(CompilerKnownDeclarationKey::try_new("9Pointer"), None);
    }

    #[test]
    fn key_domains_are_independently_typed() {
        assert_eq!(compiler_known_scope("Ambient").as_str(), "Ambient");
        assert_eq!(
            recognized_scope("StandardLibrary").as_str(),
            "StandardLibrary"
        );
        assert_eq!(value("True").as_str(), "True");
        assert_eq!(recognized("StringLength").as_str(), "StringLength");
    }

    #[test]
    fn catalog_paths_are_non_empty_valid_identifier_sequences() {
        assert_eq!(CatalogPath::try_new(Vec::<&str>::new()), None);
        assert_eq!(CatalogPath::try_new(["core", "raw-memory"]), None);

        let path = match CatalogPath::try_new(["core", "memory"]) {
            Some(path) => path,
            None => panic!("test path is valid"),
        };

        assert_eq!(path.segments().collect::<Vec<_>>(), ["core", "memory"]);
    }

    fn key(value: &str) -> CompilerKnownDeclarationKey {
        match CompilerKnownDeclarationKey::try_new(value) {
            Some(key) => key,
            None => panic!("test declaration key is valid"),
        }
    }

    fn compiler_known_scope(value: &str) -> CompilerKnownScopeKey {
        match CompilerKnownScopeKey::try_new(value) {
            Some(key) => key,
            None => panic!("test scope key is valid"),
        }
    }

    fn recognized_scope(value: &str) -> RecognizedStandardLibraryScopeKey {
        match RecognizedStandardLibraryScopeKey::try_new(value) {
            Some(key) => key,
            None => panic!("test recognized scope key is valid"),
        }
    }

    fn value(value: &str) -> CompilerKnownValueKey {
        match CompilerKnownValueKey::try_new(value) {
            Some(key) => key,
            None => panic!("test value key is valid"),
        }
    }

    fn recognized(value: &str) -> RecognizedStandardLibraryDeclarationKey {
        match RecognizedStandardLibraryDeclarationKey::try_new(value) {
            Some(key) => key,
            None => panic!("test recognized key is valid"),
        }
    }
}
