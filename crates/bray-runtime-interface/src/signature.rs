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
    /// Target-native unsigned address-sized scalar.
    Usize,
    /// Address of byte storage or a native callback.
    Pointer,
    /// Worker and timer capacities.
    Configuration,
    /// Root-start status and root handle.
    RootStart,
    /// Movable panic primary and detached outgoing ownership. Parameters transfer through a pointer.
    PanicReport,
    /// Terminal run state and payload.
    RunOutcome,
    /// Task allocation status and task handle.
    TaskAllocation,
    /// Inactive frame storage and descriptor addresses.
    InactiveFrame,
    /// Frame progress kind, state and payload.
    FrameProgress,
    /// Lane-selection status and lane identity.
    LaneResult,
    /// Loaded-product lifecycle state and outstanding obligations.
    ProductObservation,
}

/// Native field order shared by panic report type realization and source binding checks.
pub const NATIVE_PANIC_REPORT_FIELDS: [RuntimeAbiType; 19] = [
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
    RuntimeAbiType::Usize,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Pointer,
    RuntimeAbiType::Pointer,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Usize,
    RuntimeAbiType::Pointer,
];

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
