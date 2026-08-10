#include "provider.h"

#include "date/tz.h"

#include <atomic>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <mutex>
#include <string>
#include <unordered_map>

#ifdef _WIN32
#include <process.h>
#else
#include <unistd.h>
#endif

#include "bray_tzdata.inc"

using namespace bray::temporal;

namespace
{

std::once_flag database_once;
std::mutex zones_mutex;
std::unordered_map<std::uint64_t, const date::time_zone*> zones;
std::unordered_map<std::string, std::uint64_t> zone_names;
std::unordered_map<std::uint64_t, std::uint64_t> zone_references;
std::atomic<std::uint64_t> next_zone{1};

std::uint64_t process_identity()
{
#ifdef _WIN32
    return static_cast<std::uint64_t>(_getpid());
#else
    return static_cast<std::uint64_t>(getpid());
#endif
}

std::filesystem::path create_database_directory()
{
    const auto root = std::filesystem::temp_directory_path();
    const auto seed = std::chrono::steady_clock::now().time_since_epoch().count();

    for (std::uint64_t attempt = 0; attempt != 64; ++attempt)
    {
        const auto name = std::string{"bray-tzdata-2026c-"} +
            std::to_string(process_identity()) + "-" + std::to_string(seed) + "-" +
            std::to_string(attempt);
        const auto path = root / name;

        if (std::filesystem::create_directory(path))
            return path;
    }

    throw std::runtime_error("could not create private timezone database directory");
}

void initialize_database()
{
    std::call_once(database_once, []
    {
        const auto directory = create_database_directory();

        try
        {
            for (const auto& file : BRAY_TZDATA_FILES)
            {
                const auto path = directory / file.name;
                std::ofstream output{path, std::ios::binary | std::ios::trunc};

                output.write(
                    reinterpret_cast<const char*>(file.bytes),
                    static_cast<std::streamsize>(file.length)
                );

                if (!output)
                    throw std::runtime_error("could not materialize embedded timezone data");
            }

            date::set_install(directory.string());
            static_cast<void>(date::get_tzdb());
        }
        catch (...)
        {
            std::error_code ignored;
            std::filesystem::remove_all(directory, ignored);
            throw;
        }

        std::error_code ignored;
        std::filesystem::remove_all(directory, ignored);
    });
}

const date::time_zone* zone(std::uint64_t handle)
{
    std::lock_guard lock{zones_mutex};
    const auto found = zones.find(handle);

    return found == zones.end() ? nullptr : found->second;
}

std::uint32_t publish_zone(const date::time_zone* value, std::uint64_t* handle)
{
    if (value == nullptr || handle == nullptr)
        return invalid_value;

    std::lock_guard lock{zones_mutex};
    const auto existing = zone_names.find(value->name());

    if (existing != zone_names.end())
    {
        ++zone_references.at(existing->second);
        *handle = existing->second;

        return success;
    }

    const auto id = next_zone.fetch_add(1, std::memory_order_relaxed);

    if (id == 0)
        return out_of_range;

    zones.emplace(id, value);
    zone_names.emplace(value->name(), id);
    zone_references.emplace(id, 1);
    *handle = id;

    return success;
}

}

extern "C" std::uint32_t bray_temporal_zone_load(
    const std::uint8_t* name,
    std::uint64_t length,
    std::uint64_t* handle
)
{
    if (
        handle == nullptr ||
        (length != 0 && name == nullptr) ||
        length > std::numeric_limits<std::size_t>::max()
    )
        return invalid_value;

    try
    {
        initialize_database();
    }
    catch (...)
    {
        return provider_failure;
    }

    try
    {
        const auto value = text_value(name, length);

        return publish_zone(date::locate_zone(value), handle);
    }
    catch (const std::runtime_error&)
    {
        return unknown_zone;
    }
    catch (...)
    {
        return provider_failure;
    }
}

extern "C" std::uint32_t bray_temporal_zone_local(std::uint64_t* handle)
{
    if (handle == nullptr)
        return invalid_value;

    try
    {
        initialize_database();

        if (const auto* name = std::getenv("TZ"); name != nullptr && *name != '\0')
            return publish_zone(date::locate_zone(name), handle);

        return publish_zone(date::current_zone(), handle);
    }
    catch (...)
    {
        return unavailable;
    }
}

extern "C" std::uint32_t bray_temporal_zone_retain(std::uint64_t handle)
{
    try
    {
        std::lock_guard lock{zones_mutex};
        const auto found = zone_references.find(handle);

        if (
            found == zone_references.end() ||
            found->second == std::numeric_limits<std::uint64_t>::max()
        )
            return invalid_value;

        ++found->second;

        return success;
    }
    catch (...)
    {
        return provider_failure;
    }
}

extern "C" std::uint32_t bray_temporal_zone_close(std::uint64_t handle)
{
    try
    {
        std::lock_guard lock{zones_mutex};
        const auto found = zone_references.find(handle);

        if (found == zone_references.end())
            return invalid_value;

        if (--found->second != 0)
            return success;

        const auto value = zones.at(handle);

        zone_names.erase(value->name());
        zones.erase(handle);
        zone_references.erase(found);

        return success;
    }
    catch (...)
    {
        return provider_failure;
    }
}

extern "C" std::uint32_t bray_temporal_zone_name(
    std::uint64_t handle,
    std::uint8_t* destination,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
)
{
    try
    {
        const auto* value = zone(handle);

        return value == nullptr ? invalid_value :
            copy_text(value->name(), destination, capacity, written_or_required);
    }
    catch (...)
    {
        return provider_failure;
    }
}

extern "C" std::uint32_t bray_temporal_observe(
    std::uint64_t zone_handle,
    std::int32_t fixed_offset_seconds,
    std::int64_t seconds,
    std::uint32_t nanoseconds,
    BrayTemporalObservation* observation,
    std::uint8_t* abbreviation,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
)
{
    if (observation == nullptr)
        return invalid_value;

    try
    {
        const auto instant = timestamp(seconds, nanoseconds);
        std::chrono::seconds offset{fixed_offset_seconds};
        std::chrono::minutes save{0};
        std::string name = "UTC";

        if (zone_handle != 0)
        {
            const auto* value = zone(zone_handle);

            if (value == nullptr)
                return invalid_value;

            const auto info = value->get_info(instant);

            offset = info.offset;
            save = info.save;
            name = info.abbrev;
        }

        const auto local = date::local_time<std::chrono::nanoseconds>{
            instant.time_since_epoch() + offset
        };
        const auto copied = copy_text(name, abbreviation, capacity, written_or_required);

        if (copied != success)
            return copied;

        *observation = {
            civil_fields(local),
            static_cast<std::int32_t>(offset.count()),
            save == std::chrono::minutes{0} ? 0U : 1U,
        };

        return success;
    }
    catch (const std::invalid_argument&)
    {
        return invalid_value;
    }
    catch (...)
    {
        return out_of_range;
    }
}

extern "C" std::uint32_t bray_temporal_resolve(
    std::uint64_t zone_handle,
    std::int32_t fixed_offset_seconds,
    BrayTemporalDateTime local_value,
    BrayTemporalResolution* resolution
)
{
    if (resolution == nullptr)
        return invalid_value;

    try
    {
        const auto local = local_time(local_value);
        date::sys_time<std::chrono::nanoseconds> first;
        date::sys_time<std::chrono::nanoseconds> second;
        std::uint32_t kind = 0;

        if (zone_handle == 0)
        {
            first = date::sys_time<std::chrono::nanoseconds>{
                local.time_since_epoch() - std::chrono::seconds{fixed_offset_seconds}
            };
            second = first;
        }
        else
        {
            const auto* value = zone(zone_handle);

            if (value == nullptr)
                return invalid_value;

            const auto info = value->get_info(local);

            if (info.result == date::local_info::unique)
            {
                first = value->to_sys(local);
                second = first;
            }
            else if (info.result == date::local_info::ambiguous)
            {
                kind = 1;
                first = value->to_sys(local, date::choose::earliest);
                second = value->to_sys(local, date::choose::latest);
            }
            else
            {
                kind = 2;
                first = date::sys_time<std::chrono::nanoseconds>{
                    info.first.end.time_since_epoch()
                } - std::chrono::nanoseconds{1};
                second = date::sys_time<std::chrono::nanoseconds>{
                    info.second.begin.time_since_epoch()
                };
            }
        }

        std::int64_t first_seconds = 0;
        std::uint32_t first_nanoseconds = 0;
        std::int64_t second_seconds = 0;
        std::uint32_t second_nanoseconds = 0;

        timestamp_parts(first, first_seconds, first_nanoseconds);
        timestamp_parts(second, second_seconds, second_nanoseconds);

        *resolution = {
            kind,
            0,
            first_seconds,
            first_nanoseconds,
            0,
            second_seconds,
            second_nanoseconds,
            0,
        };

        return success;
    }
    catch (const std::invalid_argument&)
    {
        return invalid_value;
    }
    catch (...)
    {
        return out_of_range;
    }
}
