macro_rules! cpp_type {
    (Path) => {
        "BrayPlatformPath"
    };
    (U32) => {
        "std::uint32_t"
    };
    (U64) => {
        "std::uint64_t"
    };
    (PointerU8) => {
        "const std::uint8_t*"
    };
    (PointerU64) => {
        "std::uint64_t*"
    };
    (RawAddressPointer) => {
        "std::uint8_t**"
    };
    (Status) => {
        "BrayPlatformStatus"
    };
}

macro_rules! declarations {
    ($( $role:ident {
        $documentation:literal, $id:literal, $name:literal,
        $symbol:ident = $native:literal,
        $family:ident, [$($parameter:ident),*] -> $result:ident, bootstrap: ($($bootstrap:literal)?)
    })+) => {
        pub(crate) fn declarations() -> String {
            let mut source = String::new();

            $(declarations!(@role source, $family, $native, [$($parameter),*], $result);)+

            source
        }
    };
    (@role $source:ident, DynamicLibrary, $native:literal, [$($parameter:ident),*], $result:ident) => {
        $source.push_str(&format!("{} {}({});\n", cpp_type!($result), $native, [$(cpp_type!($parameter),)*].join(", ")));
    };
    (@role $source:ident, $family:ident, $native:literal, [$($parameter:ident),*], $result:ident) => {};
}

bray_runtime_abi::platform_role_catalog!(declarations);
