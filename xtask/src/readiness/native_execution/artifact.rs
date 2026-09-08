use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

pub(super) fn require_equal_files(left: &Path, right: &Path, artifact: &str) -> Result<(), String> {
    let left =
        std::fs::read(left).map_err(|error| format!("could not read first {artifact}: {error}"))?;

    let right = std::fs::read(right)
        .map_err(|error| format!("could not read second {artifact}: {error}"))?;

    if left != right {
        return Err(format!("{artifact} differs across repeated native builds"));
    }

    Ok(())
}

pub(super) fn object_files(directory: &Path, target: NativeTarget) -> Result<Vec<PathBuf>, String> {
    let suffix =
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::RelocatableObject)
            .suffix()
            .trim_start_matches('.')
            .to_owned();

    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("could not list native output directory: {error}"))?;

    let mut objects = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("could not read native output entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    objects.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == suffix.as_str())
    });

    objects.sort();

    if objects.is_empty() {
        return Err("native build produced no relocatable objects".to_owned());
    }

    Ok(objects)
}

pub(super) fn require_equal_artifacts(left: &[PathBuf], right: &[PathBuf]) -> Result<(), String> {
    if left.len() != right.len() {
        return Err("repeated native builds produced different object counts".to_owned());
    }

    for (left, right) in left.iter().zip(right) {
        if left.file_name() != right.file_name() {
            return Err("repeated native builds produced different object names".to_owned());
        }

        require_equal_files(left, right, "relocatable object")?;
    }

    Ok(())
}

pub(super) fn inspect_objects(root: &Path, objects: &[PathBuf]) -> Result<String, String> {
    let tool = llvm_tool(
        root,
        bray_diagnostics::DiagnosticLlvmToolRole::ObjectInspector,
    );

    let mut report = String::new();

    for object in objects {
        let mut command = Command::new(&tool);

        command.args(["--file-headers", "--sections", "--relocations", "--symbols"]);

        command.arg(object);

        let output = crate::command::require_success(command, "inspecting native object")?;

        report.push_str(&String::from_utf8_lossy(&output.stdout));
    }

    Ok(report)
}

pub(super) fn require_evidence(report: &str, required: &[&str]) -> Result<(), String> {
    for required in required {
        if !report.contains(required) {
            return Err(format!(
                "native object inspection is missing required evidence: {required}"
            ));
        }
    }

    Ok(())
}

pub(super) fn reject_evidence(report: &str, forbidden: &[&str]) -> Result<(), String> {
    for forbidden in forbidden {
        if report.contains(forbidden) {
            return Err(format!(
                "native object inspection contains forbidden evidence: {forbidden}"
            ));
        }
    }

    Ok(())
}

pub(super) fn llvm_tool(root: &Path, tool: bray_diagnostics::DiagnosticLlvmToolRole) -> PathBuf {
    bray_tooling::llvm_tool_path(tool)
        .unwrap_or_else(|_| bray_llvm_toolchain::tool_path(root, tool.executable_name()))
}
