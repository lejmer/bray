macro_rules! native_adapter {
    ($item:item) => {
        #[expect(
            unsafe_code,
            reason = "the runtime adapter publishes the stable native ABI symbol"
        )]
        #[unsafe(no_mangle)]
        $item
    };
}
