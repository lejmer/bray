//! Native symbol names derived from the execution and platform role catalogs.

macro_rules! define_runtime_symbols {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($symbol:ident = $native:literal, [$($native_parameter:ident),*] -> $native_result:ident)?),
        $(resident: ($resident_service:ident $resident_field:ident: $resident_callback:ty),)?
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        $( $(#[doc = $documentation] pub const $symbol: &str = $native;)? )+

        const EXECUTION_IDENTITIES: &[(&str, Option<&str>)] = &[
            $(($name, define_runtime_symbols!(@symbol $($native)?)),)+
        ];
    };
    (@symbol $native:literal) => { Some($native) };
    (@symbol) => { None };
}

crate::runtime_role_catalog!(define_runtime_symbols);

macro_rules! define_platform_symbols {
    ($( $role:ident {
        $documentation:literal, $id:literal, $name:literal,
        $symbol:ident = $native:literal,
        $family:ident, [$($parameter:ident),*] -> $result:ident, bootstrap: ($($bootstrap:literal)?)
    })+) => {
        $( #[doc = $documentation] pub const $symbol: &str = $native; )+

        const PLATFORM_IDENTITIES: &[(&str, Option<&str>)] = &[
            $(($name, Some($native)),)+
        ];
    };
}

crate::platform_role_catalog!(define_platform_symbols);

const _: () = crate::catalog::validate_identities(&[EXECUTION_IDENTITIES, PLATFORM_IDENTITIES]);
