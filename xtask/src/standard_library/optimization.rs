use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_native_artifact::{NativeUnitSummary, scan_bitcode_unit_summary};
use bray_target::NativeTarget;
use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;
use rayon::prelude::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};

use super::command::BuildError;

pub(super) struct BuiltNativeModules {
    pub(super) units: Vec<(NativeUnitSummary, Vec<u8>)>,
    pub(super) triple: String,
    pub(super) data_layout: String,
}

pub(super) fn from_bray_modules(
    root: &Path,
    work: &Path,
    modules: impl IntoIterator<Item = Vec<u8>>,
) -> Result<BuiltNativeModules, BuildError> {
    prepare_units(root, work, modules, None, true)
}

pub(super) fn from_native_archive(
    root: &Path,
    work: &Path,
    archive: &Path,
    target: NativeTarget,
) -> Result<Option<BuiltNativeModules>, BuildError> {
    let modules = bitcode_members(root, archive)?;

    if modules.is_empty() {
        return Ok(None);
    }

    prepare_units(root, work, modules, Some(target.as_str()), false).map(Some)
}

fn prepare_units(
    root: &Path,
    work: &Path,
    modules: impl IntoIterator<Item = Vec<u8>>,
    target_triple: Option<&str>,
    compiler_summarized: bool,
) -> Result<BuiltNativeModules, BuildError> {
    let staging = tempfile::Builder::new()
        .prefix("bray-thin-lto-")
        .tempdir_in(work)
        .map_err(BuildError::TemporaryDirectory)?;

    let mut modules: Vec<_> = modules.into_iter().collect();
    modules.sort_unstable();

    let mut units = Vec::with_capacity(modules.len());
    let mut contract = None;

    let prepare = |index, bytes| {
        prepare_module(root, staging.path(), index, bytes, target_triple, compiler_summarized)
    };

    let prepared = if compiler_summarized {
        modules.into_par_iter().enumerate().map(|(index, bytes)| prepare(index, bytes)).collect::<Vec<_>>()
    } else {
        modules.into_iter().enumerate().map(|(index, bytes)| prepare(index, bytes)).collect::<Vec<_>>()
    };

    for item in prepared {
        let (output, module_contract, summary) = item?;

        if contract
            .as_ref()
            .is_some_and(|expected| expected != &module_contract)
        {
            return Err(BuildError::NativeArchive(
                "optimization modules have incompatible target contracts".to_owned(),
            ));
        }

        contract.get_or_insert(module_contract);
        let bytes = fs::read(&output).map_err(|error| BuildError::read(&output, error))?;
        units.push((summary, bytes));
    }

    let (triple, data_layout) = contract.ok_or_else(|| {
        BuildError::NativeArchive("optimization module target is unavailable".to_owned())
    })?;

    Ok(BuiltNativeModules {
        units,
        triple,
        data_layout,
    })
}

fn prepare_module(
    root: &Path,
    staging: &Path,
    index: usize,
    bytes: Vec<u8>,
    target_triple: Option<&str>,
    compiler_summarized: bool,
) -> Result<(PathBuf, (String, String), NativeUnitSummary), BuildError> {
    if !is_llvm_bitcode(&bytes) {
        let signature = bytes
            .iter()
            .take(4)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();

        return Err(BuildError::NativeArchive(format!(
            "optimization module {index} starts with 0x{signature} instead of LLVM bitcode magic"
        )));
    }

    let output = staging.join(format!("module_{index:04}.bc"));

    if compiler_summarized {
        fs::write(&output, &bytes).map_err(|error| BuildError::write(&output, error))?;
    } else {
        let input = staging.join(format!("input_{index:04}.bc"));

        fs::write(&input, &bytes).map_err(|error| BuildError::write(&input, error))?;
        summarize_module(root, &input, &output, target_triple)?;
    }

    let summarized = if compiler_summarized {
        bytes
    } else {
        fs::read(&output).map_err(|error| BuildError::read(&output, error))?
    };

    let contract = inspect_module_contract(root, &summarized, compiler_summarized)?;
    let summary = native_unit_summary(root, &output)?;

    Ok((output, contract, summary))
}

fn native_unit_summary(root: &Path, module: &Path) -> Result<NativeUnitSummary, BuildError> {
    let inspect = |tool: &str, arguments: &[&str]| {
        let output = Command::new(bray_llvm_toolchain::tool_path(root, tool))
            .args(arguments)
            .arg(module)
            .output()
            .map_err(|error| BuildError::ToolLaunch {
                action: "inspect optimization module",
                program: bray_llvm_toolchain::tool_path(root, tool),
                source: error,
            })?;

        if !output.status.success() {
            return Err(BuildError::NativeArchive(format!(
                "LLVM could not inspect optimization module {} with {tool}",
                module.display(),
            )));
        }

        String::from_utf8(output.stdout).map_err(|_| {
            BuildError::NativeArchive(format!(
                "LLVM returned non-UTF-8 optimization symbols for {}",
                module.display(),
            ))
        })
    };

    let symbols = inspect("llvm-nm", &["--format=posix", "--extern-only"])?;
    let structure = inspect("llvm-dis", &["-o", "-"])?;

    Ok(scan_bitcode_unit_summary(&symbols, &structure))
}

fn summarize_module(
    root: &Path,
    input: &Path,
    output: &Path,
    target_triple: Option<&str>,
) -> Result<(), BuildError> {
    let canonical = canonicalize_module(root, root, input)?;
    let mut summarize = Command::new(bray_llvm_toolchain::tool_path(root, "opt"));

    summarize
        .arg("-module-summary")
        .arg(canonical)
        .arg("-o")
        .arg(output);

    if let Some(target_triple) = target_triple {
        summarize.arg(format!("-mtriple={target_triple}"));
    }

    require_success(summarize, "LLVM could not create a module summary")?;

    let mut verify = Command::new(bray_llvm_toolchain::tool_path(root, "opt"));

    verify
        .args(["-passes=verify", "-disable-output"])
        .arg(output);

    require_success(verify, "LLVM rejected an optimization module")
}

fn canonicalize_module(
    root: &Path,
    checkout_root: &Path,
    input: &Path,
) -> Result<PathBuf, BuildError> {
    let llvm_ir = input.with_extension("ll");
    let canonical = input.with_extension("canonical.bc");
    let mut disassemble = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-dis"));

    disassemble.arg(input).arg("-o").arg(&llvm_ir);

    require_success(
        disassemble,
        "LLVM could not canonicalize an optimization module",
    )?;

    let llvm_ir_text =
        fs::read_to_string(&llvm_ir).map_err(|error| BuildError::read(&llvm_ir, error))?;

    let llvm_ir_text = remap_checkout_path(&llvm_ir_text, checkout_root);

    if contains_checkout_path(&llvm_ir_text, checkout_root) {
        return Err(BuildError::NativeArchive(
            "optimization module retains the checkout path".to_owned(),
        ));
    }

    fs::write(&llvm_ir, llvm_ir_text).map_err(|error| BuildError::write(&llvm_ir, error))?;

    let mut assemble = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-as"));

    assemble.arg(&llvm_ir).arg("-o").arg(&canonical);

    require_success(
        assemble,
        "LLVM could not assemble a canonical optimization module",
    )?;

    Ok(canonical)
}

fn remap_checkout_path(llvm_ir: &str, root: &Path) -> String {
    let native = root.to_string_lossy();
    let slash = native.replace('\\', "/");
    let escaped = native.replace('\\', "\\\\");
    let llvm_escaped = native.replace('\\', "\\5C");

    [
        escaped.as_str(),
        llvm_escaped.as_str(),
        slash.as_str(),
        native.as_ref(),
    ]
    .into_iter()
    .fold(llvm_ir.to_owned(), |normalized, spelling| {
        normalized.replace(spelling, ".")
    })
}

fn contains_checkout_path(llvm_ir: &str, root: &Path) -> bool {
    let native = root.to_string_lossy();
    let slash = native.replace('\\', "/");
    let escaped = native.replace('\\', "\\\\");
    let llvm_escaped = native.replace('\\', "\\5C");

    [native.as_ref(), &slash, &escaped, &llvm_escaped]
        .into_iter()
        .any(|spelling| llvm_ir.contains(spelling))
}

fn inspect_module_contract(
    root: &Path,
    bytes: &[u8],
    check_checkout_path: bool,
) -> Result<(String, String), BuildError> {
    let mut terminated = Vec::with_capacity(bytes.len() + 1);
    terminated.extend_from_slice(bytes);
    terminated.push(0);

    let context = Context::create();
    let buffer = MemoryBuffer::create_from_memory_range(&terminated, "optimization module");

    let module = Module::parse_bitcode_from_buffer(&buffer, &context).map_err(|error| {
        BuildError::NativeArchive(format!("LLVM rejected an optimization module: {error}"))
    })?;

    let triple = module.get_triple().as_str().to_string_lossy().into_owned();
    let data_layout = module.get_data_layout().as_str().to_string_lossy().into_owned();

    if triple.is_empty() || data_layout.is_empty() {
        return Err(BuildError::NativeArchive(
            "optimization module target is incomplete".to_owned(),
        ));
    }

    if check_checkout_path && contains_checkout_path(&module.print_to_string().to_string(), root) {
        return Err(BuildError::NativeArchive(
            "compiler-produced optimization module retains the checkout path".to_owned(),
        ));
    }

    Ok((triple, data_layout))
}

fn bitcode_members(root: &Path, archive: &Path) -> Result<Vec<Vec<u8>>, BuildError> {
    let archiver = bray_llvm_toolchain::tool_path(root, "llvm-ar");

    let output = Command::new(&archiver)
        .arg("t")
        .arg(archive)
        .output()
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    if !output.status.success() {
        return Err(BuildError::NativeArchive(
            "LLVM could not inspect a native provider archive".to_owned(),
        ));
    }

    let members = String::from_utf8(output.stdout).map_err(|_| {
        BuildError::NativeArchive("native archive inventory is not UTF-8".to_owned())
    })?;

    let mut bitcode = Vec::new();

    for member in members.lines().filter(|member| !member.is_empty()) {
        if !is_optimization_member(member) {
            continue;
        }

        let output = Command::new(&archiver)
            .arg("p")
            .arg(archive)
            .arg(member)
            .output()
            .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

        if !output.status.success() {
            return Err(BuildError::NativeArchive(
                "LLVM could not read a native provider archive member".to_owned(),
            ));
        }

        if is_llvm_bitcode(&output.stdout) {
            bitcode.push(output.stdout);
        }
    }

    Ok(bitcode)
}

fn is_optimization_member(member: &str) -> bool {
    !member.contains(".rcgu.") || member.starts_with("bray_")
}

fn is_llvm_bitcode(bytes: &[u8]) -> bool {
    bytes.starts_with(b"BC\xc0\xde") || bytes.starts_with(b"\xde\xc0\x17\x0b")
}

fn require_success(mut command: Command, failure: &'static str) -> Result<(), BuildError> {
    let output = command.output().map_err(|source| BuildError::ToolLaunch {
        action: failure,
        program: PathBuf::from(command.get_program()),
        source,
    })?;

    if output.status.success() {
        return Ok(());
    }

    let detail = String::from_utf8_lossy(&output.stderr);

    Err(BuildError::NativeArchive(format!(
        "{failure}: {}",
        detail.trim()
    )))
}

pub(super) fn verify_relocated_native_modules(
    root: &Path,
    scratch: &Path,
    target: NativeTarget,
) -> Result<(), BuildError> {
    let mut modules = Vec::new();

    for directory_name in ["relocated-first", "relocated-second"] {
        let directory = scratch.join(directory_name);
        let source = directory.join("provider.c");
        let module = directory.join("provider.bc");

        fs::create_dir_all(&directory).map_err(|error| BuildError::write(&directory, error))?;

        fs::write(&source, "int bray_relocated_provider(void) { return 1; }\n")
            .map_err(|error| BuildError::write(&source, error))?;

        let mut compile = Command::new(bray_llvm_toolchain::tool_path(root, "clang"));

        compile
            .arg(format!("--target={}", target.as_str()))
            .args(crate::native_archive::thin_lto_arguments(&directory))
            .args(["-g", "-emit-llvm", "-c"])
            .arg(&source)
            .arg("-o")
            .arg(&module);

        require_success(
            compile,
            "LLVM could not build the relocated optimization fixture",
        )?;

        let canonical = canonicalize_module(root, &directory, &module)?;

        modules.push(fs::read(&canonical).map_err(|error| BuildError::read(&canonical, error))?);
    }

    if modules.windows(2).any(|pair| pair[0] != pair[1]) {
        return Err(BuildError::NativeArchive(
            "optimization module identity depends on the checkout path".to_owned(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;
    use inkwell::targets::{TargetData, TargetTriple};

    use super::{
        contains_checkout_path, is_llvm_bitcode, is_optimization_member, remap_checkout_path,
    };

    #[test]
    fn bitcode_detection_accepts_raw_and_wrapped_llvm_modules() {
        assert!(is_llvm_bitcode(b"BC\xc0\xdepayload"));
        assert!(is_llvm_bitcode(b"\xde\xc0\x17\x0bpayload"));
        assert!(!is_llvm_bitcode(b"native object"));
    }

    #[test]
    fn tool_launch_failures_retain_the_operation_program_and_io_cause() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("nonexistent-llvm-ar");

        let error = super::require_success(
            std::process::Command::new(&program),
            "create optimization archive",
        )
        .unwrap_err();

        assert!(error.to_string().contains("create optimization archive"));

        assert!(
            matches!(error, super::BuildError::ToolLaunch { program: actual, source, .. }
            if actual == program && source.kind() == std::io::ErrorKind::NotFound)
        );
    }

    #[test]
    fn native_optimization_keeps_native_and_first_party_rust_modules() {
        assert!(is_optimization_member("provider.o"));

        assert!(is_optimization_member(
            "bray_platform_abi_temporal.hash-cgu.0.rcgu.o"
        ));

        assert!(!is_optimization_member("std.hash-cgu.0.rcgu.o"));
    }

    #[test]
    fn llvm_module_contract_is_read_in_process() {
        let context = Context::create();
        let module = context.create_module("inspection.test");
        module.set_triple(&TargetTriple::create("x86_64-pc-windows-msvc"));
        module.set_data_layout(&TargetData::create("e-p:64:64").get_data_layout());
        module.add_global(context.i32_type(), None, "llvm.global_ctors");
        module.add_global(context.i32_type(), None, "llvm.global_dtors");
        module.add_function("__cxa_atexit", context.i32_type().fn_type(&[], false), None);

        let buffer = module.write_bitcode_to_memory();
        let bytes = buffer.as_slice().strip_suffix(&[0]).unwrap();

        let contract = super::inspect_module_contract(
            std::path::Path::new("C:\\workspace\\bray"), bytes, false,
        )
        .unwrap();

        assert_eq!(
            contract,
            ("x86_64-pc-windows-msvc".to_owned(), "e-p:64:64-i128:128".to_owned()),
        );
    }

    #[test]
    fn checkout_path_detection_covers_native_and_llvm_escaped_paths() {
        let root = std::path::Path::new("C:\\workspace\\bray");

        assert!(contains_checkout_path(
            "source_filename = \"C:\\\\workspace\\\\bray\\\\source.cpp\"",
            root,
        ));

        assert!(contains_checkout_path(
            "!DIFile(filename: \"source.cpp\", directory: \"C:\\5Cworkspace\\5Cbray\")",
            root,
        ));

        assert!(!contains_checkout_path(
            "source_filename = \"./source.cpp\"",
            root,
        ));

        assert_eq!(
            remap_checkout_path(
                "source_filename = \"C:\\\\workspace\\\\bray\\\\source.cpp\"",
                root,
            ),
            "source_filename = \".\\\\source.cpp\""
        );
    }

}
