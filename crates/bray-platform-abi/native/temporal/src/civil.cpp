#include "provider.h"

using namespace bray::temporal;

extern "C" std::uint32_t bray_temporal_date_validate(
    std::int32_t year,
    std::uint32_t month,
    std::uint32_t day
)
{
    try
    {
        static_cast<void>(checked_date(year, month, day));
        return success;
    }
    catch (const std::out_of_range&)
    {
        return out_of_range;
    }
    catch (const std::invalid_argument&)
    {
        return invalid_value;
    }
    catch (...)
    {
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
)
{
    if (result == nullptr || adjustment > 1)
        return invalid_value;

    try
    {
        const auto original = checked_date(value.year, value.month, value.day);
        const auto month_index = static_cast<std::int64_t>(value.year) * 12 +
            static_cast<std::int64_t>(value.month - 1) +
            static_cast<std::int64_t>(years) * 12 + months;
        auto shifted_year = month_index / 12;
        auto shifted_month = month_index % 12;

        if (shifted_month < 0)
        {
            shifted_month += 12;
            --shifted_year;
        }

        if (shifted_year < minimum_year || shifted_year > maximum_year)
            return out_of_range;

        auto date = date::year{static_cast<std::int32_t>(shifted_year)} /
            date::month{static_cast<std::uint32_t>(shifted_month + 1)} / original.day();

        if (!date.ok())
        {
            if (adjustment == 0)
                return invalid_value;

            date = date::year_month_day{
                date::sys_days{date.year() / date.month() / date::last}
            };
        }

        const auto day_number = static_cast<std::int64_t>(
            date::sys_days{date}.time_since_epoch().count()
        ) + days;
        const auto minimum_day = date::sys_days{
            date::year{minimum_year} / date::January / date::day{1}
        }.time_since_epoch().count();
        const auto maximum_day = date::sys_days{
            date::year{maximum_year} / date::December / date::day{31}
        }.time_since_epoch().count();

        if (day_number < minimum_day || day_number > maximum_day)
            return out_of_range;

        date = date::year_month_day{date::sys_days{date::days{day_number}}};

        if (!date.ok())
            return out_of_range;

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
