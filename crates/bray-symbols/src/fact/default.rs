use crate::{
    CallableParameterDefaultProviderSymbolId, DependencyContractTemplateId,
    StructFieldDefaultProviderSymbolId, TypeId, UnionPayloadDefaultProviderSymbolId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RuntimeDefaultSurfaceData {
    result: TypeId,
    dependency_contract: DependencyContractTemplateId,
}

impl RuntimeDefaultSurfaceData {
    const fn new(result: TypeId, dependency_contract: DependencyContractTemplateId) -> Self {
        Self {
            result,
            dependency_contract,
        }
    }
}

macro_rules! define_runtime_default_contract {
    (
        surface: $surface:ident,
        error: $error:ident,
        value: $value:ident,
        checked: $checked:ident,
        provider: $provider:ty,
    ) => {
        #[doc = concat!("The checked semantic surface of one `", stringify!($surface), "`.")]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $surface(RuntimeDefaultSurfaceData);

        impl $surface {
            /// Creates a valid checked runtime-default surface.
            pub const fn new(
                result: TypeId,
                dependency_contract: DependencyContractTemplateId,
            ) -> Self {
                Self(RuntimeDefaultSurfaceData::new(result, dependency_contract))
            }

            /// Returns the checked result type.
            pub const fn result(self) -> TypeId {
                self.0.result
            }

            /// Returns the normalized portable dependency contract.
            pub const fn dependency_contract(self) -> DependencyContractTemplateId {
                self.0.dependency_contract
            }
        }

        #[doc = concat!("Marks an invalid `", stringify!($surface), "` whose diagnostics belong to the fact result.")]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $error;

        #[doc = concat!("The valid or error-aware value of one `", stringify!($surface), "`.")]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum $value {
            /// A valid checked runtime-default surface.
            Valid($surface),
            /// Checking failed and diagnostics are retained by the fact result.
            Error($error),
        }

        #[doc = concat!("The provider identity and checked value of one `", stringify!($surface), "`.")]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $checked {
            provider: $provider,
            value: $value,
        }

        impl $checked {
            /// Creates a checked runtime-default fact value.
            pub const fn new(provider: $provider, value: $value) -> Self {
                Self { provider, value }
            }

            /// Returns the exact synthesized provider identity.
            pub const fn provider(self) -> $provider {
                self.provider
            }

            /// Returns the valid or error-aware checked value.
            pub const fn value(&self) -> &$value {
                &self.value
            }
        }
    };
}

define_runtime_default_contract! {
    surface: CallableParameterDefaultSurface,
    error: ErrorCallableParameterDefault,
    value: CallableParameterDefaultValue,
    checked: CheckedCallableParameterDefault,
    provider: CallableParameterDefaultProviderSymbolId,
}

define_runtime_default_contract! {
    surface: StructFieldDefaultSurface,
    error: ErrorStructFieldDefault,
    value: StructFieldDefaultValue,
    checked: CheckedStructFieldDefault,
    provider: StructFieldDefaultProviderSymbolId,
}

define_runtime_default_contract! {
    surface: UnionPayloadDefaultSurface,
    error: ErrorUnionPayloadDefault,
    value: UnionPayloadDefaultValue,
    checked: CheckedUnionPayloadDefault,
    provider: UnionPayloadDefaultProviderSymbolId,
}

#[cfg(test)]
mod tests {
    use crate::{
        CallableParameterDefaultProviderSymbolId, CallableParameterDefaultSurface,
        CallableParameterDefaultValue, DependencyContractTemplateData, SemanticValueStore,
        SymbolId, TypeData,
    };

    use super::{
        CheckedCallableParameterDefault, CheckedStructFieldDefault, CheckedUnionPayloadDefault,
    };

    #[test]
    fn owner_specific_default_facts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedCallableParameterDefault>();
        assert_send_sync::<CheckedStructFieldDefault>();
        assert_send_sync::<CheckedUnionPayloadDefault>();
    }

    #[test]
    fn checked_defaults_keep_exact_provider_and_semantic_surface() {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(result) = store.intern_type(TypeData::Error) else {
            panic!("error type must be valid");
        };

        let Ok(dependencies) =
            store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        else {
            panic!("empty dependency contract must be valid");
        };

        let provider = CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(4));

        let surface = CallableParameterDefaultSurface::new(result, dependencies);

        let value = CallableParameterDefaultValue::Valid(surface);
        let checked = CheckedCallableParameterDefault::new(provider, value);

        assert_eq!(checked.provider(), provider);
        assert_eq!(checked.value(), &value);
        assert_eq!(surface.result(), result);
        assert_eq!(surface.dependency_contract(), dependencies);
    }
}
