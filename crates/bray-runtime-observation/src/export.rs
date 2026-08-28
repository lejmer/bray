macro_rules! native_export {
    ($item:item) => {
        #[expect(
            unsafe_code,
            reason = "the native runtime artifact requires a stable exported ABI symbol"
        )]
        #[unsafe(no_mangle)]
        $item
    };
}
