pub(super) fn milliseconds(nanoseconds: u64) -> String {
    let milliseconds = nanoseconds as f64 / 1_000_000.0;

    if milliseconds < 0.001 {
        format!("{milliseconds:.6} ms")
    } else {
        format!("{milliseconds:.3} ms")
    }
}

pub(super) fn picoseconds_milliseconds(picoseconds: u64) -> String {
    let milliseconds = picoseconds as f64 / 1_000_000_000.0;

    if milliseconds < 0.001 {
        format!("{milliseconds:.9} ms")
    } else {
        format!("{milliseconds:.3} ms")
    }
}

pub(super) fn signed_milliseconds(nanoseconds: i128) -> String {
    let milliseconds = nanoseconds as f64 / 1_000_000.0;

    if milliseconds.abs() < 0.001 {
        format!("{milliseconds:+.6} ms")
    } else {
        format!("{milliseconds:+.3} ms")
    }
}

pub(super) fn kibibytes(bytes: u64) -> String {
    let unit = if bytes == 1 { "byte" } else { "bytes" };

    format!(
        "{:.2} KiB ({} {unit})",
        bytes as f64 / 1024.0,
        grouped(bytes)
    )
}

pub(super) fn signed_kibibytes(bytes: i128) -> String {
    let unit = if bytes.unsigned_abs() == 1 {
        "byte"
    } else {
        "bytes"
    };

    format!("{:+.2} KiB ({bytes:+} {unit})", bytes as f64 / 1024.0)
}

pub(super) fn nanoseconds_title(nanoseconds: u64) -> String {
    format!("title=\"{} ns\"", grouped(nanoseconds))
}

pub(super) fn signed_picoseconds_milliseconds(picoseconds: i128) -> String {
    let milliseconds = picoseconds as f64 / 1_000_000_000.0;

    if milliseconds.abs() < 0.001 {
        format!("{milliseconds:+.9} ms")
    } else {
        format!("{milliseconds:+.3} ms")
    }
}

pub(super) fn picoseconds_title(picoseconds: u64) -> String {
    format!("title=\"{} ps\"", grouped(picoseconds))
}

pub(super) fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }

        output.push(character);
    }

    output
}
