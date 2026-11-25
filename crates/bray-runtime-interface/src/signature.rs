/// Current native execution contract, including the inline panic context layout.
pub const EXECUTION_ABI_VERSION: crate::RuntimeAbiVersion = crate::RuntimeAbiVersion::new(1, 0);

/// Value kind in the native execution-runtime callable contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeAbiType {
    /// No result value.
    Void,
    /// The operation never returns to its caller.
    Never,
    /// Unsigned 8-bit scalar.
    U8,
    /// Unsigned 32-bit scalar.
    U32,
    /// Unsigned 64-bit scalar.
    U64,
    /// Signed 64-bit scalar.
    I64,
    /// Target-native unsigned address-sized scalar.
    Usize,
    /// Address of byte storage or a native callback.
    Pointer,
    /// Execution limits and optional native provider callbacks.
    Configuration,
    /// Root-start status and root handle.
    RootStart,
    /// Movable panic primary and detached outgoing ownership.
    PanicReport,
    /// Address of a panic report slot consumed or updated by the callee.
    PanicReportPointer,
    /// Terminal run state and payload.
    RunOutcome,
    /// Task allocation status and task handle.
    TaskAllocation,
    /// Inactive lease epoch and descriptor address.
    InactiveFrame,
    /// Frame progress kind, state and payload.
    FrameProgress,
    /// Frame affinity and required execution lanes.
    FrameState,
}

/// Native field order shared by panic report type realization and source binding checks.
pub const NATIVE_PANIC_REPORT_FIELDS: [RuntimeAbiType; 25] = [
    RuntimeAbiType::U32,
    RuntimeAbiType::U32,
    RuntimeAbiType::U32,
    RuntimeAbiType::U32,
    RuntimeAbiType::U64,
    RuntimeAbiType::U64,
    RuntimeAbiType::U64,
    RuntimeAbiType::U64,
    RuntimeAbiType::U64,
    RuntimeAbiType::U32,
    RuntimeAbiType::U32,
    RuntimeAbiType::U32,
    RuntimeAbiType::U32,
    RuntimeAbiType::I64,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Pointer,
    RuntimeAbiType::Pointer,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Pointer,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Pointer,
];

/// Native panic report field holding the primary panic cause.
pub const NATIVE_PANIC_REPORT_CAUSE: u32 = 9;
/// Native panic report field recording whether the primary panic is present.
pub const NATIVE_PANIC_REPORT_PRIMARY_PRESENT: u32 = 10;
/// Native panic report field holding the provider callback table.
pub const NATIVE_PANIC_REPORT_PROVIDER_CALLBACKS: u32 = 19;
/// Native panic report field counting incident records in the chain.
pub const NATIVE_PANIC_REPORT_INCIDENT_COUNT: u32 = 22;
/// Native panic report field holding unused reserved incident capacity.
pub const NATIVE_PANIC_REPORT_RESERVED: u32 = 23;
/// Native panic report field holding the report consumer.
pub const NATIVE_PANIC_REPORT_CONSUME: u32 = 24;

const _: () = {
    const fn field(index: u32) -> RuntimeAbiType {
        NATIVE_PANIC_REPORT_FIELDS[index as usize]
    }

    assert!(matches!(field(NATIVE_PANIC_REPORT_CAUSE), RuntimeAbiType::U32));
    assert!(matches!(field(NATIVE_PANIC_REPORT_PRIMARY_PRESENT), RuntimeAbiType::U32));
    assert!(matches!(field(NATIVE_PANIC_REPORT_PROVIDER_CALLBACKS), RuntimeAbiType::Pointer));
    assert!(matches!(field(NATIVE_PANIC_REPORT_INCIDENT_COUNT), RuntimeAbiType::Usize));
    assert!(matches!(field(NATIVE_PANIC_REPORT_RESERVED), RuntimeAbiType::Usize));
    assert!(matches!(field(NATIVE_PANIC_REPORT_CONSUME), RuntimeAbiType::Pointer));
    assert!(NATIVE_PANIC_REPORT_CONSUME as usize == NATIVE_PANIC_REPORT_FIELDS.len() - 1);
};

/// Exact native parameter and result kinds for an execution-runtime role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeNativeSignature {
    parameters: &'static [RuntimeAbiType],
    result: RuntimeAbiType,
}

impl RuntimeNativeSignature {
    pub(crate) const fn new(parameters: &'static [RuntimeAbiType], result: RuntimeAbiType) -> Self {
        // Catalog signatures are immutable compiler contracts, so malformed entries fail the build.
        let mut index = 0;

        while index < parameters.len() {
            assert!(
                !matches!(
                    parameters[index],
                    RuntimeAbiType::Void | RuntimeAbiType::Never
                ),
                "runtime parameter must have a value type"
            );

            index += 1;
        }

        Self { parameters, result }
    }

    /// Returns parameter kinds in native calling order.
    pub const fn parameters(self) -> &'static [RuntimeAbiType] {
        self.parameters
    }

    /// Returns the native result kind.
    pub const fn result(self) -> RuntimeAbiType {
        self.result
    }
}
