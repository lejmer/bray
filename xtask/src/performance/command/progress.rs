use std::path::Path;
use std::time::Duration;

pub(super) fn phase(message: &str) {
    crate::progress::message(message);
}

pub(super) fn plan(workloads: usize, warmups: u32, samples: u32) {
    crate::progress::message(&plan_message(workloads, warmups, samples));
}

pub(super) fn workload(index: usize, total: usize, identity: &str) {
    crate::progress::item(index, total, identity);
}

pub(super) fn workload_phase(message: &str) {
    crate::progress::message(&format!("  {message}"));
}

pub(super) fn report(kind: &str, path: &Path) {
    crate::progress::message(&format!("{kind}: {}", path.display()));
}

pub(super) fn finished(elapsed: Duration) {
    crate::progress::message(&completion_message(elapsed));
}

fn plan_message(workloads: usize, warmups: u32, samples: u32) -> String {
    let workload = if workloads == 1 {
        "workload"
    } else {
        "workloads"
    };

    let warmup = if warmups == 1 { "warmup" } else { "warmups" };

    let sample = if samples == 1 {
        "measured sample"
    } else {
        "measured samples"
    };

    format!("Measuring {workloads} {workload} with {warmups} {warmup} and {samples} {sample}")
}

fn completion_message(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();

    if seconds >= 60.0 {
        let minutes = (seconds / 60.0).floor();
        let remaining = seconds - minutes * 60.0;

        format!("Completed performance run in {minutes:.0}m {remaining:.1}s")
    } else {
        format!("Completed performance run in {seconds:.1}s")
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{completion_message, plan_message};

    #[test]
    fn progress_uses_developer_facing_counts_and_durations() {
        assert_eq!(
            plan_message(1, 2, 1),
            "Measuring 1 workload with 2 warmups and 1 measured sample"
        );

        assert_eq!(
            completion_message(Duration::from_millis(62_500)),
            "Completed performance run in 1m 2.5s"
        );
    }
}
