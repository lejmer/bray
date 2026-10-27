#[cfg(any(
    peer_workload = "format_real_shortest",
    peer_workload = "format_real_precision",
    peer_workload = "format_real_large_width"
))]
struct RealLocal {
    bytes: [u8; 128],
    length: usize,
}

#[cfg(any(
    peer_workload = "format_real_shortest",
    peer_workload = "format_real_precision",
    peer_workload = "format_real_large_width"
))]
impl std::fmt::Write for RealLocal {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let end = self.length + value.len();

        self.bytes[self.length..end].copy_from_slice(value.as_bytes());
        self.length = end;

        Ok(())
    }
}

#[cfg(peer_workload = "format_real_shortest")]
fn workload() -> bool {
    use std::fmt::Write as _;

    let mut total = 0;

    for index in 0..1_024 {
        let mut sink = RealLocal {
            bytes: [0; 128],
            length: 0,
        };
        let value = std::hint::black_box(f64::from(index) + 0.125);

        write!(&mut sink, "{value}").expect("bounded real formatting");
        std::hint::black_box(&sink.bytes[..sink.length]);
        total += sink.length;
    }

    total == 7_082
}

#[cfg(peer_workload = "format_real_precision")]
fn workload() -> bool {
    use std::fmt::Write as _;

    let mut total = 0;

    for index in 0..1_024 {
        let mut sink = RealLocal {
            bytes: [0; 128],
            length: 0,
        };
        let precision = if index % 2 == 0 { 40 } else { 80 };
        let value = std::hint::black_box(0.1_f64);

        write!(&mut sink, "{value:.precision$}").expect("bounded precision formatting");
        std::hint::black_box(&sink.bytes[..sink.length]);
        total += sink.length;
    }

    total == 63_488
}

#[cfg(peer_workload = "format_real_large_width")]
fn workload() -> bool {
    use std::fmt::Write as _;

    let mut total = 0;

    for _ in 0..1_024 {
        let mut sink = RealLocal {
            bytes: [0; 128],
            length: 0,
        };
        let value = std::hint::black_box(1.5_f64);

        write!(&mut sink, "{value:.3}").expect("bounded precision formatting");
        std::hint::black_box(&sink.bytes[..sink.length]);

        // The destination counts a repeated padding range in one operation.
        total += sink.length.max(1_000_000);
    }

    total == 1_024_000_000
}

#[cfg(peer_workload = "format_real_collection")]
fn workload() -> bool {
    use std::io::Write as _;

    let mut sink = Vec::with_capacity(7_082);

    for index in 0..1_024 {
        let value = std::hint::black_box(f64::from(index) + 0.125);

        write!(&mut sink, "{value}").expect("collection formatting");
    }

    std::hint::black_box(&sink);

    sink.len() == 7_082
}

#[cfg(peer_workload = "format_complex_writer")]
struct RealValidatingWriter {
    length: usize,
    valid: bool,
}

#[cfg(peer_workload = "format_complex_writer")]
impl std::fmt::Write for RealValidatingWriter {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        for byte in value.bytes() {
            self.valid &= byte == b"1.25-2.50i"[self.length % 10];
            self.length += 1;
        }

        Ok(())
    }
}

#[cfg(peer_workload = "format_complex_writer")]
fn workload() -> bool {
    use std::fmt::Write as _;

    let mut sink = RealValidatingWriter {
        length: 0,
        valid: true,
    };

    for _ in 0..1_024 {
        let real = std::hint::black_box(1.25_f64);
        let imaginary = std::hint::black_box(2.5_f64);

        write!(&mut sink, "{real:.2}-{imaginary:.2}i").expect("streaming complex formatting");
    }

    sink.valid && sink.length == 10_240
}
