use std::path::Path;
use std::process::Command;

use bray_diagnostics::DiagnosticLlvmToolRole;
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

use super::files;
use super::model::{LinkKind, TargetDescription};
use crate::command::require_success;

pub(in crate::standard_library) fn run(
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), String> {
    match arguments.next().as_deref() {
        Some("generate") => generate(arguments),
        Some("probe") => probe(arguments),
        _ => Err("expected `os-bindings <generate [--check] | probe [--target <triple>]>`".to_owned()),
    }
}

pub(in crate::standard_library) fn verify() -> Result<(), String> {
    files::generate(true)
}

fn generate(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let check = match arguments.next().as_deref() {
        None => false,
        Some("--check") => true,
        Some(argument) => return Err(format!("unexpected argument: {argument}")),
    };

    crate::command::reject_trailing_argument(arguments)?;

    files::generate(check)
}

fn probe(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let requested = match arguments.next().as_deref() {
        None => None,
        Some("--target") => Some(
            arguments
                .next()
                .ok_or_else(|| "missing value for --target".to_owned())?,
        ),
        Some(argument) => return Err(format!("unexpected argument: {argument}")),
    };

    crate::command::reject_trailing_argument(arguments)?;

    let host = NativeTarget::current()
        .ok_or_else(|| "the current host has no supported native target profile".to_owned())?;

    let requested = requested.as_deref().unwrap_or_else(|| host.as_str());

    if requested != host.as_str() {
        return Err(format!(
            "native OS binding probes run on their exact target host, requested {requested}, current host {}",
            host.as_str()
        ));
    }

    files::generate(true)?;

    let loaded = files::load()?;

    let probe = loaded
        .probe(requested)
        .ok_or_else(|| format!("no OS binding probe exists for {requested}"))?;

    let directory = tempfile::Builder::new()
        .prefix("bray-os-binding-probe-")
        .tempdir()
        .map_err(|error| format!("could not create native probe directory: {error}"))?;

    let executable_name = TargetOutputName::for_native(
        host.object_format(),
        TargetOutputKind::Executable,
    )
    .file_name("os_binding_probe")
    .ok_or_else(|| "could not name the native probe executable".to_owned())?;

    let executable = directory.path().join(executable_name);

    compile(&probe.source, &executable, host, probe.target)?;

    require_success(
        Command::new(&executable),
        "running the native OS binding probe",
    )
    .map(|_| ())
}

fn compile(
    source: &Path,
    executable: &Path,
    target: NativeTarget,
    description: &TargetDescription,
) -> Result<(), String> {
    let compiler = bray_tooling::llvm_tool_path(DiagnosticLlvmToolRole::CompilerDriver)
        .map_err(|error| format!("the provisioned C compiler is unavailable: {error}"))?;

    let mut command = Command::new(compiler);

    command
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
        .arg(format!("--target={}", target.as_str()))
        .arg(source)
        .arg("-o")
        .arg(executable);

    add_link_arguments(&mut command, description);

    require_success(command, "compiling the native OS binding probe").map(|_| ())
}

fn add_link_arguments(command: &mut Command, description: &TargetDescription) {
    for link in &description.links {
        match link.kind {
            LinkKind::Framework => {
                command.args(["-framework", &link.name]);
            }
            LinkKind::Dynamic | LinkKind::Static | LinkKind::System => {
                command.arg(format!("-l{}", link.name));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn command_requires_a_known_operation() {
        assert!(run(["unknown".to_owned()].into_iter()).is_err());
    }
}
