BrayPlatformStatus bray_platform_dynamic_library_open_path(
    BrayPlatformPath,
    std::uint32_t,
    std::uint64_t*
);
BrayPlatformStatus bray_platform_dynamic_library_symbol(
    std::uint64_t,
    const std::uint8_t*,
    std::uint64_t,
    std::uint8_t**
);
BrayPlatformStatus bray_platform_dynamic_library_close(std::uint64_t);
