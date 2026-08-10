/// Fixed-layout proleptic Gregorian date and wall-time fields.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativePlatformDateTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
}

impl NativePlatformDateTime {
    /// Creates one fixed-layout civil date-time record.
    pub const fn new(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        nanosecond: u32,
    ) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanosecond,
        }
    }

    /// Returns the proleptic Gregorian year.
    pub const fn year(self) -> i32 {
        self.year
    }

    /// Returns the one-based month.
    pub const fn month(self) -> u32 {
        self.month
    }

    /// Returns the one-based day of month.
    pub const fn day(self) -> u32 {
        self.day
    }

    /// Returns the zero-based hour of day.
    pub const fn hour(self) -> u32 {
        self.hour
    }

    /// Returns the minute within the hour.
    pub const fn minute(self) -> u32 {
        self.minute
    }

    /// Returns the second within the minute.
    pub const fn second(self) -> u32 {
        self.second
    }

    /// Returns the nanosecond within the second.
    pub const fn nanosecond(self) -> u32 {
        self.nanosecond
    }
}

/// Fixed-layout result of observing one absolute timestamp.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativePlatformTemporalObservation {
    local: NativePlatformDateTime,
    offset_seconds: i32,
    daylight: u32,
}

impl NativePlatformTemporalObservation {
    /// Returns the observed local date and time.
    pub const fn local(self) -> NativePlatformDateTime {
        self.local
    }

    /// Returns the observed UTC offset in seconds.
    pub const fn offset_seconds(self) -> i32 {
        self.offset_seconds
    }

    /// Returns whether daylight-saving time applies.
    pub const fn is_daylight_saving(self) -> bool {
        self.daylight == 1
    }
}

/// Fixed-layout result of resolving one local date-time.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativePlatformTemporalResolution {
    kind: u32,
    reserved: u32,
    first_seconds: i64,
    first_nanoseconds: u32,
    first_reserved: u32,
    second_seconds: i64,
    second_nanoseconds: u32,
    second_reserved: u32,
}

impl NativePlatformTemporalResolution {
    /// Returns the encoded local-time resolution kind.
    pub const fn kind(self) -> u32 {
        self.kind
    }

    /// Returns the first candidate's whole Unix seconds.
    pub const fn first_seconds(self) -> i64 {
        self.first_seconds
    }

    /// Returns the first candidate's nanosecond remainder.
    pub const fn first_nanoseconds(self) -> u32 {
        self.first_nanoseconds
    }

    /// Returns the second candidate's whole Unix seconds.
    pub const fn second_seconds(self) -> i64 {
        self.second_seconds
    }

    /// Returns the second candidate's nanosecond remainder.
    pub const fn second_nanoseconds(self) -> u32 {
        self.second_nanoseconds
    }
}

/// Fixed-layout value used by strict temporal parsing and formatting.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NativePlatformTemporalValue {
    local: NativePlatformDateTime,
    timestamp_seconds: i64,
    offset_seconds: i32,
    reserved: u32,
}

impl NativePlatformTemporalValue {
    /// Returns the civil date-time component.
    pub const fn local(self) -> NativePlatformDateTime {
        self.local
    }

    /// Returns the absolute whole Unix seconds.
    pub const fn timestamp_seconds(self) -> i64 {
        self.timestamp_seconds
    }

    /// Returns the fixed UTC offset in seconds.
    pub const fn offset_seconds(self) -> i32 {
        self.offset_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativePlatformDateTime, NativePlatformTemporalObservation,
        NativePlatformTemporalResolution, NativePlatformTemporalValue,
    };

    #[test]
    fn temporal_records_have_the_native_abi_layout() {
        assert_abi_layout!(NativePlatformDateTime, size: 28, align: 4, fields: {
            year: 0,
            month: 4,
            day: 8,
            hour: 12,
            minute: 16,
            second: 20,
            nanosecond: 24,
        });

        assert_abi_layout!(NativePlatformTemporalObservation, size: 36, align: 4, fields: {
            local: 0,
            offset_seconds: 28,
            daylight: 32,
        });

        assert_abi_layout!(NativePlatformTemporalResolution, size: 40, align: 8, fields: {
            kind: 0,
            reserved: 4,
            first_seconds: 8,
            first_nanoseconds: 16,
            first_reserved: 20,
            second_seconds: 24,
            second_nanoseconds: 32,
            second_reserved: 36,
        });

        assert_abi_layout!(NativePlatformTemporalValue, size: 48, align: 8, fields: {
            local: 0,
            timestamp_seconds: 32,
            offset_seconds: 40,
            reserved: 44,
        });
    }
}
