use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Instant;

use serde::Deserialize;

use super::compilation::external_invocation;
use super::model::{
    COMPILER_PROCESS_SCOPE, COMPILER_WORK_SCOPE, PeerCompilerConfiguration, ToolInvocationReport,
    WorkloadCompilationReport,
};

pub(in crate::performance) struct ProcessRun {
    elapsed_nanoseconds: u64,
    invocation: ToolInvocationReport,
}

pub(in crate::performance) struct CompilerRun {
    elapsed_nanoseconds: u64,
    components_nanoseconds: BTreeMap<String, u64>,
    invocation: ToolInvocationReport,
}

pub(in crate::performance) fn report(
    process: ProcessRun,
    compiler: CompilerRun,
) -> WorkloadCompilationReport {
    WorkloadCompilationReport {
        process_scope: COMPILER_PROCESS_SCOPE.to_owned(),
        process_elapsed_nanoseconds: process.elapsed_nanoseconds,
        compiler_scope: COMPILER_WORK_SCOPE.to_owned(),
        compiler_elapsed_nanoseconds: compiler.elapsed_nanoseconds,
        compiler_components_nanoseconds: compiler.components_nanoseconds,
        process_invocation: process.invocation,
        profiled_invocation: compiler.invocation,
    }
}

pub(in crate::performance) fn process(
    program: &str,
    configuration: &PeerCompilerConfiguration,
    description: &str,
) -> Result<ProcessRun, String> {
    let (elapsed_nanoseconds, _) = run(
        program,
        &configuration.arguments,
        &configuration.environment,
        description,
    )?;

    Ok(ProcessRun {
        elapsed_nanoseconds,
        invocation: invocation(
            program,
            &configuration.arguments,
            &configuration.environment,
        ),
    })
}

pub(in crate::performance) fn rust(
    program: &str,
    configuration: &PeerCompilerConfiguration,
    description: &str,
) -> Result<CompilerRun, String> {
    let (_, output) = run(
        program,
        &configuration.arguments,
        &configuration.environment,
        description,
    )?;

    let elapsed_nanoseconds = rust_total_nanoseconds(&output.stderr)?;

    Ok(CompilerRun {
        elapsed_nanoseconds,
        components_nanoseconds: BTreeMap::from([("rustc".to_owned(), elapsed_nanoseconds)]),
        invocation: invocation(
            program,
            &configuration.arguments,
            &configuration.environment,
        ),
    })
}

pub(in crate::performance) fn cpp(
    program: &str,
    configuration: &PeerCompilerConfiguration,
    compiler_trace: &Path,
    linker_trace: &Path,
    description: &str,
) -> Result<CompilerRun, String> {
    run(
        program,
        &configuration.arguments,
        &configuration.environment,
        description,
    )?;

    let compiler_nanoseconds = llvm_trace_nanoseconds(compiler_trace, "Total ExecuteCompiler")?;
    let linker_nanoseconds = llvm_linker_trace_nanoseconds(linker_trace)?;

    let elapsed_nanoseconds = compiler_nanoseconds
        .checked_add(linker_nanoseconds)
        .ok_or_else(|| "C++ compiler work exceeds the report bound".to_owned())?;

    Ok(CompilerRun {
        elapsed_nanoseconds,
        components_nanoseconds: BTreeMap::from([
            ("clang".to_owned(), compiler_nanoseconds),
            ("lld".to_owned(), linker_nanoseconds),
        ]),
        invocation: invocation(
            program,
            &configuration.arguments,
            &configuration.environment,
        ),
    })
}

pub(in crate::performance) fn bray(
    program: &Path,
    arguments: &[String],
    profile: &Path,
    description: &str,
) -> Result<CompilerRun, String> {
    let program_identity = crate::path::slash_separated(program);

    let environment = BTreeMap::new();

    run(&program_identity, arguments, &environment, description)?;

    let profile_bytes = std::fs::read(profile)
        .map_err(|error| format!("could not read {}: {error}", profile.display()))?;

    let profile: bray_compilation::CompilationProfileReport =
        serde_json::from_slice(&profile_bytes)
            .map_err(|error| format!("could not decode compiler profile: {error}"))?;

    profile
        .validate()
        .map_err(|error| format!("compiler profile is invalid: {error:?}"))?;

    let elapsed_nanoseconds = profile.elapsed_nanoseconds;

    Ok(CompilerRun {
        elapsed_nanoseconds,
        components_nanoseconds: BTreeMap::from([("brayc".to_owned(), elapsed_nanoseconds)]),
        invocation: invocation(&program_identity, arguments, &environment),
    })
}

pub(in crate::performance) fn bray_process(
    program: &Path,
    arguments: &[String],
    description: &str,
) -> Result<ProcessRun, String> {
    let program_identity = crate::path::slash_separated(program);

    let environment = BTreeMap::new();

    let (elapsed_nanoseconds, _) = run(&program_identity, arguments, &environment, description)?;

    Ok(ProcessRun {
        elapsed_nanoseconds,
        invocation: invocation(&program_identity, arguments, &environment),
    })
}

fn run(
    program: &str,
    arguments: &[String],
    environment: &BTreeMap<String, String>,
    description: &str,
) -> Result<(u64, Output), String> {
    let mut command = Command::new(program);

    command.args(arguments).envs(environment);

    let started = Instant::now();
    let output = crate::command::require_success(command, description)?;
    let elapsed_nanoseconds = super::peer::elapsed_nanoseconds(started);

    Ok((elapsed_nanoseconds, output))
}

fn invocation(
    program: &str,
    arguments: &[String],
    environment: &BTreeMap<String, String>,
) -> ToolInvocationReport {
    external_invocation(program.to_owned(), arguments.to_vec(), environment.clone())
}

#[derive(Deserialize)]
struct RustTiming {
    pass: String,
    time: f64,
}

fn rust_total_nanoseconds(stderr: &[u8]) -> Result<u64, String> {
    let stderr = String::from_utf8_lossy(stderr);

    let totals = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("time: "))
        .map(serde_json::from_str::<RustTiming>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("rustc timing output is invalid: {error}"))?
        .into_iter()
        .filter(|timing| timing.pass == "total")
        .collect::<Vec<_>>();

    let [total] = totals.as_slice() else {
        return Err("rustc timing output must contain one total".to_owned());
    };

    seconds_to_nanoseconds(total.time, "rustc total")
}

fn seconds_to_nanoseconds(seconds: f64, owner: &str) -> Result<u64, String> {
    let nanoseconds = seconds * 1_000_000_000.0;

    if !nanoseconds.is_finite() || nanoseconds <= 0.0 || nanoseconds > u64::MAX as f64 {
        return Err(format!("{owner} timing is outside its supported range"));
    }

    Ok(nanoseconds.round() as u64)
}

#[derive(Deserialize)]
struct LlvmTrace {
    #[serde(rename = "traceEvents")]
    events: Vec<LlvmTraceEvent>,
}

#[derive(Deserialize)]
struct LlvmTraceEvent {
    name: String,
    #[serde(default)]
    dur: Option<u64>,
}

fn llvm_trace_nanoseconds(path: &Path, event_name: &str) -> Result<u64, String> {
    let trace = read_llvm_trace(path)?;

    let durations = trace
        .events
        .iter()
        .filter(|event| event.name == event_name)
        .filter_map(|event| event.dur)
        .collect::<Vec<_>>();

    let [microseconds] = durations.as_slice() else {
        return Err(format!(
            "LLVM timing trace {} must contain one {event_name} event",
            path.display()
        ));
    };

    microseconds
        .checked_mul(1_000)
        .ok_or_else(|| format!("LLVM timing trace {} exceeds its bound", path.display()))
}

fn llvm_linker_trace_nanoseconds(path: &Path) -> Result<u64, String> {
    let trace = read_llvm_trace(path)?;

    let durations = trace
        .events
        .iter()
        .filter(|event| event.name.starts_with("Total ") && event.name.ends_with(" link"))
        .filter_map(|event| event.dur)
        .collect::<Vec<_>>();

    let [microseconds] = durations.as_slice() else {
        return Err(format!(
            "LLVM timing trace {} must contain one total link event",
            path.display()
        ));
    };

    microseconds
        .checked_mul(1_000)
        .ok_or_else(|| format!("LLVM timing trace {} exceeds its bound", path.display()))
}

fn read_llvm_trace(path: &Path) -> Result<LlvmTrace, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "could not decode LLVM timing trace {}: {error}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{llvm_linker_trace_nanoseconds, rust_total_nanoseconds};

    #[test]
    fn rust_total_uses_the_compiler_reported_total() {
        let output = b"time: {\"pass\":\"parse_crate\",\"time\":0.001}\n\
            time: {\"pass\":\"total\",\"time\":0.0123456}\n";

        assert_eq!(rust_total_nanoseconds(output), Ok(12_345_600));
    }

    #[test]
    fn linker_trace_accepts_each_native_object_format_name() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must be available: {error}"));

        for name in ["COFF", "ELF", "Mach-O"] {
            let path = directory.path().join(format!("{name}.json"));

            let contents =
                format!("{{\"traceEvents\":[{{\"name\":\"Total {name} link\",\"dur\":123}}]}}");

            std::fs::write(&path, contents)
                .unwrap_or_else(|error| panic!("timing trace must be writable: {error}"));

            assert_eq!(llvm_linker_trace_nanoseconds(&path), Ok(123_000));
        }
    }
}
