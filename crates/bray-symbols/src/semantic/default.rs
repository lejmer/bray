use std::{collections::BTreeSet, sync::Arc};

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;

use crate::{
    AnySymbolId, BorrowKind, CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
    DependencyContractTemplateId, ExternalSymbolKey, GenericParameterSymbolId,
    GenericSubstitutionId, ImportedInterfaceId, InterfaceSupportEntityId, LifecycleObligationKind,
    ReceiverParameterSymbolId, StructFieldDefaultProviderSymbolId, StructFieldSymbolId,
    TrustedCapabilitySymbolId, TypeId, UnionPayloadDefaultProviderSymbolId,
    UnionPayloadFieldSymbolId,
};

/// One explicit callable input available while evaluating a parameter default.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeDefaultProviderInput {
    /// The callable's implicit receiver.
    Receiver(ReceiverParameterSymbolId),
    /// A callable parameter declared before the parameter owning the default.
    EarlierParameter(CallableParameterSymbolId),
}

/// Generic parameters and the checked substitution visible to a runtime-default provider.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeDefaultGenericContext {
    /// The provider has no generic context.
    NonGeneric,
    /// The provider retains an ordered generic parameter surface and substitution.
    Generic(RuntimeDefaultGenericArguments),
}

impl RuntimeDefaultGenericContext {
    /// Creates a generic context when at least one distinct parameter is supplied.
    pub fn generic(
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        substitution: GenericSubstitutionId,
    ) -> Option<Self> {
        RuntimeDefaultGenericArguments::try_new(parameters, substitution).map(Self::Generic)
    }

    /// Returns generic parameters in declaration order.
    pub fn parameters(&self) -> &[GenericParameterSymbolId] {
        match self {
            Self::NonGeneric => &[],
            Self::Generic(arguments) => arguments.parameters(),
        }
    }

    /// Returns the checked substitution when the provider is generic.
    pub const fn substitution(&self) -> Option<GenericSubstitutionId> {
        match self {
            Self::NonGeneric => None,
            Self::Generic(arguments) => Some(arguments.substitution()),
        }
    }
}

/// A validated non-empty ordered runtime-default generic context.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeDefaultGenericArguments {
    parameters: Arc<[GenericParameterSymbolId]>,
    substitution: GenericSubstitutionId,
}

impl RuntimeDefaultGenericArguments {
    fn try_new(
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        substitution: GenericSubstitutionId,
    ) -> Option<Self> {
        let parameters: Vec<_> = parameters.into_iter().collect();

        if parameters.is_empty() || !all_distinct(&parameters) {
            return None;
        }

        Some(Self {
            parameters: shared_slice(parameters),
            substitution,
        })
    }

    /// Returns generic parameters in declaration order.
    pub fn parameters(&self) -> &[GenericParameterSymbolId] {
        &self.parameters
    }

    /// Returns the checked open or concrete substitution.
    pub const fn substitution(&self) -> GenericSubstitutionId {
        self.substitution
    }
}

/// The ownership disposition of a value produced by a runtime default.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeDefaultOwnership {
    /// The provider produces a newly owned value.
    Owned,
    /// The provider produces a copied value.
    Copied,
    /// The provider produces a borrow with the exact capability.
    Borrowed(BorrowKind),
}

macro_rules! define_runtime_default_requirement {
    ($name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(AnySymbolId);

        impl $name {
            /// Creates a requirement for one exact semantic declaration.
            pub const fn new(declaration: AnySymbolId) -> Self {
                Self(declaration)
            }

            /// Returns the exact declaration that defines the requirement.
            pub const fn declaration(self) -> AnySymbolId {
                self.0
            }
        }
    };
}

define_runtime_default_requirement!(
    RuntimeDefaultEffectRequirement,
    "One checked effect required or produced while evaluating a runtime default."
);
define_runtime_default_requirement!(
    RuntimeDefaultCapabilityRequirement,
    "One checked capability required while evaluating a runtime default."
);
/// One trusted implementation capability retained by a runtime-default provider.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeDefaultTrustedObligation(TrustedCapabilitySymbolId);

impl RuntimeDefaultTrustedObligation {
    /// Creates a requirement for one exact trusted capability.
    pub const fn new(declaration: TrustedCapabilitySymbolId) -> Self {
        Self(declaration)
    }

    /// Returns the exact trusted capability that defines the requirement.
    pub const fn declaration(self) -> TrustedCapabilitySymbolId {
        self.0
    }
}

/// Effects, capabilities, ownership, borrowing, and lifecycle behavior of a runtime default.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeDefaultBehavior {
    ownership: RuntimeDefaultOwnership,
    effects: Arc<[RuntimeDefaultEffectRequirement]>,
    capabilities: Arc<[RuntimeDefaultCapabilityRequirement]>,
    trusted_obligations: Arc<[RuntimeDefaultTrustedObligation]>,
    lifecycle_obligations: Arc<[LifecycleObligationKind]>,
    dependency_contract: DependencyContractTemplateId,
}

impl RuntimeDefaultBehavior {
    /// Creates the complete checked behavior retained by a runtime-default provider.
    pub fn new(
        ownership: RuntimeDefaultOwnership,
        effects: impl IntoIterator<Item = RuntimeDefaultEffectRequirement>,
        capabilities: impl IntoIterator<Item = RuntimeDefaultCapabilityRequirement>,
        trusted_obligations: impl IntoIterator<Item = RuntimeDefaultTrustedObligation>,
        lifecycle_obligations: impl IntoIterator<Item = LifecycleObligationKind>,
        dependency_contract: DependencyContractTemplateId,
    ) -> Self {
        Self {
            ownership,
            effects: shared_slice(effects),
            capabilities: shared_slice(capabilities),
            trusted_obligations: shared_slice(trusted_obligations),
            lifecycle_obligations: shared_slice(lifecycle_obligations),
            dependency_contract,
        }
    }

    /// Returns the produced value's ownership and borrowing disposition.
    pub const fn ownership(&self) -> RuntimeDefaultOwnership {
        self.ownership
    }

    /// Returns checked effects in canonical semantic order.
    pub fn effects(&self) -> &[RuntimeDefaultEffectRequirement] {
        &self.effects
    }

    /// Returns checked capabilities in canonical semantic order.
    pub fn capabilities(&self) -> &[RuntimeDefaultCapabilityRequirement] {
        &self.capabilities
    }

    /// Returns trusted obligations in canonical semantic order.
    pub fn trusted_obligations(&self) -> &[RuntimeDefaultTrustedObligation] {
        &self.trusted_obligations
    }

    /// Returns lifecycle obligations in canonical semantic order.
    pub fn lifecycle_obligations(&self) -> &[LifecycleObligationKind] {
        &self.lifecycle_obligations
    }

    /// Returns normalized contextual, borrow, and storage dependencies.
    pub const fn dependency_contract(&self) -> DependencyContractTemplateId {
        self.dependency_contract
    }
}

/// Identifies the semantic template that supplies one runtime default.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeDefaultTemplateReference {
    /// A source-backed template selected by its declaration-owned expression anchor.
    ///
    /// Consumers request the runtime-default semantics for this anchor and must not rebind the syntax
    /// independently.
    Source(SyntaxAnchor),
    /// A stable template record in a loaded compiled package interface.
    Interface {
        /// The loaded package interface.
        interface: ImportedInterfaceId,
        /// The private support entity containing the checked template.
        entity: InterfaceSupportEntityId,
    },
    /// A stable source-independent compiler-known or synthesized provider key.
    External(ExternalSymbolKey),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RuntimeDefaultSurfaceData {
    inputs: Arc<[RuntimeDefaultProviderInput]>,
    generic_context: RuntimeDefaultGenericContext,
    result: TypeId,
    behavior: RuntimeDefaultBehavior,
    template: RuntimeDefaultTemplateReference,
}

impl RuntimeDefaultSurfaceData {
    fn new(
        inputs: impl IntoIterator<Item = RuntimeDefaultProviderInput>,
        generic_context: RuntimeDefaultGenericContext,
        result: TypeId,
        behavior: RuntimeDefaultBehavior,
        template: RuntimeDefaultTemplateReference,
    ) -> Self {
        Self {
            inputs: shared_slice(inputs),
            generic_context,
            result,
            behavior,
            template,
        }
    }
}

macro_rules! define_runtime_default_contract {
    (
        surface: $surface:ident,
        error: $error:ident,
        value: $value:ident,
        checked: $checked:ident,
        owner: $owner:ty,
        provider: $provider:ty,
    ) => {
        #[doc = concat!("The checked semantic surface of one `", stringify!($surface), "`.")]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $surface(RuntimeDefaultSurfaceData);

        impl $surface {
            /// Returns ordered contextual provider inputs.
            pub fn inputs(&self) -> &[RuntimeDefaultProviderInput] {
                &self.0.inputs
            }

            /// Returns generic parameters and substitution context.
            pub const fn generic_context(&self) -> &RuntimeDefaultGenericContext {
                &self.0.generic_context
            }

            /// Returns the checked result type.
            pub const fn result(&self) -> TypeId {
                self.0.result
            }

            /// Returns ownership, effects, capabilities, trust, and lifecycle behavior.
            pub const fn behavior(&self) -> &RuntimeDefaultBehavior {
                &self.0.behavior
            }

            /// Returns the source-backed or source-independent checked-template reference.
            pub const fn template_reference(&self) -> &RuntimeDefaultTemplateReference {
                &self.0.template
            }
        }

        #[doc = concat!("Marks an invalid `", stringify!($surface), "` whose diagnostics belong to the query result.")]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $error;

        #[doc = concat!("The valid or error-aware value of one `", stringify!($surface), "`.")]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum $value {
            /// A valid checked runtime-default surface.
            Valid($surface),
            /// Checking failed and diagnostics are retained by the query result.
            Error($error),
        }

        #[doc = concat!("The owner, provider identity, and checked value of one `", stringify!($surface), "`.")]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $checked {
            owner: $owner,
            provider: $provider,
            value: $value,
        }

        impl $checked {
            /// Creates a checked runtime-default value.
            pub const fn new(owner: $owner, provider: $provider, value: $value) -> Self {
                Self {
                    owner,
                    provider,
                    value,
                }
            }

            /// Returns the exact parameter or field that owns the default.
            pub const fn owner(&self) -> $owner {
                self.owner
            }

            /// Returns the exact synthesized provider identity.
            pub const fn provider(&self) -> $provider {
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
    owner: CallableParameterSymbolId,
    provider: CallableParameterDefaultProviderSymbolId,
}

impl CallableParameterDefaultSurface {
    /// Creates a checked parameter-default surface with permitted contextual inputs.
    pub fn new(
        receiver: Option<ReceiverParameterSymbolId>,
        earlier_parameters: impl IntoIterator<Item = CallableParameterSymbolId>,
        generic_context: RuntimeDefaultGenericContext,
        result: TypeId,
        behavior: RuntimeDefaultBehavior,
        template: RuntimeDefaultTemplateReference,
    ) -> Self {
        let inputs = receiver
            .map(RuntimeDefaultProviderInput::Receiver)
            .into_iter()
            .chain(
                earlier_parameters
                    .into_iter()
                    .map(RuntimeDefaultProviderInput::EarlierParameter),
            );

        Self(RuntimeDefaultSurfaceData::new(
            inputs,
            generic_context,
            result,
            behavior,
            template,
        ))
    }
}

define_runtime_default_contract! {
    surface: StructFieldDefaultSurface,
    error: ErrorStructFieldDefault,
    value: StructFieldDefaultValue,
    checked: CheckedStructFieldDefault,
    owner: StructFieldSymbolId,
    provider: StructFieldDefaultProviderSymbolId,
}

impl StructFieldDefaultSurface {
    /// Creates a checked field-default surface without receiver or sibling inputs.
    pub fn new(
        generic_context: RuntimeDefaultGenericContext,
        result: TypeId,
        behavior: RuntimeDefaultBehavior,
        template: RuntimeDefaultTemplateReference,
    ) -> Self {
        Self(RuntimeDefaultSurfaceData::new(
            [],
            generic_context,
            result,
            behavior,
            template,
        ))
    }
}

define_runtime_default_contract! {
    surface: UnionPayloadDefaultSurface,
    error: ErrorUnionPayloadDefault,
    value: UnionPayloadDefaultValue,
    checked: CheckedUnionPayloadDefault,
    owner: UnionPayloadFieldSymbolId,
    provider: UnionPayloadDefaultProviderSymbolId,
}

impl UnionPayloadDefaultSurface {
    /// Creates a checked payload-default surface without receiver or sibling inputs.
    pub fn new(
        generic_context: RuntimeDefaultGenericContext,
        result: TypeId,
        behavior: RuntimeDefaultBehavior,
        template: RuntimeDefaultTemplateReference,
    ) -> Self {
        Self(RuntimeDefaultSurfaceData::new(
            [],
            generic_context,
            result,
            behavior,
            template,
        ))
    }
}

fn all_distinct<T: Ord>(items: &[T]) -> bool {
    let mut seen = BTreeSet::new();

    items.iter().all(|item| seen.insert(item))
}

#[cfg(test)]
mod tests {
    use crate::{
        AnySymbolId, BorrowKind, CallableParameterDefaultProviderSymbolId,
        CallableParameterDefaultSurface, CallableParameterDefaultValue, CallableParameterSymbolId,
        DependencyContractTemplateData, FunctionSymbolId, ImportedInterfaceId,
        InterfaceSupportEntityId, LifecycleObligationKind, ReceiverParameterSymbolId,
        RuntimeDefaultBehavior, RuntimeDefaultCapabilityRequirement,
        RuntimeDefaultEffectRequirement, RuntimeDefaultGenericContext, RuntimeDefaultOwnership,
        RuntimeDefaultProviderInput, RuntimeDefaultTemplateReference,
        RuntimeDefaultTrustedObligation, SemanticValueStore, StructFieldDefaultProviderSymbolId,
        StructFieldDefaultValue, StructFieldSymbolId, SymbolId, TrustedCapabilitySymbolId,
        TypeData, UnionPayloadDefaultProviderSymbolId, UnionPayloadDefaultValue,
        UnionPayloadFieldSymbolId,
    };

    use super::{
        CheckedCallableParameterDefault, CheckedStructFieldDefault, CheckedUnionPayloadDefault,
    };

    #[test]
    fn owner_specific_defaults_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedCallableParameterDefault>();
        assert_send_sync::<CheckedStructFieldDefault>();
        assert_send_sync::<CheckedUnionPayloadDefault>();
    }

    #[test]
    fn field_and_payload_results_retain_exact_owner_and_provider_categories() {
        let field = StructFieldSymbolId::from_symbol_id(SymbolId::new(1));
        let field_provider = StructFieldDefaultProviderSymbolId::from_symbol_id(SymbolId::new(2));

        let field_default = CheckedStructFieldDefault::new(
            field,
            field_provider,
            StructFieldDefaultValue::Error(super::ErrorStructFieldDefault),
        );

        let payload = UnionPayloadFieldSymbolId::from_symbol_id(SymbolId::new(3));

        let payload_provider =
            UnionPayloadDefaultProviderSymbolId::from_symbol_id(SymbolId::new(4));

        let payload_default = CheckedUnionPayloadDefault::new(
            payload,
            payload_provider,
            UnionPayloadDefaultValue::Error(super::ErrorUnionPayloadDefault),
        );

        assert_eq!(field_default.owner(), field);
        assert_eq!(field_default.provider(), field_provider);

        assert_eq!(payload_default.owner(), payload);
        assert_eq!(payload_default.provider(), payload_provider);
    }

    #[test]
    fn checked_parameter_defaults_retain_complete_provider_surface() {
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

        let declaration = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(1)));

        let effect = RuntimeDefaultEffectRequirement::new(declaration);
        let capability = RuntimeDefaultCapabilityRequirement::new(declaration);

        let trusted = RuntimeDefaultTrustedObligation::new(
            TrustedCapabilitySymbolId::from_symbol_id(SymbolId::new(2)),
        );

        let behavior = RuntimeDefaultBehavior::new(
            RuntimeDefaultOwnership::Borrowed(BorrowKind::Shared),
            [effect],
            [capability],
            [trusted],
            [LifecycleObligationKind::Finalization],
            dependencies,
        );

        let owner = CallableParameterSymbolId::from_symbol_id(SymbolId::new(4));
        let earlier = CallableParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let receiver = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let provider = CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(5));

        let template = RuntimeDefaultTemplateReference::Interface {
            interface: ImportedInterfaceId::new(2),
            entity: InterfaceSupportEntityId::new(7),
        };

        let surface = CallableParameterDefaultSurface::new(
            Some(receiver),
            [earlier],
            RuntimeDefaultGenericContext::NonGeneric,
            result,
            behavior,
            template,
        );

        assert_eq!(
            surface.inputs(),
            &[
                RuntimeDefaultProviderInput::Receiver(receiver),
                RuntimeDefaultProviderInput::EarlierParameter(earlier),
            ]
        );

        assert!(surface.generic_context().parameters().is_empty());
        assert_eq!(surface.result(), result);

        assert_eq!(
            surface.behavior().ownership(),
            RuntimeDefaultOwnership::Borrowed(BorrowKind::Shared)
        );

        assert_eq!(surface.behavior().effects(), &[effect]);
        assert_eq!(surface.behavior().capabilities(), &[capability]);
        assert_eq!(surface.behavior().trusted_obligations(), &[trusted]);

        assert_eq!(
            surface.behavior().lifecycle_obligations(),
            &[LifecycleObligationKind::Finalization]
        );

        assert_eq!(surface.behavior().dependency_contract(), dependencies);
        assert_eq!(surface.template_reference(), &interface_template());

        let value = CallableParameterDefaultValue::Valid(surface);
        let checked = CheckedCallableParameterDefault::new(owner, provider, value);

        assert_eq!(checked.owner(), owner);
        assert_eq!(checked.provider(), provider);
    }

    #[test]
    fn field_and_payload_surfaces_cannot_retain_contextual_inputs() {
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

        let field = super::StructFieldDefaultSurface::new(
            RuntimeDefaultGenericContext::NonGeneric,
            result,
            empty_owned_behavior(dependencies),
            interface_template(),
        );

        let payload = super::UnionPayloadDefaultSurface::new(
            RuntimeDefaultGenericContext::NonGeneric,
            result,
            empty_owned_behavior(dependencies),
            interface_template(),
        );

        assert!(field.inputs().is_empty());
        assert!(payload.inputs().is_empty());
    }

    #[test]
    fn generic_contexts_retain_ordered_parameters_and_substitution() {
        use crate::{
            GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
            GenericTypeParameterSymbolId,
        };

        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(ty) = store.intern_type(TypeData::Error) else {
            panic!("error type must be valid");
        };

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(1));
        let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(2));

        let Some(owner) = GenericOwnerId::try_new(function.into()) else {
            panic!("function must be a generic owner");
        };

        let Ok(substitution) = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::from(parameter)],
            [GenericArgument::Type(ty)],
        ) else {
            panic!("matching generic parameter and argument must be valid");
        };

        let Ok(substitution) = store.intern_generic_substitution(substitution) else {
            panic!("generic substitution must be internable");
        };

        let Some(context) = RuntimeDefaultGenericContext::generic(
            [GenericParameterSymbolId::from(parameter)],
            substitution,
        ) else {
            panic!("one distinct generic parameter must form a generic context");
        };

        assert_eq!(context.parameters(), &[parameter.into()]);
        assert_eq!(context.substitution(), Some(substitution));

        assert!(
            RuntimeDefaultGenericContext::generic(
                [
                    GenericParameterSymbolId::from(parameter),
                    GenericParameterSymbolId::from(parameter),
                ],
                substitution,
            )
            .is_none()
        );

        assert!(RuntimeDefaultGenericContext::generic([], substitution).is_none());
    }

    fn empty_owned_behavior(
        dependencies: crate::DependencyContractTemplateId,
    ) -> RuntimeDefaultBehavior {
        RuntimeDefaultBehavior::new(RuntimeDefaultOwnership::Owned, [], [], [], [], dependencies)
    }

    fn interface_template() -> RuntimeDefaultTemplateReference {
        RuntimeDefaultTemplateReference::Interface {
            interface: ImportedInterfaceId::new(2),
            entity: InterfaceSupportEntityId::new(7),
        }
    }
}
