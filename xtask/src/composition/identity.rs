use std::fs;
use std::hash::Hasher;
use std::path::Path;

use bray_emitter::ArtifactKind;

use super::{command, report};

pub(super) fn audit(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    case: &str,
) -> Result<(), String> {
    let generation = report::retained(workspace, case)?;

    let host = generation
        .artifact_path(ArtifactKind::Executable, 0)
        .ok_or("composition host is absent")?
        .to_path_buf();

    let catalog = generation
        .artifact_path(ArtifactKind::TestCatalog, 0)
        .ok_or("composition catalog is absent")?
        .to_path_buf();

    let directory = host
        .ancestors()
        .take_while(|path| path.starts_with(workspace))
        .find(|path| path.join("manifest.json").is_file())
        .ok_or("composition generation manifest is absent")?;

    let manifest = directory.join("manifest.json");

    let reference = directory
        .parent()
        .ok_or("composition generation store is absent")?
        .join("published-generation.json");

    let original = read(&manifest)?;
    let original_reference = read(&reference)?;

    // Change only the recorded identity of a real emitted product. Each probe must fail before
    // execution, and the original generation is restored before the next probe.
    for part in [
        "inputs",
        "compiler",
        "toolchain",
        "standard_library",
        "runtime",
        "catalog_protocol",
        "runner_protocol",
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&original).map_err(|error| error.to_string())?;

        let identity = &mut value["build_identity"][part];

        if let Some(bytes) = identity.as_array_mut() {
            bytes[0] = (bytes[0].as_u64().ok_or("invalid identity byte")? ^ 1).into();
        } else {
            *identity = (identity.as_u64().ok_or("invalid protocol identity")? + 1).into();
        }

        let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
        let mut digest = bray_base::StableDigestHasher::new();
        digest.write(&bytes);

        let mut reference_value: serde_json::Value =
            serde_json::from_slice(&original_reference).map_err(|error| error.to_string())?;

        reference_value["current"]["manifest_digest"] =
            bray_base::lowercase_hex(&digest.finalize()).into();

        let reference_bytes =
            serde_json::to_vec(&reference_value).map_err(|error| error.to_string())?;

        with_changed_file(&manifest, &bytes, || {
            with_changed_file(&reference, &reference_bytes, || {
                rejected(root, workspace, toolchain, case, Some(part))
            })
        })?;
    }

    for path in [&host, &catalog] {
        let mut bytes = read(path)?;
        bytes.push(0);

        with_changed_file(path, &bytes, || {
            rejected(root, workspace, toolchain, case, None)
        })?;
    }

    Ok(())
}

pub(super) fn audit_runtime(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    case: &str,
) -> Result<(), String> {
    let target = bray_target::NativeTarget::current().ok_or("unsupported composition host")?;
    let metadata = toolchain
        .join("lib/bray/runtime")
        .join(target.as_str())
        .join("bray-runtime.brayrt");
    let original = read(&metadata)?;

    for (field, expected_key, expected_value) in [
        ("abi", "kind", "runtime_artifact_abi_mismatch"),
        ("roles", "category", "missing_role_owner"),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&original).map_err(|error| error.to_string())?;

        if field == "abi" {
            value["abi"]["major"] = (value["abi"]["major"]
                .as_u64()
                .ok_or("runtime ABI major is absent")?
                + 1)
            .into();
        } else {
            let components = value["components"]
                .as_array_mut()
                .ok_or("runtime components are absent")?;

            let roles = components
                .iter_mut()
                .filter_map(|component| component["roles"].as_array_mut())
                .find(|roles| !roles.is_empty())
                .ok_or("runtime role owner is absent")?;

            roles.remove(0);
        }

        let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;

        with_changed_file(&metadata, &bytes, || {
            let output = command::test_command(root, workspace, toolchain, case, false)
                .output()
                .map_err(|error| format!("runtime identity probe launch: {error}"))?;

            let json: serde_json::Value =
                serde_json::from_slice(&output.stdout).map_err(|error| {
                    format!(
                        "runtime identity probe: {error}\n{}",
                        crate::command::failure(case, &output)
                    )
                })?;

            if output.status.success() || !contains(&json, expected_key, expected_value) {
                return Err(format!(
                    "runtime {field}: expected {expected_value}\n{}",
                    crate::command::failure(case, &output)
                ));
            }

            Ok(())
        })?;
    }

    Ok(())
}

fn rejected(
    root: &Path,
    workspace: &Path,
    toolchain: &Path,
    case: &str,
    part: Option<&str>,
) -> Result<(), String> {
    let output = command::test_command(root, workspace, toolchain, case, true)
        .output()
        .map_err(|error| format!("composition identity probe launch: {error}"))?;

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "composition identity probe report: {error}\n{}",
            crate::command::failure(case, &output)
        )
    })?;

    let expected = part.map_or_else(
        || contains(&json, "kind", "retained_artifact_length_mismatch"),
        |part| {
            contains(&json, "part", part)
                && contains(&json, "reason", "reusable_build_identity_mismatch")
        },
    );

    if output.status.success() || !expected {
        return Err(format!(
            "composition identity {part:?}: expected exact rejection\n{}",
            crate::command::failure(case, &output)
        ));
    }

    Ok(())
}

fn contains(value: &serde_json::Value, key: &str, expected: &str) -> bool {
    match value {
        serde_json::Value::Object(fields) => {
            fields.get(key).and_then(serde_json::Value::as_str) == Some(expected)
                || fields.values().any(|value| contains(value, key, expected))
        }
        serde_json::Value::Array(values) => {
            values.iter().any(|value| contains(value, key, expected))
        }
        _ => false,
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| crate::workspace::io_error("read", path, error))
}

fn with_changed_file(
    path: &Path,
    bytes: &[u8],
    action: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let original = read(path)?;

    let result = fs::write(path, bytes)
        .map_err(|error| crate::workspace::io_error("write", path, error))
        .and_then(|()| action());

    let restored = fs::write(path, original)
        .map_err(|error| crate::workspace::io_error("restore", path, error));

    match (result, restored) {
        (Err(error), Err(restore)) => Err(format!("{error}\n{restore}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}
