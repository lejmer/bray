use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "xtask/fixtures/native-execution/control-flow.bray";
const PRODUCT_NAME: &str = "application";

pub(super) fn audit(root: &Path) -> Result<(), String> {
    build_compiler(root)?;

    let first = tempfile::Builder::new()
        .prefix("bray-native-execution-first-")
        .tempdir()
        .map_err(|error| format!("could not create first native output directory: {error}"))?;

    let second = tempfile::Builder::new()
        .prefix("bray-native-execution-second-")
        .tempdir()
        .map_err(|error| format!("could not create second native output directory: {error}"))?;

    build_fixture(root, first.path())?;
    build_fixture(root, second.path())?;

    let first_executable = first.path().join(PRODUCT_NAME);
    let second_executable = second.path().join(PRODUCT_NAME);
    let first_objects = object_files(first.path())?;
    let second_objects = object_files(second.path())?;

    require_equal_files(&first_executable, &second_executable, "linked executable")?;
    require_equal_artifacts(&first_objects, &second_objects)?;
    inspect_objects(root, &first_objects)?;

    execute_product(&first_executable)
}

fn build_compiler(root: &Path) -> Result<(), String> {
    let mut command = Command::new("cargo");

    command
        .current_dir(root)
        .args(["build", "--quiet", "--package", "brayc"]);

    require_success(command, "building brayc").map(|_| ())
}

fn build_fixture(root: &Path, output: &Path) -> Result<(), String> {
    let compiler = root
        .join("target")
        .join("debug")
        .join(executable_name("brayc"));

    let fixture = root.join(FIXTURE);
    let mut command = Command::new(compiler);

    command.current_dir(root).args([
        "build",
        "--product-kind",
        "executable",
        "--inspect",
        "relocatable-object",
        "--output",
    ]);

    command.arg(output).arg(fixture);

    require_success(command, "building native execution fixture").map(|_| ())
}

fn require_equal_files(left: &Path, right: &Path, artifact: &str) -> Result<(), String> {
    let left = std::fs::read(left)
        .map_err(|error| format!("could not read first {artifact}: {error}"))?;

    let right = std::fs::read(right)
        .map_err(|error| format!("could not read second {artifact}: {error}"))?;

    if left != right {
        return Err(format!("{artifact} differs across repeated native builds"));
    }

    Ok(())
}

fn object_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut objects = std::fs::read_dir(directory)
        .map_err(|error| format!("could not list native output directory: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "o"))
        .collect::<Vec<_>>();

    objects.sort();

    if objects.is_empty() {
        return Err("native build produced no relocatable objects".to_owned());
    }

    Ok(objects)
}

fn require_equal_artifacts(left: &[PathBuf], right: &[PathBuf]) -> Result<(), String> {
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

fn inspect_objects(root: &Path, objects: &[PathBuf]) -> Result<(), String> {
    let tool = llvm_tool(root, "llvm-readobj");
    let mut report = String::new();

    for object in objects {
        let mut command = Command::new(&tool);

        command.args([
            "--file-headers",
            "--sections",
            "--relocations",
            "--symbols",
        ]);

        command.arg(object);

        let output = require_success(command, "inspecting native object")?;

        report.push_str(&String::from_utf8_lossy(&output.stdout));
    }

    for required in ["Format: elf64-x86-64", "Name: .text", "Name: _start"] {
        if !report.contains(required) {
            return Err(format!(
                "native object inspection is missing required evidence: {required}"
            ));
        }
    }

    Ok(())
}

fn execute_product(executable: &Path) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        return require_success(Command::new(executable), "executing native Bray product")
            .map(|_| ());
    }

    if cfg!(windows) {
        let mut translate = Command::new("wsl.exe");
        let windows_path = executable.to_string_lossy().replace('\\', "/");

        translate.args(["wslpath", "-a", "-u"]).arg(windows_path);

        let translated = require_success(translate, "translating the native product path")?;

        let executable = String::from_utf8(translated.stdout)
            .map_err(|_| "WSL returned a non-UTF-8 product path".to_owned())?;

        let mut command = Command::new("wsl.exe");

        command.arg("--exec").arg(executable.trim());

        return require_success(command, "executing native Bray product through WSL")
            .map(|_| ());
    }

    Err("native execution readiness supports Linux hosts and Windows hosts with WSL".to_owned())
}

fn require_success(mut command: Command, operation: &str) -> Result<Output, String> {
    let output = command
        .output()
        .map_err(|error| format!("could not start {operation}: {error}"))?;

    if output.status.success() {
        return Ok(output);
    }

    let standard_error = String::from_utf8_lossy(&output.stderr);

    Err(format!(
        "{operation} failed with status {}: {}",
        output.status,
        standard_error.trim()
    ))
}

fn llvm_tool(root: &Path, name: &str) -> PathBuf {
    root.join("target")
        .join("toolchains")
        .join("llvm")
        .join("active")
        .join("bin")
        .join(executable_name(name))
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}
