#ifndef BRAY_TEMPORAL_PROVIDER_H
#define BRAY_TEMPORAL_PROVIDER_H

#include "bray_temporal.h"

#include "date/date.h"

#include <algorithm>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include <string>

namespace bray::temporal
{

constexpr std::uint32_t success = 0;
constexpr std::uint32_t unavailable = 1;
constexpr std::uint32_t invalid_value = 2;
constexpr std::uint32_t unknown_zone = 3;
constexpr std::uint32_t out_of_range = 4;
constexpr std::uint32_t invalid_format = 5;
constexpr std::uint32_t insufficient_buffer = 6;
constexpr std::uint32_t provider_failure = 7;
constexpr std::uint32_t nanoseconds_per_second = 1000000000;
constexpr std::int32_t minimum_year = -32767;
constexpr std::int32_t maximum_year = 32767;

inline date::year_month_day checked_date(
    std::int32_t year,
    std::uint32_t month,
    std::uint32_t day
)
{
    if (year < minimum_year || year > maximum_year)
        throw std::out_of_range("civil year is outside the provider range");

    const auto value = date::year{year} / date::month{month} / date::day{day};

    if (!value.ok())
        throw std::invalid_argument("invalid civil date");

    return value;
}

inline date::local_time<std::chrono::nanoseconds> local_time(BrayTemporalDateTime value)
{
    const auto day = date::local_days{checked_date(value.year, value.month, value.day)};

    if (
        value.hour >= 24 ||
        value.minute >= 60 ||
        value.second >= 60 ||
        value.nanosecond >= nanoseconds_per_second
    )
        throw std::invalid_argument("invalid civil time");

    return day + std::chrono::hours{value.hour} + std::chrono::minutes{value.minute} +
        std::chrono::seconds{value.second} + std::chrono::nanoseconds{value.nanosecond};
}

inline BrayTemporalDateTime civil_fields(date::local_time<std::chrono::nanoseconds> value)
{
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

inline date::sys_time<std::chrono::nanoseconds> timestamp(
    std::int64_t seconds,
    std::uint32_t nanoseconds
)
{
    if (nanoseconds >= nanoseconds_per_second)
        throw std::invalid_argument("invalid timestamp nanoseconds");

    constexpr auto maximum = static_cast<std::uint64_t>(
        std::numeric_limits<std::int64_t>::max()
    );

    std::int64_t count = 0;

    if (seconds >= 0)
    {
        const auto whole = static_cast<std::uint64_t>(seconds);

        if (
            whole > maximum / nanoseconds_per_second ||
            (
                whole == maximum / nanoseconds_per_second &&
                nanoseconds > maximum % nanoseconds_per_second
            )
        )
            throw std::out_of_range("timestamp exceeds the nanosecond clock range");

        count = static_cast<std::int64_t>(whole * nanoseconds_per_second + nanoseconds);
    }
    else
    {
        constexpr auto minimum_magnitude = maximum + 1;
        const auto whole = static_cast<std::uint64_t>(-(seconds + 1));
        const auto remainder = static_cast<std::uint64_t>(nanoseconds_per_second - nanoseconds);

        if (whole > (minimum_magnitude - remainder) / nanoseconds_per_second)
            throw std::out_of_range("timestamp precedes the nanosecond clock range");

        const auto magnitude = whole * nanoseconds_per_second + remainder;
        count = magnitude == minimum_magnitude ? std::numeric_limits<std::int64_t>::min() :
            -static_cast<std::int64_t>(magnitude);
    }

    return date::sys_time<std::chrono::nanoseconds>{std::chrono::nanoseconds{count}};
}

inline void timestamp_parts(
    date::sys_time<std::chrono::nanoseconds> value,
    std::int64_t& seconds,
    std::uint32_t& nanoseconds
)
{
    const auto whole = date::floor<std::chrono::seconds>(value);
    const auto remainder = value - whole;

    seconds = whole.time_since_epoch().count();
    nanoseconds = static_cast<std::uint32_t>(remainder.count());
}

inline std::string text_value(const std::uint8_t* text, std::uint64_t length)
{
    if (length == 0)
        return {};

    return {
        reinterpret_cast<const char*>(text),
        static_cast<std::size_t>(length),
    };
}

inline std::uint32_t copy_text(
    const std::string& value,
    std::uint8_t* destination,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
)
{
    if (
        written_or_required == nullptr ||
        value.size() > std::numeric_limits<std::uint64_t>::max()
    )
        return invalid_value;

    const auto required = static_cast<std::uint64_t>(value.size());
    *written_or_required = required;

    if (capacity < required)
        return insufficient_buffer;

    if (required != 0 && destination == nullptr)
        return invalid_value;

    if (required != 0)
        std::copy(value.begin(), value.end(), destination);

    return success;
}

}

#endif
