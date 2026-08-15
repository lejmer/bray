use std::time::{Duration, Instant};

const PREFIX: &str = "[xtask]";

pub(crate) fn run<T, E>(label: &str, operation: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    eprintln!("{}", start_message(label));

    let started = Instant::now();
    let result = operation();

    let outcome = if result.is_ok() {
        "completed"
    } else {
        "failed"
    };

    eprintln!("{}", finish_message(label, outcome, started.elapsed()));

    result
}

pub(crate) fn item(index: usize, total: usize, label: &str) {
    eprintln!("{PREFIX} [{index}/{total}] {label}");
}

pub(crate) fn message(label: &str) {
    eprintln!("{PREFIX} {label}");
}

fn start_message(label: &str) -> String {
    format!("{PREFIX} {label}")
}

fn finish_message(label: &str, outcome: &str, elapsed: Duration) -> String {
    format!(
        "{PREFIX} {outcome} {label} in {:.1}s",
        elapsed.as_secs_f64()
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{finish_message, start_message};

    #[test]
    fn phase_messages_include_identity_outcome_and_elapsed_time() {
        assert_eq!(
            start_message("Building compiler tools"),
            "[xtask] Building compiler tools"
        );

        assert_eq!(
            finish_message(
                "Building compiler tools",
                "completed",
                Duration::from_millis(1250)
            ),
            "[xtask] completed Building compiler tools in 1.2s"
        );
    }
}
