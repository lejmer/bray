macro_rules! define_platform_service_roles {
    ($( $role:ident {
        $documentation:literal, $id:literal, $name:literal,
        $symbol:ident = $native:literal,
        $family:ident, [$($parameter:ident),*] -> $result:ident, bootstrap: ($($bootstrap:literal)?)
    })+) => {
        /// One closed platform-service role understood by the compiler, standard library, and runtime.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum PlatformServiceRole {
            $( #[doc = $documentation] $role, )+
        }

        impl PlatformServiceRole {
            /// Every platform-service role in stable numeric order.
            pub const ALL: &'static [Self] = &[$(Self::$role,)+];

            /// Returns the stable numeric role identity.
            pub const fn id(self) -> u32 {
                match self { $(Self::$role => $id,)+ }
            }

            /// Resolves one stable numeric role identity.
            #[deny(unreachable_patterns)]
            pub const fn from_id(id: u32) -> Option<Self> {
                match id { $($id => Some(Self::$role),)+ _ => None }
            }

            /// Returns this role's stable textual name.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$role => $name,)+ }
            }

            /// Resolves one stable textual role identity.
            #[deny(unreachable_patterns)]
            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$role),)+ _ => None }
            }

            /// Returns the independently retainable capability family.
            pub const fn family(self) -> crate::PlatformServiceFamily {
                match self { $(Self::$role => crate::PlatformServiceFamily::$family,)+ }
            }

            /// Returns the exact private callable shape required by this role.
            pub const fn signature(self) -> PlatformServiceSignature {
                match self {
                    $(Self::$role => PlatformServiceSignature::new(
                        &[$(PlatformAbiType::$parameter,)*], PlatformAbiType::$result,
                    ),)+
                }
            }

            /// Returns the trusted bootstrap declaration implementing this role.
            pub const fn bootstrap_declaration(self) -> Option<&'static str> {
                match self { $(Self::$role => define_platform_service_roles!(@bootstrap $($bootstrap)?),)+ }
            }

            /// Returns the stable native symbol provided for this role.
            pub const fn native_symbol(self) -> &'static str {
                match self { $(Self::$role => bray_runtime_abi::symbols::$symbol,)+ }
            }
        }
    };
    (@bootstrap $name:literal) => { Some($name) };
    (@bootstrap) => { None };
}

bray_runtime_abi::platform_role_catalog!(define_platform_service_roles);

pub use bray_runtime_abi::PlatformAbiType;

/// The exact parameter and result shape of one platform-service role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformServiceSignature {
    parameters: &'static [PlatformAbiType],
    result: PlatformAbiType,
}

impl PlatformServiceSignature {
    const fn new(parameters: &'static [PlatformAbiType], result: PlatformAbiType) -> Self {
        Self { parameters, result }
    }

    /// Returns parameter ABI kinds in calling order.
    pub const fn parameters(self) -> &'static [PlatformAbiType] {
        self.parameters
    }

    /// Returns the result ABI kind.
    pub const fn result(self) -> PlatformAbiType {
        self.result
    }
}

/// An explicit product association between a private declaration and platform role.
pub type PlatformServiceBinding = crate::SourceRoleBinding<PlatformServiceRole>;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{PlatformAbiType, PlatformServiceBinding, PlatformServiceRole};

    #[test]
    fn complete_platform_role_catalog_has_unique_identities() {
        let ids = PlatformServiceRole::ALL
            .iter()
            .map(|role| role.id())
            .collect::<BTreeSet<_>>();

        let names = PlatformServiceRole::ALL
            .iter()
            .map(|role| role.as_str())
            .collect::<BTreeSet<_>>();

        let symbols = PlatformServiceRole::ALL
            .iter()
            .map(|role| role.native_symbol())
            .collect::<BTreeSet<_>>();

        assert_eq!(ids.len(), PlatformServiceRole::ALL.len());
        assert_eq!(names.len(), PlatformServiceRole::ALL.len());
        assert_eq!(symbols.len(), PlatformServiceRole::ALL.len());

        for role in PlatformServiceRole::ALL {
            assert_eq!(PlatformServiceRole::from_id(role.id()), Some(*role));
            assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(*role));
        }
    }

    #[test]
    fn platform_roles_have_stable_names_ids_and_shapes() {
        let role = PlatformServiceRole::StandardOutputWrite;
        let input_lock = PlatformServiceRole::StandardInputLock;

        assert_eq!(role.id(), 0x0111);
        assert_eq!(role.as_str(), "platform.standard_output.write");
        assert_eq!(PlatformServiceRole::from_id(role.id()), Some(role));
        assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));

        assert_eq!(
            role.signature().parameters(),
            [
                PlatformAbiType::PointerU8,
                PlatformAbiType::U64,
                PlatformAbiType::PointerU64,
            ]
        );

        assert_eq!(role.signature().result(), PlatformAbiType::Status);

        assert_eq!(input_lock.id(), 0x0102);
        assert_eq!(input_lock.as_str(), "platform.standard_input.lock");

        assert_eq!(
            PlatformServiceRole::from_id(input_lock.id()),
            Some(input_lock)
        );

        assert_eq!(
            PlatformServiceRole::from_name(input_lock.as_str()),
            Some(input_lock)
        );

        assert!(input_lock.signature().parameters().is_empty());
        assert_eq!(input_lock.signature().result(), PlatformAbiType::Status);
        assert_eq!(PlatformServiceRole::from_id(0), None);
        assert_eq!(PlatformServiceRole::from_id(u32::MAX), None);
    }

    #[test]
    fn platform_scalar_role_shapes_are_exact() {
        let identity = PlatformServiceRole::ContextIdentity.signature();
        let width = PlatformServiceRole::ContextNativeTextWidth.signature();
        let monotonic = PlatformServiceRole::ClockMonotonicNow.signature();

        assert!(identity.parameters().is_empty());
        assert_eq!(identity.result(), PlatformAbiType::U64);
        assert!(width.parameters().is_empty());
        assert_eq!(width.result(), PlatformAbiType::U32);
        assert_eq!(monotonic.parameters(), [PlatformAbiType::PointerU64]);
        assert_eq!(monotonic.result(), PlatformAbiType::Status);
    }

    #[test]
    fn child_spawn_role_has_closed_request_and_owner_outputs() {
        let role = PlatformServiceRole::ChildSpawn;

        assert_eq!(role.id(), 0x0311);
        assert_eq!(role.as_str(), "platform.child.spawn");
        assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));

        assert_eq!(
            role.signature().parameters(),
            [
                PlatformAbiType::ChildRequest,
                PlatformAbiType::PointerU64,
                PlatformAbiType::PointerU64,
                PlatformAbiType::PointerU64,
                PlatformAbiType::PointerU64,
            ]
        );

        assert_eq!(role.signature().result(), PlatformAbiType::Status);
    }

    #[test]
    fn thread_storage_roles_have_closed_bootstrap_shapes() {
        let create = PlatformServiceRole::ThreadStorageCreate;
        let load = PlatformServiceRole::ThreadStorageLoad;
        let store = PlatformServiceRole::ThreadStorageStore;
        let destroy = PlatformServiceRole::ThreadStorageDestroy;

        assert_eq!(create.id(), 0x0331);
        assert_eq!(create.as_str(), "platform.thread_storage.create");

        assert_eq!(
            create.signature().parameters(),
            [PlatformAbiType::PointerU8, PlatformAbiType::PointerU64]
        );

        assert_eq!(load.id(), 0x0332);
        assert_eq!(load.as_str(), "platform.thread_storage.load");

        assert_eq!(
            load.signature().parameters(),
            [PlatformAbiType::U64, PlatformAbiType::RawAddressPointer]
        );

        assert_eq!(store.id(), 0x0333);
        assert_eq!(store.as_str(), "platform.thread_storage.store");

        assert_eq!(
            store.signature().parameters(),
            [PlatformAbiType::U64, PlatformAbiType::PointerU8]
        );

        assert_eq!(destroy.id(), 0x0334);
        assert_eq!(destroy.as_str(), "platform.thread_storage.destroy");
        assert_eq!(destroy.signature().parameters(), [PlatformAbiType::U64]);

        for role in [create, load, store, destroy] {
            assert_eq!(role.signature().result(), PlatformAbiType::Status);
            assert_eq!(PlatformServiceRole::from_id(role.id()), Some(role));
            assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));
        }
    }

    #[test]
    fn dynamic_symbol_role_has_closed_handle_name_and_address_shape() {
        let role = PlatformServiceRole::DynamicLibrarySymbol;

        assert_eq!(role.id(), 0x0803);
        assert_eq!(role.as_str(), "platform.dynamic_library.symbol");
        assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));

        assert_eq!(
            role.signature().parameters(),
            [
                PlatformAbiType::U64,
                PlatformAbiType::PointerU8,
                PlatformAbiType::U64,
                PlatformAbiType::RawAddressPointer,
            ]
        );

        assert_eq!(role.signature().result(), PlatformAbiType::Status);
    }

    #[test]
    fn platform_bindings_require_module_qualified_declarations() {
        let Some(binding) = PlatformServiceBinding::try_new(
            PlatformServiceRole::StandardOutputFlush,
            "std.io.platform_standard_output_flush",
        ) else {
            panic!("test binding path must be valid");
        };

        assert_eq!(binding.module().collect::<Vec<_>>(), ["std", "io"]);
        assert_eq!(binding.declaration(), "platform_standard_output_flush");

        assert_eq!(
            binding.dotted_path(),
            "std.io.platform_standard_output_flush"
        );

        assert_eq!(
            PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "flush"),
            None
        );
    }
}
