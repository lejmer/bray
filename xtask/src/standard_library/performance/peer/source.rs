use super::super::model::PeerLanguage;

pub(super) const RUST_SOURCE: &str = include_str!("../../../../fixtures/performance-peer.rs");
pub(super) const CPP_SOURCE: &str = include_str!("../../../../fixtures/performance-peer.cpp");

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
        "filesystem_metadata" => ("filesystem_metadata", "4"),
        "process_context" => ("process_context", "5"),
        "monotonic_clock" => ("monotonic_clock", "6"),
        "borrowed_text" => ("borrowed_text", "7"),
        "format_numbers" => ("format_numbers", "8"),
        "stream_output" => ("stream_output", "9"),
        "async_output" => ("async_output", "10"),
        "file_output" => ("file_output", "11"),
        _ => return Err(format!("workload {workload} has no Rust and C++ peer sources")),
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

pub(in crate::standard_library::performance) fn corpus_contract(workload: &str) -> String {
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

pub(in crate::standard_library::performance) fn comparison_contract(
    workload: &str,
) -> Option<&'static str> {
    match workload {
        "small_output" => Some("start and complete an empty program once"),
        "incremental_bytes_small" => Some(
            "successfully append 64 bytes with value 65 to an initially empty growable byte sequence and validate its final length",
        ),
        "incremental_bytes" => Some(
            "successfully append 4096 bytes with value 65 to an initially empty growable byte sequence and validate its final length",
        ),
        "filesystem_metadata" => Some(
            "read metadata successfully for the same existing path 256 times",
        ),
        "process_context" => Some("read the current process identity 1024 times"),
        "monotonic_clock" => Some("read a monotonic clock successfully 1024 times"),
        "borrowed_text" => Some(
            "slice UTF-8 text, construct owned UTF-8 text, compare and hash text, parse decimal 42, append borrowed text 256 times, and append quoted text",
        ),
        "format_numbers" => Some(
            "append the decimal representation of every integer from 0 through 1023 to one growable byte sequence and validate its final length",
        ),
        "stream_output" => Some(
            "complete 1024 standard-output calls that each write one byte and flush before returning",
        ),
        "async_output" => Some(
            "complete and await 128 sequential asynchronous standard-output calls that each write one byte and flush before returning",
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
            assert!(comparison_contract(workload.id).is_some(), "{}", workload.id);
            assert!(sources(workload.id).is_ok(), "{}", workload.id);
        }
    }
}
