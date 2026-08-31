#include <cstddef>
#include <cstdint>

#if !defined(BRAY_INNER_ITERATIONS)
#define BRAY_INNER_ITERATIONS 1
#endif

#if defined(BRAY_PEER_TIMING) || BRAY_WORKLOAD == 6
#include <chrono>
#endif

#if defined(BRAY_PEER_TIMING) || BRAY_WORKLOAD == 10
#include <cstdlib>
#endif

#if defined(BRAY_PEER_TIMING)
#include <fstream>
#endif

#if BRAY_WORKLOAD == 2 || BRAY_WORKLOAD == 3 || BRAY_WORKLOAD == 7 || BRAY_WORKLOAD == 8 \
    || BRAY_WORKLOAD == 12 || BRAY_WORKLOAD == 15
#include <vector>
#endif

#if BRAY_WORKLOAD == 16
#include <cstdio>
#include <iostream>
#include <string>
#endif

#if BRAY_WORKLOAD == 17
#include <deque>
#endif

#if BRAY_WORKLOAD == 18 || BRAY_WORKLOAD == 19
#include <unordered_map>
#endif

#if BRAY_WORKLOAD == 20
#include <map>
#include <set>
#endif

#if BRAY_WORKLOAD == 7
#include <charconv>
#include <string>
#include <string_view>
#endif

#if BRAY_WORKLOAD == 8 || BRAY_WORKLOAD == 12 || BRAY_WORKLOAD == 13
#include <charconv>
#endif

#if BRAY_WORKLOAD == 9 || BRAY_WORKLOAD == 10 || BRAY_WORKLOAD == 14
#include <iostream>
#endif

#if BRAY_WORKLOAD == 10
#include <coroutine>
#endif

#if BRAY_WORKLOAD == 14
#include <mutex>
#include <thread>
#endif

#if BRAY_WORKLOAD == 4
#include <filesystem>
#if defined(_WIN32)
#include <windows.h>
#else
#include <sys/stat.h>
#endif
#endif

#if BRAY_WORKLOAD == 11
#include <array>
#if defined(_WIN32)
#include <windows.h>
#else
#include <fcntl.h>
#include <unistd.h>
#endif
#endif

#if defined(_WIN32) && BRAY_WORKLOAD == 5
#include <processthreadsapi.h>
#elif BRAY_WORKLOAD == 5
#include <unistd.h>
#endif

namespace {

constexpr char observation_header[] = "BRAYPO01";
constexpr std::uint8_t controlled_duration_record = 3;

#if defined(BRAY_PEER_TIMING) || (BRAY_WORKLOAD >= 2 && BRAY_WORKLOAD <= 8) \
    || BRAY_WORKLOAD == 12 || (BRAY_WORKLOAD >= 17 && BRAY_WORKLOAD <= 20)
template <typename Value>
void retain_work(Value const& value)
{
    __asm__ volatile("" : : "g"(&value) : "memory");
}
#endif

#if BRAY_WORKLOAD == 7
char const* opaque_pointer(char const* value)
{
    __asm__ volatile("" : "+r"(value) : : "memory");

    return value;
}
#endif

#if BRAY_WORKLOAD == 8 || BRAY_WORKLOAD == 12 || BRAY_WORKLOAD == 13
std::uint32_t opaque_integer(std::uint32_t value)
{
    __asm__ volatile("" : "+r"(value) : : "memory");

    return value;
}
#endif

#if BRAY_WORKLOAD == 1
bool workload()
{
    return true;
}
#elif BRAY_WORKLOAD == 2 || BRAY_WORKLOAD == 3
bool incremental_bytes(std::size_t count)
{
    std::vector<std::uint8_t> bytes;

    for (std::size_t index = 0; index < count; ++index)
        bytes.push_back(65);

    retain_work(bytes);
    return bytes.size() == count;
}

bool workload()
{
#if BRAY_WORKLOAD == 2
    return incremental_bytes(64);
#else
    return incremental_bytes(4096);
#endif
}
#elif BRAY_WORKLOAD == 18
bool workload()
{
    std::unordered_map<std::uint64_t, std::uint64_t> map;

    for (std::uint64_t key = 0; key < 4096; ++key)
        map.emplace(key, key);

    for (std::uint64_t key = 0; key < 4096; ++key)
        if (map.find(key) == map.end())
            return false;

    retain_work(map);

    return map.size() == 4096;
}
#elif BRAY_WORKLOAD == 19
struct CollisionHasher
{
    std::size_t operator()(std::uint64_t) const
    {
        return 0;
    }
};

bool workload()
{
    std::unordered_map<std::uint64_t, std::uint64_t, CollisionHasher> map;

    map.reserve(48);

    for (std::uint64_t key = 0; key < 48; ++key)
        map.emplace(key, key);

    for (std::size_t lookup = 0; lookup < 4096; ++lookup)
        if (map.find(static_cast<std::uint64_t>(lookup % 48)) == map.end())
            return false;

    retain_work(map);

    return map.size() == 48;
}
#elif BRAY_WORKLOAD == 20
bool workload()
{
    std::map<std::uint64_t, std::uint64_t> map;
    std::set<std::uint64_t> set;

    for (std::uint64_t key = 0; key < 4096; ++key)
        map.emplace(key, key);

    for (std::uint64_t key = 4096; key > 0; --key)
        set.emplace(key - 1);

    for (std::uint64_t key = 0; key < 4096; ++key)
        if (map.find(key) == map.end() || set.find(key) == set.end())
            return false;

    retain_work(map);
    retain_work(set);

    return map.size() == 4096 && set.size() == 4096;
}
#elif BRAY_WORKLOAD == 7
struct Wide
{
    std::uint64_t high;
    std::uint64_t low;
};

constexpr Wide modulus{0x7fff'ffff'ffff'ffff, 0xffff'ffff'ffff'ffff};
constexpr Wide initial{0, 14'695'981'039'346'656'037};
constexpr std::uint64_t multiplier = 1'099'511'628'211;

bool less(Wide left, Wide right)
{
    return left.high < right.high || (left.high == right.high && left.low < right.low);
}

bool equal(Wide left, Wide right)
{
    return left.high == right.high && left.low == right.low;
}

Wide subtract(Wide left, Wide right)
{
    return Wide{
        left.high - right.high - static_cast<std::uint64_t>(left.low < right.low),
        left.low - right.low,
    };
}

Wide add(Wide left, Wide right)
{
    const std::uint64_t low = left.low + right.low;

    return Wide{
        left.high + right.high + static_cast<std::uint64_t>(low < left.low),
        low,
    };
}

Wide add_modulo(Wide left, Wide right)
{
    const Wide remaining = subtract(modulus, right);

    if (!less(left, remaining))
        return subtract(left, remaining);

    return add(left, right);
}

Wide multiply_modulo(Wide left, std::uint64_t right)
{
    Wide factor = left;
    Wide product{0, 0};

    while (right > 0)
    {
        if (right % 2 == 1)
            product = add_modulo(product, factor);

        factor = add_modulo(factor, factor);
        right /= 2;
    }

    return product;
}

Wide stable_hash(std::string_view value)
{
    Wide state = multiply_modulo(add_modulo(initial, Wide{0, value.size()}), multiplier);

    for (unsigned char byte : value)
        state = multiply_modulo(add_modulo(state, Wide{0, byte}), multiplier);

    return state;
}

char escape_code(unsigned char value)
{
    switch (value)
    {
    case '"': return '"';
    case '\\': return '\\';
    case '\n': return 'n';
    case '\r': return 'r';
    case '\t': return 't';
    default: return 0;
    }
}

std::size_t escaped_length(std::string_view value)
{
    std::size_t length = 2;

    for (unsigned char byte : value)
        length += escape_code(byte) == 0 ? 1 : 2;

    return length;
}

bool workload()
{
    constexpr char literal_source[] = "borrowed text";
    constexpr char long_source[] =
        "Bray immutable text pipeline repeated across a deliberately long UTF-8 literal for stable throughput coverage.";

    const std::string_view literal{opaque_pointer(literal_source), sizeof(literal_source) - 1};
    const std::string_view duplicate{opaque_pointer(literal_source), sizeof(literal_source) - 1};
    const std::string_view long_text{opaque_pointer(long_source), sizeof(long_source) - 1};

    const auto middle = long_text.substr(1, long_text.size() - 2);
    const std::string owned{opaque_pointer("owned text")};
    std::uint64_t parsed = 0;
    const char* number = opaque_pointer("42");
    const auto parse = std::from_chars(number, number + 2, parsed);
    std::vector<std::uint8_t> sink;

    for (std::size_t index = 0; index < 256; ++index)
        sink.insert(sink.end(), literal.begin(), literal.end());

    const std::size_t quoted_length = escaped_length(long_text);

    sink.push_back('"');

    for (unsigned char byte : long_text)
    {
        const char escaped = escape_code(byte);

        if (escaped != 0)
        {
            sink.push_back('\\');
            sink.push_back(static_cast<std::uint8_t>(escaped));
        }
        else
            sink.push_back(byte);
    }

    sink.push_back('"');
    retain_work(sink);

    return middle.size() == long_text.size() - 2
        && literal == duplicate
        && owned == "owned text"
        && equal(stable_hash(literal), stable_hash(duplicate))
        && !equal(stable_hash(literal), stable_hash(long_text))
        && parse.ec == std::errc{}
        && parse.ptr == number + 2
        && parsed == 42
        && sink.size() == literal.size() * 256 + quoted_length;
}
#elif BRAY_WORKLOAD == 8
bool workload()
{
    std::vector<char> bytes;
    char formatted[10];

    bytes.reserve(2986);

    for (std::uint32_t value = 0; value < 1024; ++value)
    {
        const auto result =
            std::to_chars(formatted, formatted + sizeof(formatted), opaque_integer(value));

        if (result.ec != std::errc{})
            return false;

        bytes.insert(bytes.end(), formatted, result.ptr);
    }

    retain_work(bytes);
    return bytes.size() == 2986;
}
#elif BRAY_WORKLOAD == 12
bool workload()
{
    std::vector<char> bytes;
    char formatted[10];

    bytes.reserve(133120);

    for (std::size_t index = 0; index < 1024; ++index)
    {
        const auto result =
            std::to_chars(formatted, formatted + sizeof(formatted), opaque_integer(42));

        if (result.ec != std::errc{})
            return false;

        bytes.insert(bytes.end(), 128, ' ');
        bytes.insert(bytes.end(), formatted, result.ptr);
    }

    retain_work(bytes);

    if (bytes.size() != 133120)
        return false;

    for (std::size_t index = 0; index < bytes.size(); ++index)
    {
        const std::size_t offset = index % 130;
        const char expected = offset < 128 ? ' ' : offset == 128 ? '4' : '2';

        if (bytes[index] != expected)
            return false;
    }

    return true;
}
#elif BRAY_WORKLOAD == 13
class ValidatingWriter
{
public:
    bool write(char const* bytes, std::size_t count)
    {
        for (std::size_t index = 0; index < count; ++index)
        {
            const char expected = length_ % 2 == 0 ? '4' : '2';

            if (bytes[index] != expected)
                valid_ = false;

            ++length_;
        }

        return true;
    }

    bool valid() const
    {
        return valid_ && length_ == 2048;
    }

private:
    std::size_t length_ = 0;
    bool valid_ = true;
};

template <typename Writer>
bool write_value(Writer& writer, std::uint32_t value)
{
    char formatted[10];
    const auto result = std::to_chars(formatted, formatted + sizeof(formatted), value);

    return result.ec == std::errc{}
        && writer.write(formatted, static_cast<std::size_t>(result.ptr - formatted));
}

bool workload()
{
    ValidatingWriter writer;

    for (std::size_t index = 0; index < 1024; ++index)
        if (!write_value(writer, opaque_integer(42)))
            return false;

    return writer.valid();
}
#elif BRAY_WORKLOAD == 9 || BRAY_WORKLOAD == 10 || BRAY_WORKLOAD == 14
bool write_standard_output()
{
#if BRAY_WORKLOAD == 14
    static std::mutex output_mutex;
    const std::lock_guard lock{output_mutex};
#endif

    std::cout.write("x", 1);
    std::cout.flush();

    return static_cast<bool>(std::cout);
}

#if BRAY_WORKLOAD == 9
bool workload()
{
    for (std::size_t index = 0; index < 1024; ++index)
        if (!write_standard_output())
            return false;

    return true;
}
#elif BRAY_WORKLOAD == 10
class OutputTask
{
public:
    struct promise_type
    {
        bool result = false;

        OutputTask get_return_object()
        {
            return OutputTask{std::coroutine_handle<promise_type>::from_promise(*this)};
        }

        std::suspend_always initial_suspend() const noexcept
        {
            return {};
        }

        std::suspend_always final_suspend() const noexcept
        {
            return {};
        }

        void return_value(bool value) noexcept
        {
            result = value;
        }

        void unhandled_exception() const noexcept
        {
            std::abort();
        }
    };

    explicit OutputTask(std::coroutine_handle<promise_type> handle) : handle_(handle) {}

    OutputTask(OutputTask const&) = delete;
    OutputTask& operator=(OutputTask const&) = delete;

    ~OutputTask()
    {
        handle_.destroy();
    }

    bool run()
    {
        handle_.resume();
        return handle_.done() && handle_.promise().result;
    }

private:
    std::coroutine_handle<promise_type> handle_;
};

struct OutputOperation
{
    bool result = false;

    bool await_ready() const noexcept
    {
        return false;
    }

    bool await_suspend(std::coroutine_handle<>) noexcept
    {
        result = write_standard_output();
        return false;
    }

    bool await_resume() const noexcept
    {
        return result;
    }
};

OutputTask write_output_async()
{
    for (std::size_t index = 0; index < 128; ++index)
        if (!(co_await OutputOperation{}))
            co_return false;

    co_return true;
}

bool workload()
{
    auto task = write_output_async();

    return task.run();
}
#else
bool write_standard_output_repeatedly()
{
    for (std::size_t index = 0; index < 64; ++index)
        if (!write_standard_output())
            return false;

    return true;
}

bool workload()
{
    bool first_result = false;
    bool second_result = false;
    std::thread first{[&first_result]() { first_result = write_standard_output_repeatedly(); }};
    std::thread second{[&second_result]() { second_result = write_standard_output_repeatedly(); }};

    first.join();
    second.join();

    return first_result && second_result;
}
#endif
#elif BRAY_WORKLOAD == 4
bool workload()
{
    std::error_code path_error;
    const auto path = std::filesystem::current_path(path_error);

    if (path_error)
        return false;

    for (std::size_t index = 0; index < 256; ++index)
    {
#if defined(_WIN32)
        WIN32_FILE_ATTRIBUTE_DATA metadata;

        if (GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &metadata) == 0)
            return false;

        retain_work(metadata);
#else
        struct stat metadata;

        if (::stat(path.c_str(), &metadata) != 0)
            return false;

        retain_work(metadata);
#endif
    }

    return true;
}
#elif BRAY_WORKLOAD == 11
bool workload()
{
    constexpr char path[] = "bray-performance-file-output";
    std::array<std::uint8_t, 4096> bytes;
    std::size_t written = 0;
    bool valid = true;

    bytes.fill(120);

#if defined(_WIN32)
    DeleteFileA(path);

    const HANDLE file = CreateFileA(
        path,
        GENERIC_WRITE,
        0,
        nullptr,
        CREATE_NEW,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);

    if (file == INVALID_HANDLE_VALUE)
        return false;

    while (written < bytes.size())
    {
        DWORD count = 0;
        const DWORD remaining = static_cast<DWORD>(bytes.size() - written);

        if (WriteFile(file, bytes.data() + written, remaining, &count, nullptr) == 0 || count == 0)
        {
            valid = false;
            break;
        }

        written += count;
    }

    if (valid && FlushFileBuffers(file) == 0)
        valid = false;

    if (CloseHandle(file) == 0)
        valid = false;

    if (DeleteFileA(path) == 0)
        valid = false;
#else
    ::unlink(path);

    const int file = ::open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);

    if (file < 0)
        return false;

    while (written < bytes.size())
    {
        const auto count = ::write(file, bytes.data() + written, bytes.size() - written);

        if (count <= 0)
        {
            valid = false;
            break;
        }

        written += static_cast<std::size_t>(count);
    }

    if (valid && ::fsync(file) != 0)
        valid = false;

    if (::close(file) != 0)
        valid = false;

    if (::unlink(path) != 0)
        valid = false;
#endif

    return valid && written == bytes.size();
}
#elif BRAY_WORKLOAD == 5
bool workload()
{
    std::uint64_t combined = 0;

    for (std::size_t index = 0; index < 1024; ++index)
    {
#if defined(_WIN32)
        combined ^= static_cast<std::uint64_t>(GetCurrentProcessId());
#else
        combined ^= static_cast<std::uint64_t>(getpid());
#endif
    }

    retain_work(combined);
    return true;
}
#elif BRAY_WORKLOAD == 6
bool workload()
{
    auto last = std::chrono::steady_clock::now();

    for (std::size_t index = 0; index < 1024; ++index)
        last = std::chrono::steady_clock::now();

    retain_work(last);
    return true;
}
#elif BRAY_WORKLOAD == 15
bool workload()
{
    std::vector<std::uint8_t> output;
    output.reserve(4096);

    std::uint8_t bytes[4096] = {};
    output.insert(output.end(), bytes, bytes + 4096);

    return output.size() == 4096;
}
#elif BRAY_WORKLOAD == 17
bool workload()
{
    std::deque<std::uint64_t> values;

    for (std::uint64_t value = 0; value < 64; ++value)
        values.push_back(value);

    for (std::uint64_t expected = 0; expected < 32; ++expected)
    {
        if (values.empty() || values.front() != expected)
            return false;

        values.pop_front();
    }

    for (std::uint64_t value = 64; value < 96; ++value)
        values.push_back(value);

    for (std::size_t round = 0; round < 2048; ++round)
    {
        if (round % 2 == 0)
        {
            const std::uint64_t value = values.front();
            values.pop_front();
            values.push_back(value);
        }
        else
        {
            const std::uint64_t value = values.back();
            values.pop_back();
            values.push_front(value);
        }
    }

    values.push_back(96);
    retain_work(values);

    return values.size() == 65 && values.front() == 32 && values.back() == 96;
}
#elif BRAY_WORKLOAD == 16
char const* executable_path = nullptr;

bool workload()
{
    std::string command = "\"";
    command += executable_path;
    command += "\" pipe-child";

#if defined(_WIN32)
    std::FILE* child = ::_popen(command.c_str(), "wb");
#else
    std::FILE* child = ::popen(command.c_str(), "w");
#endif

    if (child == nullptr)
        return false;

    std::uint8_t bytes[4096] = {};
    const std::size_t written = std::fwrite(bytes, 1, sizeof(bytes), child);

#if defined(_WIN32)
    const int status = ::_pclose(child);
#else
    const int status = ::pclose(child);
#endif

    return written == sizeof(bytes) && status == 0;
}
#else
#error "unsupported performance peer workload"
#endif

#if defined(BRAY_PEER_TIMING)
void write_duration(std::chrono::nanoseconds duration)
{
    const char* path = std::getenv("BRAY_PERFORMANCE_OBSERVATION_PATH");

    if (path == nullptr)
        std::abort();

    std::ofstream output(path, std::ios::binary | std::ios::trunc);

    if (!output)
        std::abort();

    output.write(observation_header, sizeof(observation_header) - 1);
    output.put(static_cast<char>(controlled_duration_record));

    const std::uint64_t nanoseconds = static_cast<std::uint64_t>(duration.count());

    for (std::size_t byte = 0; byte < sizeof(nanoseconds); ++byte)
        output.put(static_cast<char>((nanoseconds >> (byte * 8)) & 0xff));

    if (!output)
        std::abort();
}
#endif

}

int main(int argc, char** argv)
{
#if BRAY_WORKLOAD == 16
    if (argc > 1)
    {
        std::uint8_t bytes[4096] = {};
        std::cin.read(reinterpret_cast<char*>(bytes), sizeof(bytes));

        return std::cin.gcount() == sizeof(bytes) ? 0 : 1;
    }

    executable_path = argv[0];
#endif

#if defined(BRAY_PEER_TIMING)
    const auto started = std::chrono::steady_clock::now();
#endif

#if defined(BRAY_PEER_TIMING)
    bool valid = true;
    for (std::uint64_t iteration = 0; iteration < BRAY_INNER_ITERATIONS; ++iteration)
    {
        valid = workload() && valid;
        retain_work(valid);
    }
#else
    const bool valid = workload();
#endif

#if defined(BRAY_PEER_TIMING)
    write_duration(std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::steady_clock::now() - started));
#endif

    return valid ? 0 : 1;
}
