use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct NativeTestReport {
    pub(crate) format: u32,
    pub(crate) build: NativeTestBuildProvenance,
    pub(crate) selection: NativeSelection,
    pub(crate) products: Vec<NativeProductReport>,
    pub(crate) summary: NativeSummary,
    pub(crate) duration_nanoseconds: Option<u64>,
}

#[derive(Deserialize)]
pub(crate) struct NativeTestBuildProvenance {
    pub(crate) reused: bool,
    pub(crate) compilation: bool,
    pub(crate) emission: bool,
    pub(crate) linking: bool,
    pub(crate) products: Vec<NativeTestProductGeneration>,
}

#[derive(Deserialize)]
pub(crate) struct NativeTestProductGeneration {
    pub(crate) product: String,
    pub(crate) generation: String,
}

#[derive(Deserialize)]
pub(crate) struct NativeSelection {
    pub(crate) discovered: usize,
    pub(crate) selected: usize,
    pub(crate) filtered_out: usize,
}

#[derive(Deserialize)]
pub(crate) struct NativeSummary {
    pub(crate) passed: usize,
    pub(crate) failed: usize,
}

#[derive(Deserialize)]
pub(crate) struct NativeProductReport {
    pub(crate) package: String,
    pub(crate) product: String,
    pub(crate) catalog_digest: String,
    pub(crate) tests: Vec<NativeTestResult>,
}

#[derive(Deserialize)]
pub(crate) struct NativeTestResult {
    pub(crate) identity: String,
    pub(crate) outcome: NativeOutcome,
    pub(crate) stdout: NativeStream,
    pub(crate) stderr: NativeStream,
    pub(crate) duration_nanoseconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum NativeOutcome {
    Passed,
    ReturnedError {
        error_type: String,
        formatted_value: Option<String>,
    },
    ExplicitFailure {
        source: NativeSourceAnchor,
        message: String,
    },
    AssertionFailure {
        source: NativeSourceAnchor,
        message: Option<String>,
    },
    Panicked {
        cause: String,
        source: Option<NativeSourceAnchor>,
        message: String,
    },
    TimedOut {
        nanoseconds: u64,
    },
    Cancelled {
        #[serde(rename = "source")]
        _source: String,
    },
    InfrastructureFailed {
        failure: String,
        detail_code: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) struct NativeSourceAnchor {
    pub(crate) source: u32,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) version: u64,
}

impl NativeSourceAnchor {
    pub(crate) const fn is_valid(self) -> bool {
        self.source == 0 && self.start < self.end && self.version == 0
    }
}

impl NativeOutcome {
    pub(crate) const fn kind(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::ReturnedError { .. } => "returned_error",
            Self::ExplicitFailure { .. } => "explicit_failure",
            Self::AssertionFailure { .. } => "assertion_failure",
            Self::Panicked { .. } => "panicked",
            Self::TimedOut { .. } => "timed_out",
            Self::Cancelled { .. } => "cancelled",
            Self::InfrastructureFailed { .. } => "infrastructure_failed",
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct NativeStream {
    pub(crate) policy: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) truncated: bool,
    pub(crate) discarded_byte_count: u64,
    pub(crate) failure: Option<serde_json::Value>,
}
