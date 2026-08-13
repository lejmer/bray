use std::io;

use bray_runtime_abi::NativePlatformStatus;

/// Maps a host I/O error to Bray's portable category and native detail.
pub fn platform_io_error(error: &io::Error) -> NativePlatformStatus {
    let category = match error.kind() {
        io::ErrorKind::Unsupported => 1,
        io::ErrorKind::PermissionDenied => 2,
        io::ErrorKind::NotFound => 3,
        io::ErrorKind::AlreadyExists => 4,
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => 5,
        io::ErrorKind::Interrupted => 6,
        io::ErrorKind::OutOfMemory => 7,
        io::ErrorKind::BrokenPipe | io::ErrorKind::UnexpectedEof => 8,
        io::ErrorKind::TimedOut => 9,
        _ => 12,
    };

    NativePlatformStatus::new(category, error.raw_os_error().map_or(0, i64::from))
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::platform_io_error;

    #[test]
    fn host_io_errors_map_to_closed_portable_categories() {
        let cases = [
            (io::ErrorKind::Unsupported, 1),
            (io::ErrorKind::PermissionDenied, 2),
            (io::ErrorKind::NotFound, 3),
            (io::ErrorKind::AlreadyExists, 4),
            (io::ErrorKind::InvalidData, 5),
            (io::ErrorKind::Interrupted, 6),
            (io::ErrorKind::OutOfMemory, 7),
            (io::ErrorKind::UnexpectedEof, 8),
            (io::ErrorKind::TimedOut, 9),
            (io::ErrorKind::Other, 12),
        ];

        for (kind, expected) in cases {
            let status = platform_io_error(&io::Error::from(kind));

            assert_eq!(status.category(), expected);
            assert_eq!(status.native_code(), 0);
        }
    }
}
