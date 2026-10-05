// Independent C-layout mirror used to check the linked archive ABI.
#[repr(C)]
struct ResidentBinding {
    version: u32,
    services: u32,
    domain: usize,
    load_identity: usize,
    provider: [usize; 4],
}

#[repr(C)]
struct ResidentProductHost {
    version: u32,
    required_services: u32,
    identity: [u8; 32],
    // This empty descriptor never invokes the callback or observes a returned entry.
    static_entry: extern "C" fn(usize) -> !,
    static_count: usize,
    load_identity: std::sync::atomic::AtomicUsize,
    domain: usize,
    binding: ResidentBinding,
}

extern "C" fn no_product_static(_: usize) -> ! {
    std::process::abort()
}

// These foreign smoke hosts consume the reusable provider in a process-resident product.
#[unsafe(no_mangle)]
static bray_linked_product_host: ResidentProductHost = ResidentProductHost {
    version: 1,
    required_services: 0,
    identity: [0; 32],
    static_entry: no_product_static,
    static_count: 0,
    load_identity: std::sync::atomic::AtomicUsize::new(0),
    domain: 0,
    binding: ResidentBinding {
        version: 1,
        services: 0,
        domain: 0,
        load_identity: 0,
        provider: [0; 4],
    },
};

#[repr(C)]
struct PanicReport {
    source: [u32; 4],
    source_version: u64,
    source_package: [u8; 32],
    cause: u32,
    message: usize,
    message_length: usize,
    copy_message: Option<extern "C" fn(usize, usize, *mut u8, usize) -> u32>,
    release_message: Option<extern "C" fn(usize, usize, &mut RunOutcome)>,
    provider_context: usize,
    provider_retain: Option<extern "C" fn(usize)>,
    provider_release: Option<extern "C" fn(usize)>,
    provider_references: Option<extern "C" fn(usize) -> usize>,
    head: usize,
    tail: usize,
    count: usize,
    reserved: usize,
    consume: Option<extern "C" fn(&mut Self, bool) -> u32>,
}

impl PanicReport {
    const fn empty() -> Self {
        Self {
            source: [0; 4],
            source_version: 0,
            source_package: [0; 32],
            cause: 0,
            message: 0,
            message_length: 0,
            copy_message: None,
            release_message: None,
            provider_context: 0,
            provider_retain: None,
            provider_release: None,
            provider_references: None,
            head: 0,
            tail: 0,
            count: 0,
            reserved: 0,
            consume: None,
        }
    }
}
