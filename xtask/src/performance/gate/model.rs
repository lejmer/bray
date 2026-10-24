use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::model::{ComparisonReport, ReportIdentity};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    pub minimum_samples: usize,
    pub mad_multiplier: u64,
    pub resolution_multiple: u64,
    pub maximum_waiver_days: u64,
    pub structural: BTreeMap<String, BTreeMap<String, StructuralLimit>>,
    pub waivers: Vec<Waiver>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StructuralLimit {
    pub maximum_increase: u64,
    pub maximum_value: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Waiver {
    pub workload: String,
    pub metric: String,
    pub target: String,
    pub maximum_increase: u64,
    pub maximum_value: u64,
    pub rationale: String,
    pub owner: String,
    pub issue: String,
    pub approved_by: String,
    pub approval: String,
    pub approved_on: String,
    pub expires_on: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Checkpoint {
    pub workload: String,
    pub caller_and_callee: String,
    pub cleanup_and_report_release: String,
    pub ownership_or_failure_case: String,
    pub correctness_evidence: String,
    pub hypothesis: String,
    pub expected_improvement: String,
    pub stopping_condition: String,
    pub attribution: Attribution,
    pub attribution_evidence: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Attribution {
    WholeChangeOnly,
    GeneratedWork,
    RetainedRuntimeCode,
    RuntimeOperations,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    Pass,
    ReviewRequired,
    ConfirmedRegression,
}

#[derive(Debug, Serialize)]
pub(super) struct Decision {
    pub workload: String,
    pub metric: String,
    pub baseline: u64,
    pub candidate: u64,
    pub state: State,
    pub evidence: String,
    pub confirmation: Option<TimingEvidence>,
    pub timing: Option<TimingEvidence>,
    pub waiver: Option<Waiver>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct TimingEvidence {
    pub baseline: u64,
    pub candidate: u64,
    pub baseline_samples: usize,
    pub candidate_samples: usize,
    pub noise_bound: u64,
    pub resolution_bound: u64,
    pub resolved: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct GateReport {
    pub merge_base: String,
    pub candidate_revision: String,
    pub baseline_identity: ReportIdentity,
    pub candidate_identity: ReportIdentity,
    pub confirmation_identity: Option<ConfirmationIdentity>,
    pub state: State,
    pub blocking: bool,
    pub policy: Policy,
    pub checkpoint: Option<Checkpoint>,
    pub decisions: Vec<Decision>,
    pub active_waivers: Vec<Waiver>,
    pub comparison: ComparisonReport,
}

#[derive(Debug, Serialize)]
pub(super) struct ConfirmationIdentity {
    pub baseline: ReportIdentity,
    pub candidate: ReportIdentity,
}
