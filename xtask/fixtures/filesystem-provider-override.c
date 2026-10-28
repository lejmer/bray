#include <stdint.h>
#include <string.h>

typedef struct { const uint8_t* address; uint64_t length; uint64_t terminated; } Path;
typedef struct { uint32_t access; uint32_t creation; uint64_t reserved; } Options;
typedef struct { uint32_t category; uint32_t reserved; int64_t native_code; } Status;
typedef struct {
    uint32_t kind;
    uint32_t present;
    uint64_t bytes;
    int64_t modified_seconds;
    uint32_t modified_nanoseconds;
    uint32_t reserved;
} Metadata;

_Static_assert(sizeof(Path) == 24, "path ABI layout");
_Static_assert(sizeof(Options) == 16, "file options ABI layout");
_Static_assert(sizeof(Metadata) == 32, "metadata ABI layout");
_Static_assert(sizeof(Status) == 16, "status ABI layout");

static unsigned file_close_attempts;
static unsigned directory_close_attempts;
static unsigned directory_phase;
static unsigned file_abandon_attempts;
static unsigned file_terminal_calls;
static unsigned directory_abandon_attempts;
static unsigned directory_terminal_calls;
static Status success(void) { return (Status){0, 0, 0}; }
static Status invalid(void) { return (Status){5, 0, 0}; }
static int terminated_path(Path path)
{
    if (!path.address || !path.length || path.terminated != 1 || path.address[path.length] != 0) return 0;
#ifdef _WIN32
    if (path.address[path.length + 1] != 0 || (uintptr_t)path.address % 2 != 0) return 0;
#endif
    return 1;
}

Status bray_platform_file_open_v2(Path path, Options options, uint64_t* opened)
{
    (void)options;
    if (!terminated_path(path) || !opened) return invalid();
    if (path.address[0] == 'b') { *opened = 0; return (Status){0, 1, 0}; }
    if (path.address[0] == 'z') { file_abandon_attempts = 0; file_terminal_calls = 0; }
    *opened = path.address[0] == 'c' ? 0x2222 : path.address[0] == 'z' ? 0x7777 : 0x1111;
    file_close_attempts = 0;
    return success();
}

Status bray_platform_file_close_v3(uint64_t handle, uint32_t disposition, uint32_t* released)
{
    if (!released) return invalid();
    if (disposition > 1) { *released = 0; return invalid(); }
    *released = 0;
    if (handle == 0x7777) {
        if (disposition == 0) { file_abandon_attempts++; *released = 0; return (Status){6, 0, 0}; }
        file_terminal_calls++; *released = 1; return (Status){6, 0, 0};
    }
    if (handle == 0x2222) { *released = 1; return (Status){6, 0, 0}; }
    if (handle != 0x1111) return invalid();
    if (disposition == 1) { *released = 1; return success(); }
    if (file_close_attempts++ == 0) { *released = 0; return (Status){6, 0, 0}; }
    *released = 1;
    return success();
}

Status bray_platform_file_metadata(uint64_t handle, Metadata* output)
{
    if (handle != 0x1111 || !output) return invalid();
    *output = (Metadata){0, 1, 123, -5, 9, 0};
    return success();
}

Status bray_platform_path_metadata_v2(Path path, Metadata* output)
{
    if (!terminated_path(path) || !output) return invalid();
    if (path.address[0] == 'f') {
        if (!file_abandon_attempts || file_terminal_calls != 1) return invalid();
        *output = (Metadata){0, 0, file_abandon_attempts, 0, 0, 0}; return success();
    }
    if (path.address[0] == 'g') {
        if (!directory_abandon_attempts || directory_terminal_calls != 1) return invalid();
        *output = (Metadata){0, 0, directory_abandon_attempts, 0, 0, 0}; return success();
    }
    if (path.address[0] != 'p') return invalid();
    *output = (Metadata){1, 0, 77, 0, 0, 0};
    return success();
}

Status bray_platform_file_read(uint64_t handle, uint8_t* destination, uint64_t length, uint64_t* transferred)
{
    if (handle != 0x1111 || !transferred || (length && !destination)) return invalid();
    *transferred = length ? 1 : 0;
    if (length) destination[0] = 81;
    return success();
}

Status bray_platform_file_write(uint64_t handle, const uint8_t* source, uint64_t length, uint64_t* transferred)
{
    if (handle != 0x1111 || !transferred || (length && !source)) return invalid();
    *transferred = length;
    return success();
}

Status bray_platform_file_flush(uint64_t handle) { return handle == 0x1111 ? success() : invalid(); }
Status bray_platform_file_seek(uint64_t handle, uint64_t offset, uint32_t origin, uint64_t* position)
{
    (void)origin;
    if (handle != 0x1111 || !position) return invalid();
    *position = offset;
    return success();
}

Status bray_platform_directory_open_v2(Path path, uint64_t* opened)
{
    if (!terminated_path(path) || !opened || (path.address[0] != 'v' && path.address[0] != 'd' && path.address[0] != 'm' && path.address[0] != 'e')) return invalid();
    *opened = path.address[0] == 'd' ? 0x4444 : path.address[0] == 'm' ? 0x5555 : path.address[0] == 'e' ? 0x6666 : 0x3333;
    directory_phase = 0;
    directory_close_attempts = 0;
    return success();
}

Status bray_platform_directory_next_v2(
    uint64_t handle, uint8_t* destination, uint64_t capacity, uint64_t* length,
    uint32_t* present, Metadata* output)
{
    if (!length || !present || !output) return invalid();
    if (handle == 0x5555) { *length = 0; *present = 3; return success(); }
    if (handle != 0x3333) return invalid();
    if (directory_phase) { *length = 0; *present = 0; return success(); }
#ifdef _WIN32
    const uint64_t needed = 2200;
#else
    const uint64_t needed = 1100;
#endif
    *length = needed;
    if (capacity < needed) { *present = 2; return success(); }
    if (!destination) return invalid();
#ifdef _WIN32
    for (uint64_t index = 0; index < needed; index += 2) { destination[index] = 'x'; destination[index + 1] = 0; }
#else
    memset(destination, 'x', (size_t)needed);
#endif
    *output = (Metadata){0, 0, 99, 0, 0, 0};
    *present = 1;
    directory_phase = 1;
    return success();
}

Status bray_platform_directory_close_v3(uint64_t handle, uint32_t disposition, uint32_t* released)
{
    if (!released) return invalid();
    if (disposition > 1) { *released = 0; return invalid(); }
    *released = 0;
    if (handle == 0x6666) {
        if (disposition == 0) { directory_abandon_attempts++; *released = 0; return (Status){6, 0, 0}; }
        directory_terminal_calls++; *released = 1; return (Status){6, 0, 0};
    }
    if (handle == 0x4444) { *released = 1; return (Status){6, 0, 0}; }
    if (handle == 0x5555) { *released = 1; return success(); }
    if (handle != 0x3333) return invalid();
    if (disposition == 1) { *released = 1; return success(); }
    if (directory_close_attempts++ == 0) { *released = 0; return (Status){6, 0, 0}; }
    *released = 1;
    return success();
}
