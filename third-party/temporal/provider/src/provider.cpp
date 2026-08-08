#include "bray_temporal.h"

#include "date/date.h"
#include "date/tz.h"

#include <atomic>
#include <chrono>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <limits>
#include <mutex>
#include <sstream>
#include <string>
#include <unordered_map>

#ifdef _WIN32
#include <process.h>
#else
#include <unistd.h>
#endif

#include "bray_tzdata.inc"

namespace {

constexpr std::uint32_t success = 0;
constexpr std::uint32_t unavailable = 1;
constexpr std::uint32_t invalid_value = 2;
constexpr std::uint32_t unknown_zone = 3;
constexpr std::uint32_t out_of_range = 4;
constexpr std::uint32_t invalid_format = 5;
constexpr std::uint32_t insufficient_buffer = 6;
constexpr std::uint32_t provider_failure = 7;
constexpr std::uint32_t nanoseconds_per_second = 1000000000;

std::once_flag database_once;
std::mutex zones_mutex;
std::unordered_map<std::uint64_t, const date::time_zone*> zones;
std::unordered_map<std::string, std::uint64_t> zone_names;
std::unordered_map<std::uint64_t, std::uint64_t> zone_references;
std::atomic<std::uint64_t> next_zone{1};

std::uint64_t process_identity() {
#ifdef _WIN32
    return static_cast<std::uint64_t>(_getpid());
#else
    return static_cast<std::uint64_t>(getpid());
#endif
}

std::filesystem::path create_database_directory() {
    const auto root = std::filesystem::temp_directory_path();
    const auto seed = std::chrono::steady_clock::now().time_since_epoch().count();

    for (std::uint64_t attempt = 0; attempt != 64; ++attempt) {
        const auto name = std::string{"bray-tzdata-2026c-"} +
            std::to_string(process_identity()) + "-" + std::to_string(seed) + "-" +
            std::to_string(attempt);
        const auto path = root / name;

        if (std::filesystem::create_directory(path)) {
            return path;
        }
    }

    throw std::runtime_error("could not create private timezone database directory");
}

void initialize_database() {
    std::call_once(database_once, [] {
        const auto directory = create_database_directory();

        try {
            for (const auto& file : BRAY_TZDATA_FILES) {
                const auto path = directory / file.name;
                std::ofstream output{path, std::ios::binary | std::ios::trunc};

                output.write(
                    reinterpret_cast<const char*>(file.bytes),
                    static_cast<std::streamsize>(file.length)
                );

                if (!output) {
                    throw std::runtime_error("could not materialize embedded timezone data");
                }
            }

            date::set_install(directory.string());
            static_cast<void>(date::get_tzdb());
        } catch (...) {
            std::error_code ignored;
            std::filesystem::remove_all(directory, ignored);
            throw;
        }

        std::error_code ignored;
        std::filesystem::remove_all(directory, ignored);
    });
}

date::year_month_day checked_date(
    std::int32_t year,
    std::uint32_t month,
    std::uint32_t day
) {
    const auto value = date::year{year} / date::month{month} / date::day{day};

    if (!value.ok()) {
        throw std::invalid_argument("invalid civil date");
    }

    return value;
}

date::local_time<std::chrono::nanoseconds> local_time(BrayTemporalDateTime value) {
    const auto day = date::local_days{checked_date(value.year, value.month, value.day)};

    if (value.hour >= 24 || value.minute >= 60 || value.second >= 60 ||
        value.nanosecond >= nanoseconds_per_second) {
        throw std::invalid_argument("invalid civil time");
    }

    return day + std::chrono::hours{value.hour} + std::chrono::minutes{value.minute} +
        std::chrono::seconds{value.second} + std::chrono::nanoseconds{value.nanosecond};
}

BrayTemporalDateTime civil_fields(date::local_time<std::chrono::nanoseconds> value) {
    const auto day = date::floor<date::days>(value);
    const date::year_month_day date{date::local_days{day.time_since_epoch()}};
    const date::hh_mm_ss time{value - day};

    return {
        static_cast<std::int32_t>(date.year()),
        static_cast<std::uint32_t>(date.month()),
        static_cast<std::uint32_t>(date.day()),
        static_cast<std::uint32_t>(time.hours().count()),
        static_cast<std::uint32_t>(time.minutes().count()),
        static_cast<std::uint32_t>(time.seconds().count()),
        static_cast<std::uint32_t>(time.subseconds().count()),
    };
}

date::sys_time<std::chrono::nanoseconds> timestamp(
    std::int64_t seconds,
    std::uint32_t nanoseconds
) {
    if (nanoseconds >= nanoseconds_per_second) {
        throw std::invalid_argument("invalid timestamp nanoseconds");
    }

    return date::sys_time<std::chrono::nanoseconds>{
        std::chrono::seconds{seconds} + std::chrono::nanoseconds{nanoseconds}
    };
}

void timestamp_parts(
    date::sys_time<std::chrono::nanoseconds> value,
    std::int64_t& seconds,
    std::uint32_t& nanoseconds
) {
    const auto whole = date::floor<std::chrono::seconds>(value);
    const auto remainder = value - whole;

    seconds = whole.time_since_epoch().count();
    nanoseconds = static_cast<std::uint32_t>(remainder.count());
}

const date::time_zone* zone(std::uint64_t handle) {
    std::lock_guard lock{zones_mutex};
    const auto found = zones.find(handle);

    return found == zones.end() ? nullptr : found->second;
}

std::uint32_t publish_zone(const date::time_zone* value, std::uint64_t* handle) {
    if (value == nullptr || handle == nullptr) {
        return invalid_value;
    }

    std::lock_guard lock{zones_mutex};
    const auto existing = zone_names.find(value->name());

    if (existing != zone_names.end()) {
        ++zone_references.at(existing->second);
        *handle = existing->second;

        return success;
    }

    const auto id = next_zone.fetch_add(1, std::memory_order_relaxed);

    if (id == 0) {
        return out_of_range;
    }

    zones.emplace(id, value);
    zone_names.emplace(value->name(), id);
    zone_references.emplace(id, 1);
    *handle = id;

    return success;
}

std::string text_value(const std::uint8_t* text, std::uint64_t length) {
    if (length == 0) {
        return {};
    }

    return {
        reinterpret_cast<const char*>(text),
        static_cast<std::size_t>(length),
    };
}

std::uint32_t copy_text(
    const std::string& value,
    std::uint8_t* destination,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
) {
    if (written_or_required == nullptr ||
        value.size() > std::numeric_limits<std::uint64_t>::max()) {
        return invalid_value;
    }

    const auto required = static_cast<std::uint64_t>(value.size());
    *written_or_required = required;

    if (capacity < required) {
        return insufficient_buffer;
    }

    if (required != 0 && destination == nullptr) {
        return invalid_value;
    }

    if (required != 0) {
        std::copy(value.begin(), value.end(), destination);
    }

    return success;
}

bool digit(char value) {
    return value >= '0' && value <= '9';
}

bool unsigned_field(
    const std::string& text,
    std::size_t& cursor,
    std::size_t digits,
    std::uint32_t& value
) {
    if (text.size() - cursor < digits) {
        return false;
    }

    value = 0;

    for (std::size_t index = 0; index != digits; ++index) {
        const auto character = text[cursor + index];

        if (!digit(character)) {
            return false;
        }

        value = value * 10 + static_cast<std::uint32_t>(character - '0');
    }

    cursor += digits;

    return true;
}

bool character(const std::string& text, std::size_t& cursor, char expected) {
    if (cursor == text.size() || text[cursor] != expected) {
        return false;
    }

    ++cursor;

    return true;
}

bool parse_date(const std::string& text, std::size_t& cursor, BrayTemporalDateTime& value) {
    bool negative = false;

    if (cursor != text.size() && (text[cursor] == '-' || text[cursor] == '+')) {
        negative = text[cursor] == '-';
        ++cursor;
    }

    std::uint32_t year = 0;

    if (!unsigned_field(text, cursor, 4, year) || !character(text, cursor, '-') ||
        !unsigned_field(text, cursor, 2, value.month) || !character(text, cursor, '-') ||
        !unsigned_field(text, cursor, 2, value.day) || year > 32767) {
        return false;
    }

    value.year = negative ? -static_cast<std::int32_t>(year) : static_cast<std::int32_t>(year);

    return true;
}

bool parse_time(const std::string& text, std::size_t& cursor, BrayTemporalDateTime& value) {
    if (!unsigned_field(text, cursor, 2, value.hour) || !character(text, cursor, ':') ||
        !unsigned_field(text, cursor, 2, value.minute) || !character(text, cursor, ':') ||
        !unsigned_field(text, cursor, 2, value.second)) {
        return false;
    }

    value.nanosecond = 0;

    if (cursor != text.size() && text[cursor] == '.') {
        ++cursor;
        std::size_t digits = 0;

        while (cursor != text.size() && digit(text[cursor]) && digits != 9) {
            value.nanosecond = value.nanosecond * 10 +
                static_cast<std::uint32_t>(text[cursor] - '0');
            ++cursor;
            ++digits;
        }

        if (digits == 0 || (cursor != text.size() && digit(text[cursor]))) {
            return false;
        }

        while (digits != 9) {
            value.nanosecond *= 10;
            ++digits;
        }
    }

    return true;
}

std::string padded(std::int64_t value, std::size_t width) {
    auto text = std::to_string(value);

    if (text.size() < width) {
        text.insert(0, width - text.size(), '0');
    }

    return text;
}

std::string format_date(BrayTemporalDateTime value) {
    const auto year = static_cast<std::int64_t>(value.year);
    const auto magnitude = year < 0 ? -year : year;

    return (year < 0 ? "-" : "") + padded(magnitude, 4) + "-" + padded(value.month, 2) +
        "-" + padded(value.day, 2);
}

std::string format_time(BrayTemporalDateTime value) {
    auto result = padded(value.hour, 2) + ":" + padded(value.minute, 2) + ":" +
        padded(value.second, 2);

    if (value.nanosecond != 0) {
        auto fraction = padded(value.nanosecond, 9);

        while (fraction.back() == '0') {
            fraction.pop_back();
        }

        result += "." + fraction;
    }

    return result;
}

}  // namespace

extern "C" std::uint32_t bray_temporal_date_validate(
    std::int32_t year,
    std::uint32_t month,
    std::uint32_t day
) {
    try {
        static_cast<void>(checked_date(year, month, day));
        return success;
    } catch (const std::invalid_argument&) {
        return invalid_value;
    } catch (...) {
        return provider_failure;
    }
}

extern "C" std::uint32_t bray_temporal_date_add(
    BrayTemporalDateTime value,
    std::int32_t years,
    std::int32_t months,
    std::int32_t days,
    std::uint32_t adjustment,
    BrayTemporalDateTime* result
) {
    if (result == nullptr || adjustment > 1) {
        return invalid_value;
    }

    try {
        auto date = checked_date(value.year, value.month, value.day);
        date = date + date::years{years} + date::months{months};

        if (!date.ok()) {
            if (adjustment == 0) {
                return invalid_value;
            }

            date = date::year_month_day{
                date::sys_days{date.year() / date.month() / date::last}
            };
        }

        date = date::year_month_day{date::sys_days{date} + date::days{days}};

        if (!date.ok()) {
            return out_of_range;
        }

        *result = {
            static_cast<std::int32_t>(date.year()),
            static_cast<std::uint32_t>(date.month()),
            static_cast<std::uint32_t>(date.day()),
            value.hour,
            value.minute,
            value.second,
            value.nanosecond,
        };

        return success;
    } catch (const std::invalid_argument&) {
        return invalid_value;
    } catch (...) {
        return out_of_range;
    }
}

extern "C" std::uint32_t bray_temporal_zone_load(
    const std::uint8_t* name,
    std::uint64_t length,
    std::uint64_t* handle
) {
    if (handle == nullptr || (length != 0 && name == nullptr) ||
        length > std::numeric_limits<std::size_t>::max()) {
        return invalid_value;
    }

    try {
        initialize_database();
    } catch (...) {
        return provider_failure;
    }

    try {
        const auto value = text_value(name, length);

        return publish_zone(date::locate_zone(value), handle);
    } catch (const std::runtime_error&) {
        return unknown_zone;
    } catch (...) {
        return provider_failure;
    }
}

extern "C" std::uint32_t bray_temporal_zone_local(std::uint64_t* handle) {
    if (handle == nullptr) {
        return invalid_value;
    }

    try {
        initialize_database();

        if (const auto* name = std::getenv("TZ"); name != nullptr && *name != '\0') {
            return publish_zone(date::locate_zone(name), handle);
        }

        return publish_zone(date::current_zone(), handle);
    } catch (...) {
        return unavailable;
    }
}

extern "C" std::uint32_t bray_temporal_zone_retain(std::uint64_t handle) {
    std::lock_guard lock{zones_mutex};
    const auto found = zone_references.find(handle);

    if (found == zone_references.end() || found->second == std::numeric_limits<std::uint64_t>::max()) {
        return invalid_value;
    }

    ++found->second;

    return success;
}

extern "C" std::uint32_t bray_temporal_zone_close(std::uint64_t handle) {
    std::lock_guard lock{zones_mutex};
    const auto found = zone_references.find(handle);

    if (found == zone_references.end()) {
        return invalid_value;
    }

    if (--found->second != 0) {
        return success;
    }

    const auto value = zones.at(handle);

    zone_names.erase(value->name());
    zones.erase(handle);
    zone_references.erase(found);

    return success;
}

extern "C" std::uint32_t bray_temporal_zone_name(
    std::uint64_t handle,
    std::uint8_t* destination,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
) {
    try {
        const auto* value = zone(handle);

        return value == nullptr ? invalid_value :
            copy_text(value->name(), destination, capacity, written_or_required);
    } catch (...) {
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
) {
    if (observation == nullptr) {
        return invalid_value;
    }

    try {
        const auto instant = timestamp(seconds, nanoseconds);
        std::chrono::seconds offset{fixed_offset_seconds};
        std::chrono::minutes save{0};
        std::string name = "UTC";

        if (zone_handle != 0) {
            const auto* value = zone(zone_handle);

            if (value == nullptr) {
                return invalid_value;
            }

            const auto info = value->get_info(instant);

            offset = info.offset;
            save = info.save;
            name = info.abbrev;
        }

        const auto local = date::local_time<std::chrono::nanoseconds>{
            instant.time_since_epoch() + offset
        };
        const auto copied = copy_text(name, abbreviation, capacity, written_or_required);

        if (copied != success) {
            return copied;
        }

        *observation = {
            civil_fields(local),
            static_cast<std::int32_t>(offset.count()),
            save == std::chrono::minutes{0} ? 0U : 1U,
        };

        return success;
    } catch (const std::invalid_argument&) {
        return invalid_value;
    } catch (...) {
        return out_of_range;
    }
}

extern "C" std::uint32_t bray_temporal_resolve(
    std::uint64_t zone_handle,
    std::int32_t fixed_offset_seconds,
    BrayTemporalDateTime local_value,
    BrayTemporalResolution* resolution
) {
    if (resolution == nullptr) {
        return invalid_value;
    }

    try {
        const auto local = local_time(local_value);
        date::sys_time<std::chrono::nanoseconds> first;
        date::sys_time<std::chrono::nanoseconds> second;
        std::uint32_t kind = 0;

        if (zone_handle == 0) {
            first = date::sys_time<std::chrono::nanoseconds>{
                local.time_since_epoch() - std::chrono::seconds{fixed_offset_seconds}
            };
            second = first;
        } else {
            const auto* value = zone(zone_handle);

            if (value == nullptr) {
                return invalid_value;
            }

            const auto info = value->get_info(local);

            if (info.result == date::local_info::unique) {
                first = value->to_sys(local);
                second = first;
            } else if (info.result == date::local_info::ambiguous) {
                kind = 1;
                first = value->to_sys(local, date::choose::earliest);
                second = value->to_sys(local, date::choose::latest);
            } else {
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
    } catch (const std::invalid_argument&) {
        return invalid_value;
    } catch (...) {
        return out_of_range;
    }
}

extern "C" std::uint32_t bray_temporal_parse(
    std::uint32_t kind,
    const std::uint8_t* text,
    std::uint64_t length,
    BrayTemporalValue* value,
    std::uint64_t* invalid_offset
) {
    if (value == nullptr || invalid_offset == nullptr || (length != 0 && text == nullptr) ||
        length > std::numeric_limits<std::size_t>::max() || kind > 3) {
        return invalid_value;
    }

    const auto source = text_value(text, length);
    std::size_t cursor = 0;
    BrayTemporalValue parsed{};
    bool valid = kind == 1 ? parse_time(source, cursor, parsed.local) :
        parse_date(source, cursor, parsed.local);

    if (valid && (kind == 2 || kind == 3)) {
        valid = character(source, cursor, 'T') && parse_time(source, cursor, parsed.local);
    }

    if (valid && kind == 3) {
        std::int32_t offset = 0;

        if (cursor != source.size() && source[cursor] == 'Z') {
            ++cursor;
        } else {
            const bool negative = cursor != source.size() && source[cursor] == '-';

            if (cursor == source.size() || (source[cursor] != '+' && source[cursor] != '-')) {
                valid = false;
            } else {
                ++cursor;
                std::uint32_t hour = 0;
                std::uint32_t minute = 0;

                valid = unsigned_field(source, cursor, 2, hour) &&
                    character(source, cursor, ':') &&
                    unsigned_field(source, cursor, 2, minute) && hour <= 23 && minute <= 59;
                offset = static_cast<std::int32_t>(hour * 3600 + minute * 60);

                if (negative) {
                    offset = -offset;
                }
            }
        }

        parsed.offset_seconds = offset;

        if (valid) {
            try {
                const auto instant = date::sys_time<std::chrono::nanoseconds>{
                    local_time(parsed.local).time_since_epoch() - std::chrono::seconds{offset}
                };

                timestamp_parts(instant, parsed.timestamp_seconds, parsed.local.nanosecond);
            } catch (...) {
                valid = false;
            }
        }
    }

    if (valid) {
        try {
            if (kind == 0) {
                static_cast<void>(checked_date(parsed.local.year, parsed.local.month, parsed.local.day));
            } else if (kind == 1) {
                parsed.local.year = 1970;
                parsed.local.month = 1;
                parsed.local.day = 1;
                static_cast<void>(local_time(parsed.local));
            } else {
                static_cast<void>(local_time(parsed.local));
            }
        } catch (...) {
            valid = false;
        }
    }

    if (!valid || cursor != source.size()) {
        *invalid_offset = static_cast<std::uint64_t>(cursor);
        return invalid_format;
    }

    *value = parsed;
    *invalid_offset = static_cast<std::uint64_t>(source.size());

    return success;
}

extern "C" std::uint32_t bray_temporal_format(
    std::uint32_t kind,
    BrayTemporalValue value,
    std::uint8_t* destination,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
) {
    if (kind > 3) {
        return invalid_value;
    }

    try {
        std::string result;

        if (kind == 0) {
            static_cast<void>(checked_date(value.local.year, value.local.month, value.local.day));
            result = format_date(value.local);
        } else if (kind == 1) {
            static_cast<void>(local_time(value.local));
            result = format_time(value.local);
        } else if (kind == 2) {
            static_cast<void>(local_time(value.local));
            result = format_date(value.local) + "T" + format_time(value.local);
        } else {
            const auto instant = timestamp(value.timestamp_seconds, value.local.nanosecond);
            const auto local = date::local_time<std::chrono::nanoseconds>{
                instant.time_since_epoch() + std::chrono::seconds{value.offset_seconds}
            };
            const auto fields = civil_fields(local);

            result = format_date(fields) + "T" + format_time(fields);

            if (value.offset_seconds == 0) {
                result += "Z";
            } else {
                const auto magnitude = value.offset_seconds < 0 ?
                    -static_cast<std::int64_t>(value.offset_seconds) : value.offset_seconds;

                result += value.offset_seconds < 0 ? "-" : "+";
                result += padded(magnitude / 3600, 2) + ":" + padded(magnitude % 3600 / 60, 2);
            }
        }

        return copy_text(result, destination, capacity, written_or_required);
    } catch (const std::invalid_argument&) {
        return invalid_value;
    } catch (...) {
        return out_of_range;
    }
}
