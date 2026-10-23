use std::collections::BTreeSet;
use std::io::Write as _;
use std::process::{Command, Stdio};

use serde::Deserialize;

use super::model::{GateReport, State, Waiver};

pub(super) fn apply(
    report: &mut GateReport,
    today: u64,
    mut issue_is_open: impl FnMut(&str) -> Result<bool, String>,
) -> Result<(), String> {
    let mut scopes = BTreeSet::new();

    for waiver in &report.policy.waivers {
        validate(waiver, today, report.policy.maximum_waiver_days)?;

        if !scopes.insert((&waiver.workload, &waiver.metric, &waiver.target)) {
            return Err(
                "performance waivers repeat a workload, metric, and target scope".to_owned(),
            );
        }

        let known_scope = report
            .policy
            .structural
            .get(&waiver.workload)
            .is_some_and(|metrics| metrics.contains_key(&waiver.metric))
            || (super::super::corpus::WORKLOADS
                .iter()
                .any(|workload| workload.id == waiver.workload)
                && matches!(
                    waiver.metric.as_str(),
                    "controlled_execution.picoseconds" | "process_execution.picoseconds"
                ));

        if !known_scope {
            return Err(format!(
                "waiver has an unknown or broadened scope: {} / {}",
                waiver.workload, waiver.metric
            ));
        }

        if !issue_is_open(&waiver.issue)? {
            return Err(format!(
                "waiver tracking issue is not open: {}",
                waiver.issue
            ));
        }

        for decision in &mut report.decisions {
            if decision.workload == waiver.workload
                && decision.metric == waiver.metric
                && report.candidate_identity.target == waiver.target
            {
                if decision.candidate > waiver.maximum_value
                    || decision.candidate.saturating_sub(decision.baseline)
                        > waiver.maximum_increase
                    || decision.confirmation.is_some_and(|evidence| {
                        evidence.candidate > waiver.maximum_value
                            || evidence.candidate.saturating_sub(evidence.baseline)
                                > waiver.maximum_increase
                    })
                {
                    return Err(format!(
                        "measurement exceeds waiver bounds: {} / {}",
                        waiver.workload, waiver.metric
                    ));
                }

                decision.waiver = Some(waiver.clone());
            }
        }

        report.active_waivers.push(waiver.clone());
    }

    report.blocking =
        report.decisions.iter().any(|decision| {
            decision.state == State::ConfirmedRegression && decision.waiver.is_none()
        }) || !report.comparison.baseline_conformance_failures.is_empty()
            || !report.comparison.candidate_conformance_failures.is_empty();

    Ok(())
}

fn validate(waiver: &Waiver, today: u64, maximum_days: u64) -> Result<(), String> {
    if [
        &waiver.workload,
        &waiver.metric,
        &waiver.target,
        &waiver.rationale,
        &waiver.owner,
        &waiver.approved_by,
        &waiver.approval,
    ]
    .iter()
    .any(|text| text.trim().is_empty() || text.len() > 4096)
        || !waiver
            .approval
            .strip_prefix("https://github.com/lejmer/bray/pull/")
            .and_then(|suffix| suffix.split(['/', '#', '?']).next())
            .is_some_and(|number| number.parse::<u64>().is_ok_and(|number| number > 0))
        || bray_target::TargetIdentity::try_new(waiver.target.as_str())
            .and_then(|identity| bray_target::NativeTarget::for_identity(&identity))
            .is_none()
    {
        return Err("waiver requires an exact scope, owner, rationale, approver, and repository PR approval URL".to_owned());
    }

    issue_identifier(&waiver.issue)?;

    let approved = date(&waiver.approved_on)?;
    let expiry = date(&waiver.expires_on)?;

    if approved > today || expiry <= today || expiry <= approved || expiry - approved > maximum_days
    {
        return Err(format!(
            "waiver is expired or outside its approved time limit: {}",
            waiver.expires_on
        ));
    }

    Ok(())
}

pub(super) fn today() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() / 86_400)
        .map_err(|error| format!("waiver clock precedes Unix epoch: {error}"))
}

pub(super) fn date(value: &str) -> Result<u64, String> {
    let invalid = || format!("invalid waiver date, expected YYYY-MM-DD: {value}");

    if value.len() != 10
        || !value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 4 | 7) {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return Err(invalid());
    }

    let year: u64 = value[..4].parse().map_err(|_| invalid())?;
    let month: usize = value[5..7].parse().map_err(|_| invalid())?;
    let day: u64 = value[8..].parse().map_err(|_| invalid())?;

    let leap = |year: u64| {
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
    };

    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day == 0
        || day > months[month - 1]
    {
        return Err(invalid());
    }

    Ok((1970..year)
        .map(|year| if leap(year) { 366 } else { 365 })
        .sum::<u64>()
        + months[..month - 1].iter().sum::<u64>()
        + day
        - 1)
}

fn issue_identifier(url: &str) -> Result<&str, String> {
    let identifier = url
        .strip_prefix("https://linear.app/bray-lang/issue/")
        .and_then(|path| path.split('/').next())
        .ok_or_else(|| "waiver requires a Bray Linear issue URL".to_owned())?;

    if !identifier.strip_prefix("BRA-").is_some_and(|number| {
        !number.is_empty()
            && number.bytes().all(|byte| byte.is_ascii_digit())
            && !number.starts_with('0')
    }) {
        return Err("waiver requires an exact Bray Linear issue identifier".to_owned());
    }

    Ok(identifier)
}

pub(super) fn live_issue_is_open(url: &str) -> Result<bool, String> {
    let identifier = issue_identifier(url)?;

    let token = std::env::var("LINEAR_API_KEY").map_err(|_| {
        "active waivers require LINEAR_API_KEY for live issue-state validation".to_owned()
    })?;

    if token.is_empty() || token.bytes().any(|byte| byte.is_ascii_control()) {
        return Err("LINEAR_API_KEY must be nonempty and contain no control characters".to_owned());
    }

    let query = serde_json::json!({
        "query": "query($id: String!) { issue(id: $id) { identifier archivedAt state { type } } }",
        "variables": { "id": identifier }
    });

    let mut child = Command::new("curl")
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "30",
            "--max-filesize",
            "1048576",
            "--config",
            "-",
            "--url",
            "https://api.linear.app/graphql",
            "--header",
            "Content-Type: application/json",
            "--data-binary",
            &query.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start live Linear waiver check: {error}"))?;

    let escaped = token.replace('\\', "\\\\").replace('"', "\\\"");

    let write_result = child
        .stdin
        .take()
        .expect("piped curl configuration")
        .write_all(format!("header = \"Authorization: {escaped}\"\n").as_bytes());

    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not complete live Linear waiver check: {error}"))?;

    write_result.map_err(|error| format!("could not supply Linear authorization: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "live Linear waiver check failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    issue_response_is_open(identifier, &output.stdout)
}

pub(super) fn issue_response_is_open(identifier: &str, bytes: &[u8]) -> Result<bool, String> {
    #[derive(Deserialize)]
    struct Response {
        data: Option<Data>,
        errors: Option<serde_json::Value>,
    }

    #[derive(Deserialize)]
    struct Data {
        issue: Option<Issue>,
    }

    #[derive(Deserialize)]
    struct Issue {
        identifier: String,
        #[serde(rename = "archivedAt")]
        #[serde(deserialize_with = "Option::deserialize")]
        archived_at: Option<String>,
        state: IssueState,
    }

    #[derive(Deserialize)]
    struct IssueState {
        r#type: String,
    }

    let response: Response = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid live Linear issue response: {error}"))?;

    if response.errors.is_some() {
        return Err("Linear rejected live waiver issue validation".to_owned());
    }

    let issue = response
        .data
        .and_then(|data| data.issue)
        .ok_or_else(|| "Linear waiver tracking issue is missing or inaccessible".to_owned())?;

    Ok(issue.identifier == identifier
        && issue.archived_at.is_none()
        && matches!(
            issue.state.r#type.as_str(),
            "triage" | "backlog" | "unstarted" | "started"
        ))
}
