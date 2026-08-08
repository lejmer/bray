#ifndef BRAY_TEMPORAL_H
#define BRAY_TEMPORAL_H

#include <cstddef>
#include <cstdint>

extern "C"
{

struct BrayTemporalDateTime
{
    std::int32_t year;
    std::uint32_t month;
    std::uint32_t day;
    std::uint32_t hour;
    std::uint32_t minute;
    std::uint32_t second;
    std::uint32_t nanosecond;
};

struct BrayTemporalObservation
{
    BrayTemporalDateTime local;
    std::int32_t offset_seconds;
    std::uint32_t daylight;
};

struct BrayTemporalResolution
{
    std::uint32_t kind;
    std::uint32_t reserved;
    std::int64_t first_seconds;
    std::uint32_t first_nanoseconds;
    std::uint32_t first_reserved;
    std::int64_t second_seconds;
    std::uint32_t second_nanoseconds;
    std::uint32_t second_reserved;
};

struct BrayTemporalValue
{
    BrayTemporalDateTime local;
    std::int64_t timestamp_seconds;
    std::int32_t offset_seconds;
    std::uint32_t reserved;
};

std::uint32_t bray_temporal_date_validate(
    std::int32_t year,
    std::uint32_t month,
    std::uint32_t day
);

std::uint32_t bray_temporal_date_add(
    BrayTemporalDateTime value,
    std::int32_t years,
    std::int32_t months,
    std::int32_t days,
    std::uint32_t adjustment,
    BrayTemporalDateTime* result
);

std::uint32_t bray_temporal_zone_load(
    const std::uint8_t* name,
    std::uint64_t length,
    std::uint64_t* handle
);

std::uint32_t bray_temporal_zone_local(std::uint64_t* handle);
std::uint32_t bray_temporal_zone_retain(std::uint64_t handle);
std::uint32_t bray_temporal_zone_close(std::uint64_t handle);

std::uint32_t bray_temporal_zone_name(
    std::uint64_t handle,
    std::uint8_t* destination,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
);

std::uint32_t bray_temporal_observe(
    std::uint64_t zone,
    std::int32_t fixed_offset_seconds,
    std::int64_t seconds,
    std::uint32_t nanoseconds,
    BrayTemporalObservation* observation,
    std::uint8_t* abbreviation,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
);

std::uint32_t bray_temporal_resolve(
    std::uint64_t zone,
    std::int32_t fixed_offset_seconds,
    BrayTemporalDateTime local,
    BrayTemporalResolution* resolution
);

std::uint32_t bray_temporal_parse(
    std::uint32_t kind,
    const std::uint8_t* text,
    std::uint64_t length,
    BrayTemporalValue* value,
    std::uint64_t* invalid_offset
);

std::uint32_t bray_temporal_format(
    std::uint32_t kind,
    BrayTemporalValue value,
    std::uint8_t* destination,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
);

}

#endif
