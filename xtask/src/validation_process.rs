//! Bounded capture for produced native validation executables.

use std::fmt;
use std::io::{self, Write};
use std::process::{ExitStatus, Output};
use std::time::{Duration, Instant};

use subprocess::{Exec, Job, Redirection};

pub(crate) const TIMEOUT: Duration = Duration::from_secs(30);
const REAP_TIMEOUT: Duration = Duration::from_secs(1);
const CAPTURE_LIMIT: usize = 1024 * 1024;
const READ_BATCH: usize = 64 * 1024;

#[derive(Debug)]
enum Cause {
    Spawn(io::Error),
    Read(io::Error),
    Wait(io::Error),
    Timeout(Duration),
    NonzeroExit,
    CaptureLimit,
}

#[derive(Debug)]
pub(crate) struct Failure {
    command: String,
    cause: Cause,
    status: Option<ExitStatus>,
    stdout: Capture,
    stderr: Capture,
    containment_errors: Vec<(&'static str, io::Error)>,
}

#[derive(Debug, Default)]
struct Capture {
    bytes: Vec<u8>,
    discarded: usize,
}

impl Capture {
    fn total(&self) -> usize {
        self.bytes.len().saturating_add(self.discarded)
    }
}

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let retained = bytes.len().min(CAPTURE_LIMIT - self.bytes.len());

        self.bytes.extend_from_slice(&bytes[..retained]);
        self.discarded = self.discarded.saturating_add(bytes.len() - retained);

        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ObservedCapture<'a> {
    capture: &'a mut Capture,
    observe: &'a mut dyn FnMut(&[u8]),
}

impl Write for ObservedCapture<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.capture.write(bytes)?;

        (self.observe)(&bytes[..written]);

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.capture.flush()
    }
}

pub(crate) fn output(command: Exec, timeout: Duration) -> Result<Output, Box<Failure>> {
    execute(command, timeout, false, &mut |_| {})
}

pub(crate) fn success(command: Exec, timeout: Duration) -> Result<Output, Box<Failure>> {
    execute(command, timeout, true, &mut |_| {})
}

pub(crate) fn output_observed(
    command: Exec,
    timeout: Duration,
    observe_stdout: &mut dyn FnMut(&[u8]),
) -> Result<Output, Box<Failure>> {
    execute(command, timeout, false, observe_stdout)
}

fn execute(
    command: Exec,
    timeout: Duration,
    require_success: bool,
    observe_stdout: &mut dyn FnMut(&[u8]),
) -> Result<Output, Box<Failure>> {
    let mut failure = Box::new(Failure {
        command: command.to_cmdline_lossy(),
        cause: Cause::Timeout(timeout),
        status: None,
        stdout: Capture::default(),
        stderr: Capture::default(),
        containment_errors: Vec::new(),
    });

    let deadline = Instant::now() + timeout;

    let child = command
        .stdin(Redirection::Null)
        .stdout(Redirection::Pipe)
        .stderr(Redirection::Pipe)
        .start();

    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            failure.cause = Cause::Spawn(error);

            return Err(failure);
        }
    };

    // The communicator multiplexes both streams without reader workers that could
    // outlive this call when a descendant retains an inherited pipe handle.
    let capture = capture(&mut child, deadline, timeout, &mut failure, observe_stdout);

    if let Err(cause) = capture {
        failure.cause = cause;
        contain(&child, &mut failure);

        return Err(failure);
    }

    if failure.stdout.discarded != 0 || failure.stderr.discarded != 0 {
        failure.cause = Cause::CaptureLimit;

        return Err(failure);
    }

    let status = failure
        .status
        .expect("completed capture has reaped its child");

    if require_success && !status.success() {
        failure.cause = Cause::NonzeroExit;

        return Err(failure);
    }

    Ok(Output {
        status,
        stdout: failure.stdout.bytes,
        stderr: failure.stderr.bytes,
    })
}

fn capture(
    child: &mut Job,
    deadline: Instant,
    timeout: Duration,
    failure: &mut Failure,
    observe_stdout: &mut dyn FnMut(&[u8]),
) -> Result<(), Cause> {
    let mut reader = child.communicate().map_err(Cause::Read)?;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());

        if remaining.is_zero() {
            return Err(Cause::Timeout(timeout));
        }

        let before = (failure.stdout.total(), failure.stderr.total());

        // Bound each batch as well as time: continuously ready Windows pipes can
        // otherwise avoid the timed wait inside the communicator.
        reader = reader.limit_size(READ_BATCH).limit_time(remaining);

        let result = reader.read_to(
            &mut ObservedCapture {
                capture: &mut failure.stdout,
                observe: observe_stdout,
            },
            &mut failure.stderr,
        );

        if let Err(error) = result {
            return Err(if error.kind() == io::ErrorKind::TimedOut {
                Cause::Timeout(timeout)
            } else {
                Cause::Read(error)
            });
        }

        if before == (failure.stdout.total(), failure.stderr.total()) {
            break;
        }
    }

    // EOF does not imply exit: a child can close its streams and keep running.
    failure.status = child
        .wait_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(Cause::Wait)?
        .map(exit_status)
        .transpose()
        .map_err(Cause::Wait)?;

    if failure.status.is_none() {
        return Err(Cause::Timeout(timeout));
    }

    Ok(())
}

fn contain(child: &Job, failure: &mut Failure) {
    if let Err(error) = child.kill() {
        failure.containment_errors.push(("kill", error));
    }

    match child.wait_timeout(REAP_TIMEOUT) {
        Ok(Some(status)) => match exit_status(status) {
            Ok(status) => failure.status = Some(status),
            Err(error) => failure.containment_errors.push(("reap status", error)),
        },
        result => {
            let error = match result {
                Err(error) => error,
                Ok(None) => {
                    io::Error::new(io::ErrorKind::TimedOut, "child was not reaped after kill")
                }
                Ok(Some(_)) => unreachable!("reaped status handled above"),
            };

            failure.containment_errors.push(("reap", error));

            // Prevent the library's dropping wait from hiding the original failure
            // behind an unbounded wait when OS containment itself failed.
            child.detach();
        }
    }
}

fn exit_status(status: subprocess::ExitStatus) -> io::Result<ExitStatus> {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt as _;

    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt as _;

    #[cfg(unix)]
    use subprocess::unix::ExitStatusExt as _;

    #[cfg(windows)]
    use subprocess::windows::ExitStatusExt as _;

    status.into_raw().map(ExitStatus::from_raw).ok_or_else(|| {
        io::Error::other("validation child exit status was consumed by another waiter")
    })
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native validation command `{}`: ", self.command)?;

        match &self.cause {
            Cause::Spawn(error) => write!(formatter, "spawn failed: {error}"),
            Cause::Read(error) => write!(formatter, "capture failed: {error}"),
            Cause::Wait(error) => write!(formatter, "wait failed: {error}"),
            Cause::Timeout(timeout) => write!(formatter, "exceeded {timeout:?} execution deadline"),
            Cause::NonzeroExit => formatter.write_str("nonzero exit"),
            Cause::CaptureLimit => formatter.write_str("output exceeded capture limit"),
        }?;

        if let Some(status) = self.status {
            write!(formatter, " ({status})")?;
        }

        for (operation, error) in &self.containment_errors {
            write!(formatter, "\ncontainment {operation} failed: {error}")?;
        }

        for (name, capture) in [("stdout", &self.stdout), ("stderr", &self.stderr)] {
            write!(
                formatter,
                "\n{name}:\n{}",
                String::from_utf8_lossy(&capture.bytes)
            )?;

            if capture.discarded != 0 {
                write!(
                    formatter,
                    "\n[{name} truncated: {} bytes discarded]",
                    capture.discarded
                )?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            Cause::Spawn(error) | Cause::Read(error) | Cause::Wait(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::time::{Duration, Instant};

    use subprocess::Exec;

    use super::{CAPTURE_LIMIT, Cause, Failure, output, output_observed, success};

    fn fixture(mode: &str) -> Exec {
        Exec::cmd(std::env::current_exe().expect("test executable must be available"))
            .args([
                "--exact",
                "validation_process::tests::child_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("BRAY_VALIDATION_PROCESS_FIXTURE", mode)
    }

    #[test]
    fn drains_both_streams_beyond_pipe_capacity_without_misattribution() {
        let output = success(fixture("noisy"), Duration::from_secs(5))
            .expect("noisy child must finish while both pipes are drained");

        assert_eq!(
            output.stdout.iter().filter(|byte| **byte == 0xa5).count(),
            256 * 1024
        );

        assert_eq!(
            output.stderr.iter().filter(|byte| **byte == 0x5a).count(),
            256 * 1024
        );

        assert!(!output.stdout.contains(&0x5a));
        assert!(!output.stderr.contains(&0xa5));
    }

    #[test]
    fn timeout_preserves_partial_output_and_reaps_without_claiming_cleanup() {
        let start = Instant::now();

        let failure = output(fixture("hang"), Duration::from_millis(500))
            .expect_err("hanging child must time out");

        assert!(matches!(failure.cause, Cause::Timeout(_)));
        assert!(start.elapsed() < Duration::from_secs(5));

        assert!(
            failure
                .command
                .contains("validation_process::tests::child_fixture")
        );

        assert!(String::from_utf8_lossy(&failure.stderr.bytes).contains("started"));
        assert!(!String::from_utf8_lossy(&failure.stderr.bytes).contains("fixture cleanup"));
        assert_reaped(&failure);
    }

    #[test]
    fn observation_sees_partial_stdout_before_the_deadline() {
        let mut observed = Vec::new();

        let failure = output_observed(fixture("hang"), Duration::from_millis(500), &mut |bytes| {
            observed.extend_from_slice(bytes);
        })
        .expect_err("hanging observed child must time out");

        assert!(matches!(failure.cause, Cause::Timeout(_)));
        assert!(!observed.is_empty());
        assert_eq!(observed, failure.stdout.bytes);
        assert_reaped(&failure);
    }

    #[test]
    fn continuous_output_cannot_extend_the_absolute_deadline() {
        let start = Instant::now();

        let failure = output(fixture("flood"), Duration::from_millis(500))
            .expect_err("continuously ready pipes must still time out");

        assert!(matches!(failure.cause, Cause::Timeout(_)));
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(failure.stdout.bytes.len() <= CAPTURE_LIMIT);
        assert!(failure.stderr.bytes.len() <= CAPTURE_LIMIT);
        assert!(!failure.stderr.bytes.is_empty());
        assert_reaped(&failure);
    }

    fn assert_reaped(failure: &Failure) {
        assert!(
            failure.status.is_some(),
            "child must have been reaped: {failure}"
        );

        assert!(
            failure.containment_errors.is_empty(),
            "containment failed: {failure}"
        );

        #[cfg(target_os = "linux")]
        {
            let stdout = String::from_utf8_lossy(&failure.stdout.bytes);

            let pid = stdout
                .split("pid=")
                .nth(1)
                .and_then(|tail| tail.lines().next())
                .expect("fixture must report its PID");

            assert!(
                !std::path::Path::new("/proc").join(pid).exists(),
                "child remains alive or a zombie"
            );
        }
    }

    #[test]
    fn nonzero_exit_remains_available_to_scenario_oracles_or_reports_full_failure() {
        let result = output(fixture("nonzero"), Duration::from_secs(5))
            .expect("expected nonzero status is a scenario outcome");

        assert_eq!(result.status.code(), Some(17));
        assert!(String::from_utf8_lossy(&result.stderr).contains("original failure"));

        let failure = success(fixture("nonzero"), Duration::from_secs(5))
            .expect_err("success policy must reject nonzero exit");

        assert!(matches!(failure.cause, Cause::NonzeroExit));
        assert_eq!(failure.status.and_then(|status| status.code()), Some(17));
        assert!(failure.to_string().contains("original failure"));
    }

    #[test]
    fn excessive_output_is_drained_and_explicitly_truncated_after_reaping() {
        let failure = output(fixture("excessive"), Duration::from_secs(5))
            .expect_err("incomplete capture must never satisfy an output oracle");

        assert!(matches!(failure.cause, Cause::CaptureLimit));
        assert!(failure.status.is_some_and(|status| status.success()));
        assert_eq!(failure.stdout.bytes.len(), CAPTURE_LIMIT);
        assert!(failure.stdout.discarded >= CAPTURE_LIMIT);
        assert!(failure.to_string().contains("stdout truncated:"));
    }

    #[test]
    fn spawn_failure_preserves_its_io_cause_and_command() {
        let directory = tempfile::tempdir().expect("spawn fixture directory must exist");
        let executable = directory.path().join("missing-validation-executable");

        let failure = output(Exec::cmd(&executable), Duration::from_secs(1))
            .expect_err("missing child must fail to spawn");

        assert!(
            matches!(&failure.cause, Cause::Spawn(error) if error.kind() == std::io::ErrorKind::NotFound)
        );

        assert!(failure.command.contains("missing-validation-executable"));
        assert!(std::error::Error::source(&*failure).is_some());
        assert!(failure.status.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn deadline_also_bounds_a_child_that_closes_its_streams_before_hanging() {
        let start = Instant::now();

        let failure = output(
            Exec::cmd("sh").args(["-c", "exec 1>&- 2>&-; exec sleep 30"]),
            Duration::from_millis(100),
        )
        .expect_err("EOF must not turn the exit wait into an unbounded wait");

        assert!(matches!(failure.cause, Cause::Timeout(_)));
        assert!(failure.status.is_some());
        assert!(failure.containment_errors.is_empty());
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    #[ignore = "isolated subprocess fixture invoked by the parent tests"]
    fn child_fixture() {
        struct Cleanup;

        impl Drop for Cleanup {
            fn drop(&mut self) {
                eprintln!("fixture cleanup");
            }
        }

        let _cleanup = Cleanup;

        match std::env::var("BRAY_VALIDATION_PROCESS_FIXTURE")
            .expect("parent sets fixture mode")
            .as_str()
        {
            "noisy" => {
                let stderr = std::thread::spawn(|| {
                    std::io::stderr()
                        .write_all(&vec![0x5a; 256 * 1024])
                        .expect("fixture stderr must write");
                });

                std::io::stdout()
                    .write_all(&vec![0xa5; 256 * 1024])
                    .expect("fixture stdout must write");

                stderr.join().expect("fixture writer must finish");
            }
            "hang" => {
                println!("pid={}", std::process::id());
                std::io::stdout().flush().expect("PID must reach parent");
                eprintln!("started");
                std::thread::sleep(Duration::from_secs(30));
            }
            "flood" => {
                println!("pid={}", std::process::id());
                std::io::stdout().flush().expect("PID must reach parent");

                loop {
                    std::io::stdout()
                        .write_all(&[0xa5; 4096])
                        .expect("fixture stdout must drain");

                    std::io::stderr()
                        .write_all(&[0x5a; 4096])
                        .expect("fixture stderr must drain");
                }
            }
            "nonzero" => {
                eprintln!("original failure");
                std::process::exit(17);
            }
            "excessive" => {
                std::io::stdout()
                    .write_all(&vec![0xa5; 2 * CAPTURE_LIMIT])
                    .expect("fixture stdout must drain");
            }
            mode => panic!("unexpected child fixture mode {mode}"),
        }
    }
}
