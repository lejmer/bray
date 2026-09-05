/// Declares one checked native platform ABI export.
///
/// Native signatures must match the role catalog, including exports expanded by another macro.
///
/// ```compile_fail
/// use bray_platform_abi_support::native_platform_export;
/// native_platform_export! {
///     pub extern "C" fn bray_platform_standard_output_flush() -> u32 { 0 }
/// }
/// ```
#[macro_export]
macro_rules! native_platform_export {
    ($(#[$attribute:meta])* $visibility:vis extern "C" fn $name:ident(
        $($parameter:ident: $ty:ty),* $(,)?
    ) -> $result:ty $body:block) => {
        const _: () = assert!(
            $crate::platform_signature_matches(
                stringify!($name),
                &[$(<$ty as $crate::PlatformAbiValue>::ABI_TYPE,)*],
                <$result as $crate::PlatformAbiValue>::ABI_TYPE,
            ),
            concat!("native platform export ", stringify!($name), " does not match its catalog signature"),
        );

        $(#[$attribute])*
        #[expect(
            unsafe_code,
            reason = "native platform services require stable exported ABIs and checked raw access"
        )]
        #[unsafe(no_mangle)]
        $visibility extern "C" fn $name($($parameter: $ty),*) -> $result $body
    };
}
