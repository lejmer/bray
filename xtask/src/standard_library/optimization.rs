use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::NativeTarget;
use inkwell::context::Context;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;

use super::command::BuildError;

pub(super) fn from_native_archive(
    root: &Path,
    work: &Path,
    archive: &Path,
    target: NativeTarget,
) -> Result<Option<Vec<u8>>, BuildError> {
    let bytes = fs::read(archive).map_err(|error| BuildError::read(archive, error))?;

    let archive = object::read::archive::ArchiveFile::parse(bytes.as_slice())
        .expect("foreign producer archive must parse");

    let members = archive
        .members()
        .map(|member| {
            member
                .expect("foreign producer archive member must parse")
                .data(bytes.as_slice())
                .expect("foreign producer archive member must be in bounds")
                .to_vec()
        })
        .collect::<Vec<_>>();

    if !members.iter().any(|member| is_llvm_bitcode(member)) {
        return Ok(None);
    }

    let members = prepare_units(root, work, members, target)?;

    crate::native_archive::archive_bytes(
        &bray_llvm_toolchain::tool_path(root, "llvm-ar"),
        &members,
        "o",
    )
    .map(Some)
    .map_err(|error| BuildError::NativeArchive(error.to_string()))
}

fn prepare_units(
    root: &Path,
    work: &Path,
    mut modules: Vec<Vec<u8>>,
    target: NativeTarget,
) -> Result<Vec<Vec<u8>>, BuildError> {
    let staging = tempfile::Builder::new()
        .prefix("bray-thin-lto-")
        .tempdir_in(work)
        .map_err(BuildError::TemporaryDirectory)?;

    let mut contract = None;

    for (index, bytes) in modules.iter_mut().enumerate() {
        if !is_llvm_bitcode(&bytes) {
            continue;
        }

        let (output, module_contract) = prepare_module(root, staging.path(), index, bytes, target)?;

        if contract
            .as_ref()
            .is_some_and(|expected| expected != &module_contract)
        {
            return Err(BuildError::NativeArchive(
                "optimization modules have incompatible target contracts".to_owned(),
            ));
        }

        contract.get_or_insert(module_contract);

        *bytes = fs::read(&output).map_err(|error| BuildError::read(&output, error))?;
    }

    Ok(modules)
}

fn prepare_module(
    root: &Path,
    staging: &Path,
    index: usize,
    bytes: &[u8],
    target: NativeTarget,
) -> Result<(PathBuf, (String, String)), BuildError> {
    let output = staging.join(format!("module_{index:04}.bc"));

    let input = staging.join(format!("input_{index:04}.bc"));

    fs::write(&input, &bytes).map_err(|error| BuildError::write(&input, error))?;

    summarize_module(root, &input, &output, target.as_str())?;

    let summarized = fs::read(&output).map_err(|error| BuildError::read(&output, error))?;
    let contract = inspect_module_contract(root, &summarized, true)?;

    Ok((output, contract))
}

fn summarize_module(
    root: &Path,
    input: &Path,
    output: &Path,
    target_triple: &str,
) -> Result<(), BuildError> {
    let canonical = canonicalize_module(root, input)?;
    let mut summarize = Command::new(bray_llvm_toolchain::tool_path(root, "opt"));

    summarize
        .args(["-module-summary", "-module-hash"])
        .arg(canonical)
        .arg("-o")
        .arg(output);

    summarize.arg(format!("-mtriple={target_triple}"));

    require_success(summarize, "LLVM could not create a module summary")?;

    let mut verify = Command::new(bray_llvm_toolchain::tool_path(root, "opt"));

    verify
        .args(["-passes=verify", "-disable-output"])
        .arg(output);

    require_success(verify, "LLVM rejected an optimization module")
}

fn canonicalize_module(checkout_root: &Path, input: &Path) -> Result<PathBuf, BuildError> {
    let canonical = input.with_extension("canonical.bc");
    let context = Context::create();

    let buffer = MemoryBuffer::create_from_file(input)
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    let module = Module::parse_bitcode_from_buffer(&buffer, &context)
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    let mut llvm_ir_text =
        remap_checkout_path(&module.print_to_string().to_string(), checkout_root);

    if contains_checkout_path(&llvm_ir_text, checkout_root) {
        return Err(BuildError::NativeArchive(
            "optimization module retains the checkout path".to_owned(),
        ));
    }

    llvm_ir_text.push('\0');

    let buffer =
        MemoryBuffer::create_from_memory_range(llvm_ir_text.as_bytes(), "canonical module");

    let module = context
        .create_module_from_ir(buffer)
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

    fs::write(&canonical, bray_codegen_llvm::bitcode_bytes(&module))
        .map_err(|error| BuildError::write(&canonical, error))?;

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
    let context = Context::create();

    let module = bray_codegen_llvm::parse_bitcode(bytes, &context).map_err(|error| {
        BuildError::NativeArchive(format!("LLVM rejected an optimization module: {error:?}"))
    })?;

    let triple = module.get_triple().as_str().to_string_lossy().into_owned();

    let data_layout = module
        .get_data_layout()
        .as_str()
        .to_string_lossy()
        .into_owned();

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

        let canonical = canonicalize_module(&directory, &module)?;

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

    use super::{contains_checkout_path, is_llvm_bitcode, remap_checkout_path};

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
            std::path::Path::new("C:\\workspace\\bray"),
            bytes,
            false,
        )
        .unwrap();

        assert_eq!(
            contract,
            (
                "x86_64-pc-windows-msvc".to_owned(),
                "e-p:64:64-i128:128".to_owned()
            ),
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
