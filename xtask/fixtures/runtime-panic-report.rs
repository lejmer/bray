// Independent C-layout mirror used to check the linked archive ABI.
#[repr(C)]
struct PanicReport {
    source: [u32; 4],
    source_version: u64,
    source_package: [u8; 32],
    cause: u32,
    primary_present: u32,
    message: usize,
    message_length: usize,
    copy_message: Option<extern "C" fn(usize, usize, *mut u8, usize) -> u32>,
    release_message: Option<extern "C" fn(usize, usize, &mut RunOutcome)>,
    provider_context: usize,
    provider_callbacks: usize,
    head: usize,
    tail: usize,
    count: usize,
    reserved: usize,
    consume: Option<extern "C" fn(&mut Self, u32) -> u32>,
}

impl PanicReport {
    const fn empty() -> Self {
        Self {
            source: [0; 4],
            source_version: 0,
            source_package: [0; 32],
            cause: 0,
            primary_present: 0,
            message: 0,
            message_length: 0,
            copy_message: None,
            release_message: None,
            provider_context: 0,
            provider_callbacks: 0,
            head: 0,
            tail: 0,
            count: 0,
            reserved: 0,
            consume: None,
        }
    }
}
