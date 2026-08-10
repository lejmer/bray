#include "provider.h"

#include <string>

using namespace bray::temporal;

namespace
{

bool digit(char value)
{
    return value >= '0' && value <= '9';
}

bool unsigned_field(
    const std::string& text,
    std::size_t& cursor,
    std::size_t digits,
    std::uint32_t& value
)
{
    if (text.size() - cursor < digits)
        return false;

    value = 0;

    for (std::size_t index = 0; index != digits; ++index)
    {
        const auto character = text[cursor + index];

        if (!digit(character))
            return false;

        value = value * 10 + static_cast<std::uint32_t>(character - '0');
    }

    cursor += digits;

    return true;
}

bool character(const std::string& text, std::size_t& cursor, char expected)
{
    if (cursor == text.size() || text[cursor] != expected)
        return false;

    ++cursor;

    return true;
}

bool parse_date(const std::string& text, std::size_t& cursor, BrayTemporalDateTime& value)
{
    char sign = '\0';

    if (cursor != text.size() && (text[cursor] == '-' || text[cursor] == '+'))
    {
        sign = text[cursor];
        ++cursor;
    }

    const auto year_start = cursor;

    while (cursor != text.size() && digit(text[cursor]) && cursor - year_start != 6)
        ++cursor;

    const auto year_digits = cursor - year_start;
    std::uint32_t year = 0;
    std::size_t year_cursor = year_start;

    if (
        (year_digits != 4 && year_digits != 6) ||
        (sign == '+' && year_digits != 6) ||
        (sign == '\0' && year_digits != 4) ||
        !unsigned_field(text, year_cursor, year_digits, year) ||
        year_cursor != cursor ||
        !character(text, cursor, '-') ||
        !unsigned_field(text, cursor, 2, value.month) ||
        !character(text, cursor, '-') ||
        !unsigned_field(text, cursor, 2, value.day) ||
        year > maximum_year ||
        (year_digits == 6 && year <= 9999) ||
        (sign == '-' && year == 0)
    )
        return false;

    value.year = sign == '-' ? -static_cast<std::int32_t>(year) :
        static_cast<std::int32_t>(year);

    return true;
}

bool parse_time(const std::string& text, std::size_t& cursor, BrayTemporalDateTime& value)
{
    if (
        !unsigned_field(text, cursor, 2, value.hour) ||
        !character(text, cursor, ':') ||
        !unsigned_field(text, cursor, 2, value.minute) ||
        !character(text, cursor, ':') ||
        !unsigned_field(text, cursor, 2, value.second)
    )
        return false;

    value.nanosecond = 0;

    if (cursor != text.size() && text[cursor] == '.')
    {
        ++cursor;
        std::size_t digits = 0;

        while (cursor != text.size() && digit(text[cursor]) && digits != 9)
        {
            value.nanosecond = value.nanosecond * 10 +
                static_cast<std::uint32_t>(text[cursor] - '0');
            ++cursor;
            ++digits;
        }

        if (digits == 0 || (cursor != text.size() && digit(text[cursor])))
            return false;

        while (digits != 9)
        {
            value.nanosecond *= 10;
            ++digits;
        }
    }

    return true;
}

std::string padded(std::int64_t value, std::size_t width)
{
    auto text = std::to_string(value);

    if (text.size() < width)
        text.insert(0, width - text.size(), '0');

    return text;
}

std::string format_date(BrayTemporalDateTime value)
{
    const auto year = static_cast<std::int64_t>(value.year);
    const auto magnitude = year < 0 ? -year : year;
    const auto expanded = magnitude > 9999;

    return (year < 0 ? "-" : expanded ? "+" : "") +
        padded(magnitude, expanded ? 6 : 4) + "-" + padded(value.month, 2) + "-" +
        padded(value.day, 2);
}

std::string format_time(BrayTemporalDateTime value)
{
    auto result = padded(value.hour, 2) + ":" + padded(value.minute, 2) + ":" +
        padded(value.second, 2);

    if (value.nanosecond != 0)
    {
        auto fraction = padded(value.nanosecond, 9);

        while (fraction.back() == '0')
            fraction.pop_back();

        result += "." + fraction;
    }

    return result;
}

}

extern "C" std::uint32_t bray_temporal_parse(
    std::uint32_t kind,
    const std::uint8_t* text,
    std::uint64_t length,
    BrayTemporalValue* value,
    std::uint64_t* invalid_offset
)
{
    if (
        value == nullptr ||
        invalid_offset == nullptr ||
        (length != 0 && text == nullptr) ||
        length > std::numeric_limits<std::size_t>::max() ||
        kind > 3
    )
        return invalid_value;

    try
    {
        const auto source = text_value(text, length);
        std::size_t cursor = 0;
        BrayTemporalValue parsed{};
        bool valid = kind == 1 ? parse_time(source, cursor, parsed.local) :
            parse_date(source, cursor, parsed.local);

        if (valid && (kind == 2 || kind == 3))
            valid = character(source, cursor, 'T') && parse_time(source, cursor, parsed.local);

        if (valid && kind == 3)
        {
            std::int32_t offset = 0;

            if (cursor != source.size() && source[cursor] == 'Z')
            {
                ++cursor;
            }
            else
            {
                const bool negative = cursor != source.size() && source[cursor] == '-';

                if (
                    cursor == source.size() ||
                    (source[cursor] != '+' && source[cursor] != '-')
                )
                {
                    valid = false;
                }
                else
                {
                    ++cursor;
                    std::uint32_t hour = 0;
                    std::uint32_t minute = 0;

                    valid = unsigned_field(source, cursor, 2, hour) &&
                        character(source, cursor, ':') &&
                        unsigned_field(source, cursor, 2, minute) &&
                        hour <= 23 && minute <= 59;
                    offset = static_cast<std::int32_t>(hour * 3600 + minute * 60);

                    if (negative)
                        offset = -offset;
                }
            }

            parsed.offset_seconds = offset;

            if (valid)
            {
                try
                {
                    const auto instant = date::sys_time<std::chrono::nanoseconds>{
                        local_time(parsed.local).time_since_epoch() -
                        std::chrono::seconds{offset}
                    };

                    timestamp_parts(instant, parsed.timestamp_seconds, parsed.local.nanosecond);
                }
                catch (...)
                {
                    valid = false;
                }
            }
        }

        if (valid)
        {
            try
            {
                if (kind == 0)
                    static_cast<void>(
                        checked_date(parsed.local.year, parsed.local.month, parsed.local.day)
                    );
                else if (kind == 1)
                {
                    parsed.local.year = 1970;
                    parsed.local.month = 1;
                    parsed.local.day = 1;
                    static_cast<void>(local_time(parsed.local));
                }
                else
                    static_cast<void>(local_time(parsed.local));
            }
            catch (...)
            {
                valid = false;
            }
        }

        if (!valid || cursor != source.size())
        {
            *invalid_offset = static_cast<std::uint64_t>(cursor);
            return invalid_format;
        }

        *value = parsed;
        *invalid_offset = static_cast<std::uint64_t>(source.size());

        return success;
    }
    catch (...)
    {
        return provider_failure;
    }
}

extern "C" std::uint32_t bray_temporal_format(
    std::uint32_t kind,
    BrayTemporalValue value,
    std::uint8_t* destination,
    std::uint64_t capacity,
    std::uint64_t* written_or_required
)
{
    if (kind > 3)
        return invalid_value;

    try
    {
        std::string result;

        if (kind == 0)
        {
            static_cast<void>(checked_date(value.local.year, value.local.month, value.local.day));
            result = format_date(value.local);
        }
        else if (kind == 1)
        {
            static_cast<void>(local_time(value.local));
            result = format_time(value.local);
        }
        else if (kind == 2)
        {
            static_cast<void>(local_time(value.local));
            result = format_date(value.local) + "T" + format_time(value.local);
        }
        else
        {
            const auto instant = timestamp(value.timestamp_seconds, value.local.nanosecond);
            const auto local = date::local_time<std::chrono::nanoseconds>{
                instant.time_since_epoch() + std::chrono::seconds{value.offset_seconds}
            };
            const auto fields = civil_fields(local);

            result = format_date(fields) + "T" + format_time(fields);

            if (value.offset_seconds == 0)
            {
                result += "Z";
            }
            else
            {
                const auto magnitude = value.offset_seconds < 0 ?
                    -static_cast<std::int64_t>(value.offset_seconds) : value.offset_seconds;

                result += value.offset_seconds < 0 ? "-" : "+";
                result += padded(magnitude / 3600, 2) + ":" +
                    padded(magnitude % 3600 / 60, 2);
            }
        }

        return copy_text(result, destination, capacity, written_or_required);
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
