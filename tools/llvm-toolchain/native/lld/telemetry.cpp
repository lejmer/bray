#include "telemetry.h"

#include "llvm/ADT/DenseMap.h"
#include "llvm/ADT/StringRef.h"
#include "llvm/IR/GlobalValue.h"
#include "llvm/IR/Module.h"
#include "llvm/Support/Errc.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/Path.h"
#include "llvm/Support/Process.h"
#include "llvm/Support/raw_ostream.h"

#include <algorithm>
#include <atomic>
#include <cstdint>
#include <mutex>
#include <optional>
#include <string>
#include <system_error>
#include <utility>

#if defined(_WIN32)
#include <windows.h>
#include <psapi.h>
#else
#include <sys/resource.h>
#endif

namespace
{

constexpr llvm::StringLiteral report_environment = "BRAY_LLD_OPTIMIZATION_REPORT";
constexpr llvm::StringLiteral cache_magic = "BRAYLTOCACHE";
constexpr std::uint32_t format = 1;

#ifndef BRAY_LLD_TOOLCHAIN_IDENTITY
#error BRAY_LLD_TOOLCHAIN_IDENTITY must identify the pinned linker build
#endif

constexpr llvm::StringLiteral toolchain_identity = BRAY_LLD_TOOLCHAIN_IDENTITY;

enum class DefinitionKind : std::uint8_t
{
    Function,
    Data,
};

struct DefinitionState
{
    DefinitionKind kind;
    std::uint64_t bytes;
};

struct ModuleState
{
    llvm::DenseMap<llvm::GlobalValue::GUID, DefinitionState> internalized;
    llvm::DenseMap<llvm::GlobalValue::GUID, DefinitionState> imported;
    std::uint64_t imported_functions = 0;
    std::uint64_t imported_data = 0;
    std::uint64_t eliminated_functions = 0;
    std::uint64_t eliminated_data = 0;
    std::uint64_t eliminated_bytes = 0;
};

struct CachedState
{
    std::uint64_t imported_functions;
    std::uint64_t imported_data;
    std::uint64_t eliminated_functions;
    std::uint64_t eliminated_data;
    std::uint64_t eliminated_bytes;
};

struct Totals
{
    std::uint64_t imported_functions = 0;
    std::uint64_t imported_data = 0;
    std::uint64_t eliminated_functions = 0;
    std::uint64_t eliminated_data = 0;
    std::uint64_t eliminated_bytes = 0;
    std::uint64_t cache_hits = 0;
    std::uint64_t cache_misses = 0;
    std::uint64_t cache_writes = 0;
};

void append_u32(llvm::SmallVectorImpl<char>& destination, std::uint32_t value)
{
    for (unsigned shift = 0; shift < 32; shift += 8)
        destination.push_back(static_cast<char>((value >> shift) & 0xff));
}

void append_u64(llvm::SmallVectorImpl<char>& destination, std::uint64_t value)
{
    for (unsigned shift = 0; shift < 64; shift += 8)
        destination.push_back(static_cast<char>((value >> shift) & 0xff));
}

std::optional<std::uint32_t> read_u32(llvm::StringRef bytes, std::size_t& offset)
{
    if (offset > bytes.size() || bytes.size() - offset < sizeof(std::uint32_t))
        return std::nullopt;

    std::uint32_t value = 0;

    for (unsigned shift = 0; shift < 32; shift += 8)
        value |= static_cast<std::uint32_t>(
            static_cast<unsigned char>(bytes[offset++])
        ) << shift;

    return value;
}

std::optional<std::uint64_t> read_u64(llvm::StringRef bytes, std::size_t& offset)
{
    if (offset > bytes.size() || bytes.size() - offset < sizeof(std::uint64_t))
        return std::nullopt;

    std::uint64_t value = 0;

    for (unsigned shift = 0; shift < 64; shift += 8)
        value |= static_cast<std::uint64_t>(
            static_cast<unsigned char>(bytes[offset++])
        ) << shift;

    return value;
}

llvm::SmallVector<char, 64> encode_cache_state(const CachedState& state)
{
    llvm::SmallVector<char, 64> bytes;

    append_u32(bytes, format);
    append_u64(bytes, state.imported_functions);
    append_u64(bytes, state.imported_data);
    append_u64(bytes, state.eliminated_functions);
    append_u64(bytes, state.eliminated_data);
    append_u64(bytes, state.eliminated_bytes);
    bytes.append(toolchain_identity.begin(), toolchain_identity.end());
    bytes.append(cache_magic.begin(), cache_magic.end());

    return bytes;
}

std::optional<std::pair<llvm::StringRef, CachedState>> decode_cache_entry(
    llvm::StringRef bytes
)
{
    constexpr std::size_t payload_size = sizeof(std::uint32_t) + 5 * sizeof(std::uint64_t);
    constexpr std::size_t trailer_size = payload_size
        + toolchain_identity.size()
        + cache_magic.size();

    if (bytes.size() < trailer_size)
        return std::nullopt;

    const llvm::StringRef trailer = bytes.take_back(trailer_size);

    if (trailer.take_back(cache_magic.size()) != cache_magic)
        return std::nullopt;

    if (trailer.slice(payload_size, payload_size + toolchain_identity.size())
        != toolchain_identity)
        return std::nullopt;

    std::size_t offset = 0;
    const std::optional<std::uint32_t> found_format = read_u32(trailer, offset);
    const std::optional<std::uint64_t> imported_functions = read_u64(trailer, offset);
    const std::optional<std::uint64_t> imported_data = read_u64(trailer, offset);
    const std::optional<std::uint64_t> eliminated_functions = read_u64(trailer, offset);
    const std::optional<std::uint64_t> eliminated_data = read_u64(trailer, offset);
    const std::optional<std::uint64_t> eliminated_bytes = read_u64(trailer, offset);

    if (found_format != format || !imported_functions || !imported_data
        || !eliminated_functions || !eliminated_data || !eliminated_bytes)
        return std::nullopt;

    return std::pair{
        bytes.drop_back(trailer_size),
        CachedState{
            *imported_functions,
            *imported_data,
            *eliminated_functions,
            *eliminated_data,
            *eliminated_bytes,
        },
    };
}

llvm::DenseMap<llvm::GlobalValue::GUID, DefinitionState> definitions(
    const llvm::Module& module
)
{
    llvm::DenseMap<llvm::GlobalValue::GUID, DefinitionState> result;

    for (const llvm::GlobalValue& value : module.global_values())
    {
        if (value.isDeclaration())
            continue;

        const DefinitionKind kind = value.getValueType()->isFunctionTy()
            ? DefinitionKind::Function
            : DefinitionKind::Data;

        std::string representation;
        llvm::raw_string_ostream output(representation);

        value.print(output);
        output.flush();

        result.try_emplace(
            value.getGUID(),
            DefinitionState{kind, static_cast<std::uint64_t>(representation.size())}
        );
    }

    return result;
}

std::uint64_t peak_resident_bytes()
{
#if defined(_WIN32)
    PROCESS_MEMORY_COUNTERS counters{};

    if (!GetProcessMemoryInfo(GetCurrentProcess(), &counters, sizeof(counters)))
        return 0;

    return static_cast<std::uint64_t>(counters.PeakWorkingSetSize);
#else
    rusage usage{};

    if (getrusage(RUSAGE_SELF, &usage) != 0)
        return 0;

#if defined(__APPLE__)
    return static_cast<std::uint64_t>(usage.ru_maxrss);
#else
    return static_cast<std::uint64_t>(usage.ru_maxrss) * 1024;
#endif
#endif
}

const char* driver_name()
{
#if defined(_WIN32)
    return "coff";
#elif defined(__APPLE__)
    return "macho";
#else
    return "elf";
#endif
}

class Telemetry
{
public:
    static Telemetry& get()
    {
        static Telemetry telemetry;

        return telemetry;
    }

    bool active() const
    {
        return report_path.has_value();
    }

    void configure(llvm::lto::Config& config)
    {
        if (!active())
            return;

        const llvm::lto::Config::ModuleHookFn prior_internalize =
            std::move(config.PostInternalizeModuleHook);
        const llvm::lto::Config::ModuleHookFn prior_import =
            std::move(config.PostImportModuleHook);
        const llvm::lto::Config::ModuleHookFn prior_opt =
            std::move(config.PostOptModuleHook);

        config.PostInternalizeModuleHook = [this, prior_internalize](
            unsigned task,
            const llvm::Module& module
        ) {
            record_internalized(task, module);

            return !prior_internalize || prior_internalize(task, module);
        };

        config.PostImportModuleHook = [this, prior_import](
            unsigned task,
            const llvm::Module& module
        ) {
            record_imported(task, module);

            return !prior_import || prior_import(task, module);
        };

        config.PostOptModuleHook = [this, prior_opt](
            unsigned task,
            const llvm::Module& module
        ) {
            record_optimized(task, module);

            return !prior_opt || prior_opt(task, module);
        };
    }

    void record_internalized(unsigned task, const llvm::Module& module)
    {
        acquire_worker();
        std::lock_guard lock(mutex);

        modules[task].internalized = definitions(module);
    }

    void record_imported(unsigned task, const llvm::Module& module)
    {
        const auto found = definitions(module);
        std::lock_guard lock(mutex);
        ModuleState& state = modules[task];

        for (const auto& [guid, definition] : found)
        {
            if (state.internalized.contains(guid))
                continue;

            if (definition.kind == DefinitionKind::Function)
                ++state.imported_functions;
            else
                ++state.imported_data;
        }

        state.imported = found;
    }

    void record_optimized(unsigned task, const llvm::Module& module)
    {
        const auto found = definitions(module);
        std::lock_guard lock(mutex);
        ModuleState& state = modules[task];

        for (const auto& [guid, definition] : state.imported)
        {
            if (found.contains(guid))
                continue;

            if (definition.kind == DefinitionKind::Function)
                ++state.eliminated_functions;
            else
                ++state.eliminated_data;

            state.eliminated_bytes += definition.bytes;
        }

        add_to_totals(state);
        release_worker();
    }

    CachedState cached_state(unsigned task)
    {
        std::lock_guard lock(mutex);
        const ModuleState& state = modules[task];

        return {
            state.imported_functions,
            state.imported_data,
            state.eliminated_functions,
            state.eliminated_data,
            state.eliminated_bytes,
        };
    }

    void record_cache_hit(const CachedState& state)
    {
        std::lock_guard lock(mutex);

        ++totals.cache_hits;
        totals.imported_functions += state.imported_functions;
        totals.imported_data += state.imported_data;
        totals.eliminated_functions += state.eliminated_functions;
        totals.eliminated_data += state.eliminated_data;
        totals.eliminated_bytes += state.eliminated_bytes;
    }

    void record_cache_miss()
    {
        std::lock_guard lock(mutex);

        ++totals.cache_misses;
    }

    void record_cache_write()
    {
        std::lock_guard lock(mutex);

        ++totals.cache_writes;
    }

    void acquire_worker()
    {
        const std::uint64_t active = active_workers.fetch_add(1) + 1;
        std::uint64_t observed = peak_workers.load();

        while (observed < active
            && !peak_workers.compare_exchange_weak(observed, active))
        {
        }
    }

    void release_worker()
    {
        active_workers.fetch_sub(1);
    }

    bool finish()
    {
        if (!active())
            return true;

        const std::string partial_path = *report_path + ".partial";
        std::error_code error;
        llvm::raw_fd_ostream output(partial_path, error);

        if (error)
            return false;

        Totals snapshot;

        {
            std::lock_guard lock(mutex);
            snapshot = totals;
        }

        output << "{\n"
               << "  \"format\": 1,\n"
               << "  \"toolchain\": \"" << toolchain_identity << "\",\n"
               << "  \"driver\": \"" << driver_name() << "\",\n"
               << "  \"imported_functions\": " << snapshot.imported_functions << ",\n"
               << "  \"imported_data\": " << snapshot.imported_data << ",\n"
               << "  \"eliminated_functions\": " << snapshot.eliminated_functions << ",\n"
               << "  \"eliminated_data\": " << snapshot.eliminated_data << ",\n"
               << "  \"eliminated_bytes\": " << snapshot.eliminated_bytes << ",\n"
               << "  \"cache_hits\": " << snapshot.cache_hits << ",\n"
               << "  \"cache_misses\": " << snapshot.cache_misses << ",\n"
               << "  \"cache_writes\": " << snapshot.cache_writes << ",\n"
               << "  \"reused_partitions\": " << snapshot.cache_hits << ",\n"
               << "  \"peak_resident_bytes\": " << peak_resident_bytes() << ",\n"
               << "  \"active_workers\": " << peak_workers.load() << "\n"
               << "}\n";

        output.close();

        if (output.has_error())
            return false;

        error = llvm::sys::fs::rename(partial_path, *report_path);

        return !error;
    }

    void abandon()
    {
        if (!active())
            return;

        const std::string partial_path = *report_path + ".partial";
        std::error_code ignored = llvm::sys::fs::remove(partial_path);

        static_cast<void>(ignored);
    }

private:
    Telemetry()
        : report_path(llvm::sys::Process::GetEnv(report_environment))
    {
    }

    void add_to_totals(const ModuleState& state)
    {
        totals.imported_functions += state.imported_functions;
        totals.imported_data += state.imported_data;
        totals.eliminated_functions += state.eliminated_functions;
        totals.eliminated_data += state.eliminated_data;
        totals.eliminated_bytes += state.eliminated_bytes;
    }

    std::optional<std::string> report_path;
    std::mutex mutex;
    llvm::DenseMap<unsigned, ModuleState> modules;
    Totals totals;
    std::atomic<std::uint64_t> active_workers{0};
    std::atomic<std::uint64_t> peak_workers{0};
};

class CacheStream final : public llvm::CachedFileStream
{
public:
    CacheStream(
        std::unique_ptr<llvm::raw_pwrite_stream> output,
        llvm::AddBufferFn add_buffer,
        llvm::sys::fs::TempFile temporary,
        std::string entry_path,
        std::string module_name,
        unsigned task,
        Telemetry& telemetry
    )
        : llvm::CachedFileStream(std::move(output), std::move(entry_path)),
          add_buffer(std::move(add_buffer)),
          temporary(std::move(temporary)),
          module_name(std::move(module_name)),
          task(task),
          telemetry(telemetry)
    {
    }

    llvm::Error commit() override
    {
        if (llvm::Error error = llvm::CachedFileStream::commit())
            return error;

        OS.reset();

        llvm::ErrorOr<std::unique_ptr<llvm::MemoryBuffer>> object =
            llvm::MemoryBuffer::getOpenFile(
                llvm::sys::fs::convertFDToNativeFile(temporary.FD),
                ObjectPathName,
                -1,
                false
            );

        if (!object)
            return llvm::createStringError(object.getError(), "cannot read ThinLTO cache entry");

        const llvm::StringRef object_bytes = (*object)->getBuffer();
        const CachedState state = telemetry.cached_state(task);
        const llvm::SmallVector<char, 64> trailer = encode_cache_state(state);
        llvm::raw_fd_ostream append(
            temporary.FD,
            false,
            true,
            llvm::raw_ostream::OStreamKind::OK_FDStream
        );

        append.write(trailer.data(), trailer.size());
        append.flush();

        if (append.has_error())
            return llvm::createStringError(append.error(), "cannot write ThinLTO cache telemetry");

        std::unique_ptr<llvm::MemoryBuffer> linked_object =
            llvm::MemoryBuffer::getMemBufferCopy(object_bytes, ObjectPathName);

        if (llvm::Error error = temporary.keep(ObjectPathName))
        {
            const std::error_code code = llvm::errorToErrorCode(std::move(error));
            const bool concurrent_entry = llvm::sys::fs::exists(ObjectPathName)
                && (code == std::errc::file_exists
                    || code == std::errc::permission_denied);

            if (!concurrent_entry)
            {
                return llvm::createStringError(
                    code,
                    "cannot publish ThinLTO cache entry"
                );
            }
        }
        else
        {
            telemetry.record_cache_write();
        }

        add_buffer(task, module_name, std::move(linked_object));

        return llvm::Error::success();
    }

private:
    llvm::AddBufferFn add_buffer;
    llvm::sys::fs::TempFile temporary;
    std::string module_name;
    unsigned task;
    Telemetry& telemetry;
};

llvm::ErrorOr<std::unique_ptr<llvm::MemoryBuffer>> read_cache_entry(
    llvm::StringRef path
)
{
    llvm::Expected<llvm::sys::fs::file_t> opened =
        llvm::sys::fs::openNativeFileForRead(path, llvm::sys::fs::OF_UpdateAtime);

    if (!opened)
        return llvm::errorToErrorCode(opened.takeError());

    llvm::sys::fs::file_t descriptor = *opened;
    llvm::ErrorOr<std::unique_ptr<llvm::MemoryBuffer>> buffer =
        llvm::MemoryBuffer::getOpenFile(descriptor, path, -1, false);

    const std::error_code close_error = llvm::sys::fs::closeFile(descriptor);

    if (!buffer)
        return buffer;

    if (close_error)
        return close_error;

    return std::move(*buffer);
}

}

namespace bray::lld
{

void configure_telemetry(llvm::lto::Config& config)
{
    Telemetry::get().configure(config);
}

llvm::Expected<llvm::FileCache> telemetry_cache(
    const llvm::Twine& directory_reference,
    llvm::AddBufferFn add_buffer
)
{
    Telemetry& telemetry = Telemetry::get();

    if (!telemetry.active())
        return llvm::localCache("ThinLTO", "Thin", directory_reference, std::move(add_buffer));

    llvm::SmallString<128> directory;
    directory_reference.toVector(directory);

    auto cache = [directory, add_buffer = std::move(add_buffer), &telemetry](
        unsigned task,
        llvm::StringRef key,
        const llvm::Twine& module_name
    ) -> llvm::Expected<llvm::AddStreamFn> {
        llvm::SmallString<160> entry_path;
        llvm::sys::path::append(entry_path, directory, "llvmcache-" + key);

        llvm::ErrorOr<std::unique_ptr<llvm::MemoryBuffer>> entry =
            read_cache_entry(entry_path);

        if (entry)
        {
            const auto decoded = decode_cache_entry((*entry)->getBuffer());

            if (decoded)
            {
                add_buffer(
                    task,
                    module_name,
                    llvm::MemoryBuffer::getMemBufferCopy(decoded->first, entry_path)
                );
                telemetry.record_cache_hit(decoded->second);

                return llvm::AddStreamFn{};
            }

            const std::error_code removal = llvm::sys::fs::remove(entry_path);

            if (removal && removal != std::errc::no_such_file_or_directory
                && removal != std::errc::permission_denied)
            {
                return llvm::createStringError(
                    removal,
                    "cannot replace invalid ThinLTO cache entry"
                );
            }
        }
        else if (entry.getError() != std::errc::no_such_file_or_directory
            && entry.getError() != std::errc::permission_denied)
        {
            return llvm::createStringError(entry.getError(), "cannot read ThinLTO cache entry");
        }

        telemetry.record_cache_miss();

        return [directory,
                entry_path = entry_path.str().str(),
                add_buffer,
                &telemetry](unsigned task, const llvm::Twine& module_name)
            -> llvm::Expected<std::unique_ptr<llvm::CachedFileStream>> {
            if (std::error_code error = llvm::sys::fs::create_directories(
                    directory,
                    true
                ))
            {
                return llvm::createStringError(error, "cannot create ThinLTO cache directory");
            }

            llvm::SmallString<160> temporary_model;
            llvm::sys::path::append(temporary_model, directory, "Thin-%%%%%%.tmp.o");
            llvm::Expected<llvm::sys::fs::TempFile> temporary =
                llvm::sys::fs::TempFile::create(
                    temporary_model,
                    llvm::sys::fs::owner_read | llvm::sys::fs::owner_write
                );

            if (!temporary)
                return temporary.takeError();

            const int descriptor = temporary->FD;

            return std::make_unique<CacheStream>(
                std::make_unique<llvm::raw_fd_ostream>(descriptor, false),
                add_buffer,
                std::move(*temporary),
                entry_path,
                module_name.str(),
                task,
                telemetry
            );
        };
    };

    return llvm::FileCache(std::move(cache), directory.str().str());
}

bool finish_telemetry()
{
    return Telemetry::get().finish();
}

void abandon_telemetry()
{
    Telemetry::get().abandon();
}

}
