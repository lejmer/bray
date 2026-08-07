macro_rules! native_platform_export {
    ($item:item) => {
        #[expect(
            unsafe_code,
            reason = "native platform services require stable exported ABIs and checked raw access"
        )]
        #[unsafe(no_mangle)]
        $item
    };
}
