use crate::record::ImportedSymbolBacking;
use crate::{AnySymbolId, ImportedSemanticKey, SymbolKey, SymbolOrigin};

macro_rules! define_default_provider_record {
    ($record:ident, $id:ident, $subject:ident) => {
        #[doc = concat!("The synthesized declaration record for a `", stringify!($record), "`.")]
        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
        pub struct $record {
            id: crate::$id,
            key: SymbolKey,
            containing_symbol: AnySymbolId,
            subject: crate::$subject,
            imported: Option<ImportedSymbolBacking>,
        }

        impl $record {
            pub(crate) const fn new(
                id: crate::$id,
                key: SymbolKey,
                containing_symbol: AnySymbolId,
                subject: crate::$subject,
            ) -> Self {
                Self {
                    id,
                    key,
                    containing_symbol,
                    subject,
                    imported: None,
                }
            }

            pub(crate) const fn new_imported(
                id: crate::$id,
                key: SymbolKey,
                containing_symbol: AnySymbolId,
                subject: crate::$subject,
                imported: ImportedSymbolBacking,
            ) -> Self {
                Self {
                    id,
                    key,
                    containing_symbol,
                    subject,
                    imported: Some(imported),
                }
            }

            /// Returns this provider's exact compilation-local ID.
            pub const fn id(&self) -> crate::$id {
                self.id
            }

            /// Returns this provider's deterministic synthesized key.
            pub const fn key(&self) -> &SymbolKey {
                &self.key
            }

            /// Returns this provider's synthesized origin.
            pub const fn origin(&self) -> SymbolOrigin {
                if self.imported.is_some() {
                    SymbolOrigin::Imported
                } else {
                    SymbolOrigin::Synthesized
                }
            }

            /// Returns this provider's semantic container.
            pub const fn containing_symbol(&self) -> AnySymbolId {
                self.containing_symbol
            }

            /// Returns the parameter or field whose default this provider evaluates.
            pub const fn subject(&self) -> crate::$subject {
                self.subject
            }

            /// Returns the imported semantic key, when available.
            pub fn imported_semantic_key(&self) -> Option<ImportedSemanticKey<crate::$id>> {
                self.imported.map(ImportedSymbolBacking::semantic_key)
            }
        }
    };
}

define_default_provider_record!(
    CallableParameterDefaultProviderSymbol,
    CallableParameterDefaultProviderSymbolId,
    CallableParameterSymbolId
);
define_default_provider_record!(
    StructFieldDefaultProviderSymbol,
    StructFieldDefaultProviderSymbolId,
    StructFieldSymbolId
);
define_default_provider_record!(
    UnionPayloadDefaultProviderSymbol,
    UnionPayloadDefaultProviderSymbolId,
    UnionPayloadFieldSymbolId
);
