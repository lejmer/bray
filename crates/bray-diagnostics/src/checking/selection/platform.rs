use crate::{DiagnosticCallableExecution, DiagnosticType};

macro_rules! define_diagnostic_platform_roles {
    ($( $role:ident {
        $documentation:literal, $id:literal, $name:literal,
        $symbol:ident = $native:literal,
        $family:ident, [$($parameter:ident),*] -> $result:ident, bootstrap: ($($bootstrap:literal)?)
    })+) => {
        /// Validated stable identity of one compiler-defined platform service role.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum DiagnosticPlatformServiceRole {
            $( #[doc = $documentation] $role, )+
        }

        impl DiagnosticPlatformServiceRole {
            /// Every platform role that can be carried by a diagnostic.
            pub const ALL: &'static [Self] = &[$(Self::$role,)+];

            /// Creates a role only when `id` belongs to the platform-service catalog.
            #[deny(unreachable_patterns)]
            pub const fn try_new(id: u32) -> Option<Self> {
                match id { $($id => Some(Self::$role),)+ _ => None }
            }

            /// Returns the stable platform-service role ID.
            pub const fn id(self) -> u32 {
                match self { $(Self::$role => $id,)+ }
            }
        }
    };
}

bray_runtime_abi::platform_role_catalog!(define_diagnostic_platform_roles);

/// One ABI value kind in the closed platform-service callable schema.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticPlatformAbiType {
    /// A 32-bit signed integer value.
    I32,
    /// A 32-bit unsigned integer value.
    U32,
    /// A 64-bit unsigned integer value.
    U64,
    /// A 64-bit signed integer value.
    I64,
    /// A raw pointer to 8-bit unsigned values.
    PointerU8,
    /// A raw pointer to 32-bit unsigned values.
    PointerU32,
    /// A raw pointer to 64-bit unsigned values.
    PointerU64,
    /// A raw pointer to 64-bit signed values.
    PointerI64,
    /// A pointer through which a raw address is returned.
    RawAddressPointer,
    /// The runtime platform-path value.
    Path,
    /// The runtime native-text value.
    NativeText,
    /// The runtime file-open options value.
    FileOptions,
    /// A pointer through which file metadata is returned.
    FileMetadataPointer,
    /// The runtime child-process request value.
    ChildRequest,
    /// A pointer through which a child exit status is returned.
    ExitStatusPointer,
    /// The runtime calendar date-and-time value.
    TemporalDateTime,
    /// A pointer to a runtime calendar date-and-time value.
    TemporalDateTimePointer,
    /// A pointer through which a clock observation is returned.
    TemporalObservationPointer,
    /// A pointer through which a local-time resolution is returned.
    TemporalResolutionPointer,
    /// The runtime temporal scalar value.
    TemporalValue,
    /// A pointer to a runtime temporal scalar value.
    TemporalValuePointer,
    /// The runtime platform-operation status value.
    Status,
}

/// Exact callable-surface mismatch for one compiler-defined platform service.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticPlatformServiceSignatureProblem {
    /// Callable ABI differs from the required C boundary.
    CallableAbi {
        /// Compiler-defined role whose callable surface was checked.
        role: DiagnosticPlatformServiceRole,
        /// ABI declared by the callable.
        actual: crate::DiagnosticCallableAbi,
    },
    /// Callable execution mode differs from the required synchronous boundary.
    Execution {
        /// Compiler-defined role whose callable surface was checked.
        role: DiagnosticPlatformServiceRole,
        /// Execution mode declared by the callable.
        actual: DiagnosticCallableExecution,
    },
    /// Callable parameter count differs from the role schema.
    ParameterCount {
        /// Compiler-defined role whose callable surface was checked.
        role: DiagnosticPlatformServiceRole,
        /// Number of parameters required by the role schema.
        expected: u64,
        /// Number of parameters declared by the callable.
        actual: u64,
    },
    /// One callable parameter has the wrong ABI value kind.
    ParameterType {
        /// Compiler-defined role whose callable surface was checked.
        role: DiagnosticPlatformServiceRole,
        /// Zero-based parameter position in the role schema.
        ordinal: u64,
        /// ABI value kind required at this position.
        expected: DiagnosticPlatformAbiType,
        /// Type declared at this position by the callable.
        actual: DiagnosticType,
    },
    /// Callable result has the wrong ABI value kind.
    ResultType {
        /// Compiler-defined role whose callable surface was checked.
        role: DiagnosticPlatformServiceRole,
        /// ABI value kind required for the result.
        expected: DiagnosticPlatformAbiType,
        /// Result type declared by the callable.
        actual: DiagnosticType,
    },
}

#[cfg(test)]
mod tests {
    use super::DiagnosticPlatformServiceRole;

    #[test]
    fn every_catalog_role_round_trips_through_diagnostics() {
        for role in DiagnosticPlatformServiceRole::ALL {
            assert_eq!(
                DiagnosticPlatformServiceRole::try_new(role.id()),
                Some(*role)
            );
        }
    }

    #[test]
    fn diagnostic_roles_cover_bootstrap_thread_storage() {
        for id in 0x0331..=0x0334 {
            assert_eq!(
                DiagnosticPlatformServiceRole::try_new(id).map(|role| role.id()),
                Some(id)
            );
        }

        assert_eq!(DiagnosticPlatformServiceRole::try_new(0x0330), None);
        assert_eq!(DiagnosticPlatformServiceRole::try_new(0x0335), None);
    }
}
