use super::super::model::PeerLanguage;

pub(super) const RUST_SOURCE: &str = include_str!("../../../fixtures/performance-peer.rs");
pub(super) const CPP_SOURCE: &str = include_str!("../../../fixtures/performance-peer.cpp");

pub(super) struct PeerSource {
    pub language: PeerLanguage,
    pub selector: &'static str,
    pub contents: &'static str,
}

pub(super) fn sources(workload: &str) -> Result<[PeerSource; 2], String> {
    let (rust_selector, cpp_selector) = match workload {
        "small_output" => ("small_output", "1"),
        "incremental_bytes_small" => ("incremental_bytes_small", "2"),
        "incremental_bytes" => ("incremental_bytes", "3"),
        "deque_mixed_ends" => ("deque_mixed_ends", "17"),
        "hash_map_growth_and_healthy_lookup" => ("hash_map_growth_and_healthy_lookup", "18"),
        "hash_map_collision_lookup" => ("hash_map_collision_lookup", "19"),
        "filesystem_metadata" => ("filesystem_metadata", "4"),
        "process_context" => ("process_context", "5"),
        "monotonic_clock" => ("monotonic_clock", "6"),
        "borrowed_text" => ("borrowed_text", "7"),
        "format_numbers" => ("format_numbers", "8"),
        "format_large_width" => ("format_large_width", "12"),
        "format_writer" => ("format_writer", "13"),
        "stream_output" => ("stream_output", "9"),
        "async_output" => ("async_output", "10"),
        "contended_output" => ("contended_output", "14"),
        "captured_output" => ("captured_output", "15"),
        "process_pipe_transfer" => ("process_pipe_transfer", "16"),
        "file_output" => ("file_output", "11"),
        _ => {
            return Err(format!(
                "workload {workload} has no Rust and C++ peer sources"
            ));
        }
    };

    Ok([
        PeerSource {
            language: PeerLanguage::Rust,
            selector: rust_selector,
            contents: RUST_SOURCE,
        },
        PeerSource {
            language: PeerLanguage::Cpp,
            selector: cpp_selector,
            contents: CPP_SOURCE,
        },
    ])
}

pub(in crate::performance) fn corpus_contract(workload: &str) -> String {
    let comparison_contract = comparison_contract(workload).unwrap_or("missing");
    let mut contract = format!("contract\0{comparison_contract}\0");

    if let Ok(sources) = sources(workload) {
        for source in sources {
            contract.push_str(match source.language {
                PeerLanguage::Rust => "rust\0",
                PeerLanguage::Cpp => "cpp\0",
            });

            contract.push_str(source.selector);
            contract.push('\0');
            contract.push_str(source.contents);
            contract.push('\0');
        }
    }

    contract
}

pub(in crate::performance) fn comparison_contract(workload: &str) -> Option<&'static str> {
    match workload {
        "small_output" => Some("start and complete an empty program once"),
        "incremental_bytes_small" => Some(
            "successfully append 64 bytes with value 65 to an initially empty growable byte sequence and validate its final length",
        ),
        "incremental_bytes" => Some(
            "successfully append 4096 bytes with value 65 to an initially empty growable byte sequence and validate its final length",
        ),
        "deque_mixed_ends" => Some(
            "fill a 64-element double-ended sequence, remove 32 front elements, wrap by appending 32 elements, perform 4096 alternating end operations without changing length, grow once, and validate final end values",
        ),
        "hash_map_growth_and_healthy_lookup" => Some(
            "insert 4096 distinct integer key-value pairs into an initially empty hash map, successfully look up every key under healthy hashing, and validate the final length",
        ),
        "hash_map_collision_lookup" => Some(
            "insert 48 distinct integer key-value pairs into a hash map whose hasher maps every key to one collision chain, complete 4096 successful lookups across that chain, and validate the final length",
        ),
        "filesystem_metadata" => {
            Some("read metadata successfully for the same existing path 256 times")
        }
        "process_context" => Some("read the current process identity 1024 times"),
        "monotonic_clock" => Some("read a monotonic clock successfully 1024 times"),
        "borrowed_text" => Some(
            "slice UTF-8 text, construct owned UTF-8 text, compare and hash text, parse decimal 42, append borrowed text 256 times, and append quoted text",
        ),
        "format_numbers" => Some(
            "append the decimal representation of every integer from 0 through 1023 to one growable byte sequence and validate its final length",
        ),
        "format_large_width" => Some(
            "append decimal unsigned integer 42 right-aligned to width 130 exactly 1024 times and validate every output byte",
        ),
        "format_writer" => Some(
            "format decimal unsigned integer 42 directly to a validating byte writer exactly 1024 times and validate every output byte",
        ),
        "stream_output" => Some(
            "complete 1024 standard-output calls that each write one byte and flush before returning",
        ),
        "async_output" => Some(
            "complete and await 128 sequential asynchronous standard-output calls that each write one byte and flush before returning",
        ),
        "contended_output" => Some(
            "start two concurrent tasks that each complete 64 standard-output operations that write one byte and flush before returning, then join both tasks",
        ),
        "captured_output" => Some(
            "write one borrowed 4096-byte range into a preallocated captured-output sink and validate its final length",
        ),
        "process_pipe_transfer" => Some(
            "start one child copy of the workload, write one borrowed 4096-byte range to its piped standard input, close the pipe, and join the child successfully",
        ),
        "file_output" => Some(
            "create a new file, write all 4096 bytes while accepting partial writes, flush, close, and remove the file",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{comparison_contract, sources};

    #[test]
    fn every_workload_has_a_contract_and_both_peer_sources() {
        for workload in super::super::super::corpus::WORKLOADS {
            assert!(
                comparison_contract(workload.id).is_some(),
                "{}",
                workload.id
            );

            assert!(sources(workload.id).is_ok(), "{}", workload.id);
        }
    }
}
