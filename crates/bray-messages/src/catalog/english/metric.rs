use std::time::Duration;

use crate::units::{ByteUnit, DurationUnit, scaled_bytes, scaled_duration};

pub(crate) fn duration(duration: Duration) -> String {
    let (whole, fraction, unit) = scaled_duration(duration);

    let unit = match unit {
        DurationUnit::Nanoseconds => "ns",
        DurationUnit::Microseconds => "us",
        DurationUnit::Milliseconds => "ms",
        DurationUnit::Seconds => "s",
        DurationUnit::Minutes => "min",
    };

    if fraction == 0 {
        return format!("{whole} {unit}");
    }

    let fraction = format!("{fraction:03}");
    let fraction = fraction.trim_end_matches('0');

    format!("{whole}.{fraction} {unit}")
}

pub(super) fn bytes(value: u64) -> String {
    let (scaled, unit) = scaled_bytes(value);

    match unit {
        ByteUnit::Bytes => format!("{} B", grouped(value)),
        ByteUnit::Kibibytes => format!("{scaled:.2} KiB"),
        ByteUnit::Mebibytes => format!("{scaled:.2} MiB"),
    }
}

pub(super) fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, byte) in digits.bytes().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }

        output.push(char::from(byte));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::bytes;

    #[test]
    fn byte_units_render_with_english_labels_and_precision() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1_023), "1,023 B");
        assert_eq!(bytes(1_024), "1.00 KiB");
        assert_eq!(bytes(1_536), "1.50 KiB");
        assert_eq!(bytes(1_048_575), "1024.00 KiB");
        assert_eq!(bytes(1_048_576), "1.00 MiB");
        assert_eq!(bytes(1_572_864), "1.50 MiB");
    }
}
