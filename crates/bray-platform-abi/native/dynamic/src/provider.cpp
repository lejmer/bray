#include "bray_dynamic.h"

#include <atomic>
#include <cerrno>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <mutex>
#include <new>
#include <string>
#include <unordered_map>

#ifdef _WIN32
#define NOMINMAX
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace
{

constexpr std::uint32_t success = 0;
constexpr std::uint32_t unsupported = 1;
constexpr std::uint32_t permission_denied = 2;
constexpr std::uint32_t not_found = 3;
constexpr std::uint32_t invalid_input = 5;
constexpr std::uint32_t interrupted = 6;
constexpr std::uint32_t exhausted = 7;
constexpr std::uint32_t other = 12;

constexpr std::uint32_t platform_default = 0;
constexpr std::uint32_t local_immediate = 1;
constexpr std::uint32_t local_lazy = 2;
constexpr std::uint32_t global_immediate = 3;
constexpr std::uint32_t global_lazy = 4;
constexpr std::uint32_t c_runtime = 0;

#ifdef _WIN32
using NativeLibrary = HMODULE;
#else
using NativeLibrary = void*;
#endif

std::mutex libraries_mutex;
std::unordered_map<std::uint64_t, NativeLibrary> libraries;
std::atomic<std::uint64_t> next_handle{1};

BrayPlatformStatus status(std::uint32_t category, std::int64_t native_code = 0)
{
    return {category, 0, native_code};
}

#ifdef _WIN32
BrayPlatformStatus native_failure(DWORD code)
{
    switch (code)
    {
    case ERROR_ACCESS_DENIED:
    case ERROR_PRIVILEGE_NOT_HELD:
        return status(permission_denied, code);
    case ERROR_FILE_NOT_FOUND:
    case ERROR_MOD_NOT_FOUND:
    case ERROR_PATH_NOT_FOUND:
    case ERROR_PROC_NOT_FOUND:
        return status(not_found, code);
    case ERROR_INVALID_PARAMETER:
        return status(invalid_input, code);
    case ERROR_NOT_ENOUGH_MEMORY:
    case ERROR_OUTOFMEMORY:
    case ERROR_TOO_MANY_OPEN_FILES:
        return status(exhausted, code);
    default:
        return status(other, code);
    }
}
#else
BrayPlatformStatus native_failure(int code)
{
    switch (code)
    {
    case EACCES:
    case EPERM:
        return status(permission_denied, code);
    case ENOENT:
    case ENOTDIR:
        return status(not_found, code);
    case EINVAL:
        return status(invalid_input, code);
    case EINTR:
        return status(interrupted, code);
    case EMFILE:
    case ENFILE:
    case ENOMEM:
        return status(exhausted, code);
    default:
        return status(other, code);
    }
}
#endif

bool aligned(const void* pointer, std::size_t alignment)
{
    return reinterpret_cast<std::uintptr_t>(pointer) % alignment == 0;
}

bool ranges_overlap(
    const void* left,
    std::uint64_t left_length,
    const void* right,
    std::uint64_t right_length
)
{
    const auto left_start = reinterpret_cast<std::uintptr_t>(left);
    const auto right_start = reinterpret_cast<std::uintptr_t>(right);

    if (
        left_length > std::numeric_limits<std::uintptr_t>::max() - left_start ||
        right_length > std::numeric_limits<std::uintptr_t>::max() - right_start
    )
        return true;

    const auto left_end = left_start + static_cast<std::uintptr_t>(left_length);
    const auto right_end = right_start + static_cast<std::uintptr_t>(right_length);

    return left_start < right_end && right_start < left_end;
}

BrayPlatformStatus insert_library(NativeLibrary library, std::uint64_t* opened)
{
    std::lock_guard lock{libraries_mutex};

    for (std::uint32_t attempt = 0; attempt != 128; ++attempt)
    {
        const auto handle = next_handle.fetch_add(1, std::memory_order_relaxed);

        if (handle == 0 || libraries.find(handle) != libraries.end())
            continue;

        libraries.emplace(handle, library);
        *opened = handle;

        return status(success);
    }

    return status(exhausted);
}

#ifdef _WIN32
bool absolute_path(const std::wstring& path)
{
    if (
        path.size() >= 3 &&
        (
            (path[0] >= L'A' && path[0] <= L'Z') ||
            (path[0] >= L'a' && path[0] <= L'z')
        ) &&
        path[1] == L':' &&
        (path[2] == L'\\' || path[2] == L'/')
    )
        return true;

    return path.size() >= 2 && path[0] == L'\\' && path[1] == L'\\';
}

BrayPlatformStatus load_path(BrayPlatformPath path, std::uint32_t policy, NativeLibrary* library)
{
    if (
        path.address == nullptr ||
        path.length == 0 ||
        path.length % sizeof(wchar_t) != 0
    )
        return status(invalid_input);

    const auto length = path.length / sizeof(wchar_t);
    const auto* units = reinterpret_cast<const wchar_t*>(path.address);
    std::wstring native_path{units, units + length};

    if (
        native_path.find(L'\0') != std::wstring::npos ||
        !absolute_path(native_path)
    )
        return status(invalid_input);

    if (policy != platform_default && policy != local_immediate)
        return policy <= global_lazy ? status(unsupported) : status(invalid_input);

    SetLastError(ERROR_SUCCESS);
    *library = LoadLibraryExW(
        native_path.c_str(),
        nullptr,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32
    );

    return *library == nullptr ? native_failure(GetLastError()) : status(success);
}

BrayPlatformStatus load_system(std::uint32_t policy, NativeLibrary* library)
{
    if (policy != platform_default && policy != local_immediate)
        return policy <= global_lazy ? status(unsupported) : status(invalid_input);

    SetLastError(ERROR_SUCCESS);
    *library = LoadLibraryExW(L"ucrtbase.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);

    return *library == nullptr ? native_failure(GetLastError()) : status(success);
}

BrayPlatformStatus close_library(NativeLibrary library)
{
    SetLastError(ERROR_SUCCESS);

    return FreeLibrary(library) == 0 ? native_failure(GetLastError()) : status(success);
}

BrayPlatformStatus find_symbol(NativeLibrary library, const std::string& name, std::uint8_t** address)
{
    SetLastError(ERROR_SUCCESS);
    const auto symbol = GetProcAddress(library, name.c_str());

    if (symbol == nullptr)
        return native_failure(GetLastError());

    *address = reinterpret_cast<std::uint8_t*>(symbol);

    return status(success);
}
#else
bool absolute_path(const std::string& path)
{
    return !path.empty() && path.front() == '/';
}

BrayPlatformStatus loader_policy(std::uint32_t policy, int* flags)
{
    switch (policy)
    {
    case platform_default:
    case local_immediate:
        *flags = RTLD_LOCAL | RTLD_NOW;
        return status(success);
    case local_lazy:
        *flags = RTLD_LOCAL | RTLD_LAZY;
        return status(success);
    case global_immediate:
        *flags = RTLD_GLOBAL | RTLD_NOW;
        return status(success);
    case global_lazy:
        *flags = RTLD_GLOBAL | RTLD_LAZY;
        return status(success);
    default:
        return status(invalid_input);
    }
}

BrayPlatformStatus load_named(
    const char* name,
    std::uint32_t policy,
    NativeLibrary* library
)
{
    int flags = 0;
    const auto policy_status = loader_policy(policy, &flags);

    if (policy_status.category != success)
        return policy_status;

    errno = 0;
    dlerror();
    *library = dlopen(name, flags);

    if (*library != nullptr)
        return status(success);

    const auto code = errno;
    return code == 0 ? status(not_found) : native_failure(code);
}

BrayPlatformStatus load_path(BrayPlatformPath path, std::uint32_t policy, NativeLibrary* library)
{
    if (
        path.address == nullptr ||
        path.length == 0 ||
        path.length > std::numeric_limits<std::size_t>::max()
    )
        return status(invalid_input);

    std::string native_path{
        reinterpret_cast<const char*>(path.address),
        static_cast<std::size_t>(path.length)
    };

    if (
        native_path.find('\0') != std::string::npos ||
        !absolute_path(native_path)
    )
        return status(invalid_input);

    return load_named(native_path.c_str(), policy, library);
}

BrayPlatformStatus load_system(std::uint32_t policy, NativeLibrary* library)
{
#ifdef __APPLE__
    return load_named("/usr/lib/libSystem.B.dylib", policy, library);
#else
    return load_named("libc.so.6", policy, library);
#endif
}

BrayPlatformStatus close_library(NativeLibrary library)
{
    errno = 0;
    dlerror();

    if (dlclose(library) == 0)
        return status(success);

    const auto code = errno;
    return code == 0 ? status(other) : native_failure(code);
}

BrayPlatformStatus find_symbol(NativeLibrary library, const std::string& name, std::uint8_t** address)
{
    errno = 0;
    dlerror();
    const auto symbol = dlsym(library, name.c_str());
    const auto* error = dlerror();

    if (error != nullptr)
    {
        const auto code = errno;
        return code == 0 ? status(not_found) : native_failure(code);
    }

    if (symbol == nullptr)
        return status(other);

    *address = static_cast<std::uint8_t*>(symbol);

    return status(success);
}
#endif

void discard_library(NativeLibrary library)
{
    static_cast<void>(close_library(library));
}

}

extern "C" BrayPlatformStatus bray_platform_dynamic_library_open_path(
    BrayPlatformPath path,
    std::uint32_t policy,
    std::uint64_t* opened
)
{
    if (
        opened == nullptr ||
        !aligned(opened, alignof(std::uint64_t)) ||
        (
            path.address != nullptr &&
            ranges_overlap(path.address, path.length, opened, sizeof(*opened))
        )
    )
        return status(invalid_input);

    *opened = 0;

    try
    {
        NativeLibrary library = nullptr;
        const auto loaded = load_path(path, policy, &library);

        if (loaded.category != success)
            return loaded;

        const auto inserted = insert_library(library, opened);

        if (inserted.category != success)
            discard_library(library);

        return inserted;
    }
    catch (const std::bad_alloc&)
    {
        return status(exhausted);
    }
    catch (...)
    {
        return status(other);
    }
}

extern "C" BrayPlatformStatus bray_platform_dynamic_library_open_system(
    std::uint32_t identity,
    std::uint32_t policy,
    std::uint64_t* opened
)
{
    if (opened == nullptr || !aligned(opened, alignof(std::uint64_t)))
        return status(invalid_input);

    *opened = 0;

    if (identity != c_runtime)
        return status(invalid_input);

    try
    {
        NativeLibrary library = nullptr;
        const auto loaded = load_system(policy, &library);

        if (loaded.category != success)
            return loaded;

        const auto inserted = insert_library(library, opened);

        if (inserted.category != success)
            discard_library(library);

        return inserted;
    }
    catch (const std::bad_alloc&)
    {
        return status(exhausted);
    }
    catch (...)
    {
        return status(other);
    }
}

extern "C" BrayPlatformStatus bray_platform_dynamic_library_symbol(
    std::uint64_t handle,
    const std::uint8_t* name,
    std::uint64_t length,
    std::uint8_t** address
)
{
    if (
        name == nullptr ||
        length == 0 ||
        length > std::numeric_limits<std::size_t>::max() ||
        address == nullptr ||
        !aligned(address, alignof(void*)) ||
        ranges_overlap(name, length, address, sizeof(*address))
    )
        return status(invalid_input);

    *address = nullptr;

    try
    {
        std::string symbol_name{reinterpret_cast<const char*>(name), static_cast<std::size_t>(length)};

        if (symbol_name.find('\0') != std::string::npos)
            return status(invalid_input);

        std::lock_guard lock{libraries_mutex};
        const auto library = libraries.find(handle);

        if (library == libraries.end())
            return status(invalid_input);

        return find_symbol(library->second, symbol_name, address);
    }
    catch (const std::bad_alloc&)
    {
        return status(exhausted);
    }
    catch (...)
    {
        return status(other);
    }
}

extern "C" BrayPlatformStatus bray_platform_dynamic_library_close(std::uint64_t handle)
{
    try
    {
        NativeLibrary library = nullptr;

        {
            std::lock_guard lock{libraries_mutex};
            const auto found = libraries.find(handle);

            if (found == libraries.end())
                return status(invalid_input);

            library = found->second;
            libraries.erase(found);
        }

        return close_library(library);
    }
    catch (...)
    {
        return status(other);
    }
}
