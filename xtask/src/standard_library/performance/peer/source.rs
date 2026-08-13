use super::super::model::PeerLanguage;

pub(super) const RUST_SOURCE: &str = include_str!("../../../../fixtures/performance-peer.rs");
pub(super) const CPP_SOURCE: &str = include_str!("../../../../fixtures/performance-peer.cpp");

pub(super) struct PeerSource {
    pub language: PeerLanguage,
    pub selector: &'static str,
    pub contents: &'static str,
}

pub(super) fn sources(workload: &str) -> Result<[PeerSource; 2], &'static str> {
    let (rust_selector, cpp_selector) = match workload {
        "small_output" => ("small_output", "1"),
        "incremental_bytes_small" => ("incremental_bytes_small", "2"),
        "incremental_bytes" => ("incremental_bytes", "3"),
        "filesystem_metadata" => ("filesystem_metadata", "4"),
        "process_context" => ("process_context", "5"),
        "monotonic_clock" => ("monotonic_clock", "6"),
        "borrowed_text" => {
            return Err("the Bray text pipeline has no matching standard Rust or C++ hash and escaping contract");
        }
        "format_numbers" => {
            return Err("the Bray formatting options have no matching standard Rust or C++ formatting contract");
        }
        "stream_output" => {
            return Err("standard stream buffering and locking semantics differ across the language runtimes");
        }
        "async_output" => {
            return Err("Rust and C++ standard libraries do not provide the same structured asynchronous output contract");
        }
        "file_output" => {
            return Err("portable Rust and C++ file APIs do not share Bray partial-write and flush semantics");
        }
        _ => return Err("the workload has no matched Rust and C++ peer contract"),
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
    let comparison_contract = comparison_contract(workload).unwrap_or("unsupported");
    let mut contract = format!("contract\0{comparison_contract}\0");

    match sources(workload) {
        Ok(sources) => {
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

            contract
        }
        Err(reason) => {
            contract.push_str("unsupported\0");
            contract.push_str(reason);
            contract.push('\0');

            contract
        }
    }
}

pub(in crate::standard_library::performance) fn comparison_contract(
    workload: &str,
) -> Option<&'static str> {
    match workload {
        "small_output" => Some("start and complete an empty program once"),
        "incremental_bytes_small" => Some(
            "append 64 bytes with value 65 to an initially empty growable byte sequence and validate its final length",
        ),
        "incremental_bytes" => Some(
            "append 4096 bytes with value 65 to an initially empty growable byte sequence and validate its final length",
        ),
        "filesystem_metadata" => Some(
            "read metadata successfully for the same existing path 256 times",
        ),
        "process_context" => Some("read the current process identity 1024 times"),
        "monotonic_clock" => Some("read a monotonic clock successfully 1024 times"),
        _ => None,
    }
}

pub(in crate::standard_library::performance) fn support_reason(workload: &str) -> Option<&'static str> {
    sources(workload).err()
}

#[cfg(test)]
mod tests {
    use super::{comparison_contract, support_reason};

    #[test]
    fn every_workload_has_exactly_one_peer_support_outcome() {
        for workload in super::super::super::corpus::WORKLOADS {
            assert_ne!(
                comparison_contract(workload.id).is_some(),
                support_reason(workload.id).is_some(),
                "{} must be either matched or unsupported",
                workload.id,
            );
        }
    }
}
