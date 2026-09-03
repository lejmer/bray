/// One ABI value kind used by the closed platform-service callable schema.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformAbiType {
    /// Fixed-width signed 32-bit scalar.
    I32,
    /// Fixed-width unsigned 32-bit scalar.
    U32,
    /// Fixed-width unsigned 64-bit scalar.
    U64,
    /// Fixed-width signed 64-bit scalar.
    I64,
    /// Raw pointer to byte storage.
    PointerU8,
    /// Raw pointer to unsigned 32-bit storage.
    PointerU32,
    /// Raw pointer to unsigned 64-bit storage.
    PointerU64,
    /// Raw pointer to signed 64-bit storage.
    PointerI64,
    /// Raw pointer to one target-native raw address output.
    RawAddressPointer,
    /// Call-only target-native path bytes.
    Path,
    /// Call-only target-native text units.
    NativeText,
    /// The fixed-layout file-open options record.
    FileOptions,
    /// Raw pointer to a fixed-layout file metadata record.
    FileMetadataPointer,
    /// The fixed-layout child-process construction record.
    ChildRequest,
    /// Raw pointer to a fixed-layout child exit-status record.
    ExitStatusPointer,
    /// The fixed-layout civil date-time record.
    TemporalDateTime,
    /// Raw pointer to a civil date-time record.
    TemporalDateTimePointer,
    /// Raw pointer to a timezone observation record.
    TemporalObservationPointer,
    /// Raw pointer to a local-time resolution record.
    TemporalResolutionPointer,
    /// The fixed-layout parsing and formatting value record.
    TemporalValue,
    /// Raw pointer to a parsing and formatting value record.
    TemporalValuePointer,
    /// The fixed-layout platform status record.
    Status,
}

mod sealed {
    pub trait Sealed {}
}

/// Native Rust value representation admitted by the platform callable catalog.
///
/// Implementations are sealed so export declarations cannot claim an unrelated ABI kind.
pub trait PlatformAbiValue: sealed::Sealed {
    /// The exact native ABI kind represented by this Rust type.
    const ABI_TYPE: PlatformAbiType;
}

macro_rules! values {
    ($($ty:ty => $kind:ident),+ $(,)?) => {
        $(
            impl sealed::Sealed for $ty {}
            impl PlatformAbiValue for $ty {
                const ABI_TYPE: PlatformAbiType = PlatformAbiType::$kind;
            }
        )+
    };
}

values! {
    i32 => I32,
    u32 => U32,
    u64 => U64,
    i64 => I64,
    crate::NativePlatformPath => Path,
    crate::NativePlatformText => NativeText,
    crate::NativePlatformFileOptions => FileOptions,
    crate::NativePlatformChildRequest => ChildRequest,
    crate::NativePlatformDateTime => TemporalDateTime,
    crate::NativePlatformTemporalValue => TemporalValue,
    crate::NativePlatformStatus => Status,
}

macro_rules! pointers {
    ($($ty:ty => $kind:ident),+ $(,)?) => {
        $(
            values! { *const $ty => $kind, *mut $ty => $kind }
        )+
    };
}

pointers! {
    u8 => PointerU8,
    u32 => PointerU32,
    u64 => PointerU64,
    i64 => PointerI64,
    usize => RawAddressPointer,
    crate::NativePlatformFileMetadata => FileMetadataPointer,
    crate::NativePlatformExitStatus => ExitStatusPointer,
    crate::NativePlatformDateTime => TemporalDateTimePointer,
    crate::NativePlatformTemporalObservation => TemporalObservationPointer,
    crate::NativePlatformTemporalResolution => TemporalResolutionPointer,
    crate::NativePlatformTemporalValue => TemporalValuePointer,
}

const fn same_signature(
    parameters: &[PlatformAbiType],
    result: PlatformAbiType,
    expected_parameters: &[PlatformAbiType],
    expected_result: PlatformAbiType,
) -> bool {
    if parameters.len() != expected_parameters.len() || result as u32 != expected_result as u32 {
        return false;
    }

    let mut index = 0;

    while index < parameters.len() {
        if parameters[index] as u32 != expected_parameters[index] as u32 {
            return false;
        }

        index += 1;
    }

    true
}

macro_rules! define_platform_signature_check {
    ($( $role:ident {
        $documentation:literal, $id:literal, $name:literal,
        $symbol:ident = $native:literal,
        $family:ident, [$($parameter:ident),*] -> $result:ident, bootstrap: ($($bootstrap:literal)?)
    })+) => {
        /// Checks a native platform export against its complete catalog signature.
        pub const fn platform_signature_matches(
            symbol: &str,
            parameters: &[PlatformAbiType],
            result: PlatformAbiType,
        ) -> bool {
            $(
                if crate::catalog::equal(symbol, $native) {
                    return same_signature(parameters, result, &[$(PlatformAbiType::$parameter,)*], PlatformAbiType::$result);
                }
            )+

            false
        }
    };
}

crate::platform_role_catalog!(define_platform_signature_check);

#[cfg(test)]
mod tests {
    use super::{PlatformAbiType as Type, PlatformAbiValue, platform_signature_matches};

    #[test]
    fn native_platform_contract_rejects_changed_parameters_results_and_unknown_exports() {
        let symbol = crate::symbols::PLATFORM_STANDARD_OUTPUT_WRITE_SYMBOL;
        let parameters = [Type::PointerU8, Type::U64, Type::PointerU64];

        assert!(platform_signature_matches(
            symbol,
            &parameters,
            Type::Status
        ));

        assert!(!platform_signature_matches(
            symbol,
            &parameters[..2],
            Type::Status
        ));

        assert!(!platform_signature_matches(
            symbol,
            &[Type::PointerU8, Type::U32, Type::PointerU64],
            Type::Status
        ));

        assert!(!platform_signature_matches(symbol, &parameters, Type::U32));

        assert!(!platform_signature_matches(
            "bray_platform_unknown",
            &parameters,
            Type::Status
        ));
    }

    #[test]
    fn pointer_authority_preserves_the_native_pointer_kind() {
        assert_eq!(
            <*const u8 as PlatformAbiValue>::ABI_TYPE,
            <*mut u8 as PlatformAbiValue>::ABI_TYPE
        );

        assert_ne!(
            <*mut u8 as PlatformAbiValue>::ABI_TYPE,
            <*mut u64 as PlatformAbiValue>::ABI_TYPE
        );
    }
}
