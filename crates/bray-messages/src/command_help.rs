//! Canonical English help for Bray command-line surfaces.

/// Describes the Bray project command.
pub const TACK_ABOUT: &str = "Build, run, test, inspect, and maintain Bray projects";
/// Describes the standalone compiler command.
pub const COMPILER_ABOUT: &str = "Compile and inspect explicitly selected Bray source files";
/// Describes the standalone formatter command.
pub const FORMATTER_ABOUT: &str = "Format Bray source files or standard input";

/// Describes the project workspace option.
pub const WORKSPACE: &str = "Use DIRECTORY as the project workspace root";
/// Describes the toolchain-root option.
pub const TOOLCHAIN_ROOT: &str =
    "Load compiler, standard library, and runtime artifacts from DIRECTORY";
/// Describes the worker-count option.
pub const CPU_COUNT: &str = "Run compiler work with at most N worker threads; N must be positive";
/// Describes the output-format option.
pub const OUTPUT_FORMAT: &str = "Render command diagnostics and reports in the selected format";
/// Describes plain-text output.
pub const OUTPUT_TEXT: &str = "Human-readable terminal output";
/// Describes JSON output.
pub const OUTPUT_JSON: &str = "Locale-neutral structured JSON output";
/// Describes verbose project output.
pub const VERBOSE: &str = "Show compiler commands and additional project progress";
/// Describes compiler profiling.
pub const PROFILE: &str = "Collect compiler timing and unit statistics in MODE";
/// Describes summary profiling.
pub const PROFILE_SUMMARY: &str = "Print an aggregated human-readable profile";
/// Describes trace profiling.
pub const PROFILE_TRACE: &str = "Write a detailed machine-readable trace";
/// Describes profile output selection.
pub const PROFILE_OUTPUT: &str = "Write detailed profile reports under DIRECTORY";

/// Describes project initialization.
pub const INIT: &str = "Create a Bray project manifest and source layout";
/// Describes project checking.
pub const CHECK: &str =
    "Parse, bind, and type-check the selected product without emitting native artifacts";
/// Describes project building.
pub const BUILD: &str = "Compile the selected product and emit its configured artifacts";
/// Describes project execution.
pub const RUN: &str = "Build and execute the selected executable product";
/// Describes project tests.
pub const TEST: &str = "Build and execute tests selected from the project test catalog";
/// Describes project formatting.
pub const FORMAT: &str = "Format project source files or verify that they are already formatted";
/// Describes project inspection.
pub const INSPECT: &str = "Render a selected project or compiler representation";
/// Describes compiler profile reports.
pub const PROFILE_REPORT: &str = "Read and compare compiler profile reports";
/// Describes the language server.
pub const LANGUAGE_SERVER: &str = "Serve Bray language intelligence over standard input and output";
/// Describes dependency vendoring.
pub const VENDOR: &str = "Manage source dependencies stored in the workspace vendor directory";
/// Describes showing a profile.
pub const PROFILE_SHOW: &str = "Render one compiler profile report";
/// Describes comparing profiles.
pub const PROFILE_COMPARE: &str = "Compare two compatible compiler profile reports";
/// Describes dependency installation.
pub const VENDOR_INSTALL: &str = "Install a named Git dependency into the workspace";

/// Describes an optional initialization directory.
pub const INIT_DIRECTORY: &str = "Create the project in DIRECTORY instead of the workspace root";
/// Describes package identity selection.
pub const PACKAGE: &str = "Select package IDENTITY";
/// Describes product selection.
pub const PRODUCT: &str = "Select product NAME within the selected package";
/// Describes target selection.
pub const TARGET: &str = "Select target NAME or target triple";
/// Describes release configuration.
pub const RELEASE: &str = "Build with release optimization and release artifact paths";
/// Describes arguments passed to a program.
pub const PROGRAM_ARGUMENT: &str =
    "Pass ARG to the executed program; arguments after the command are not parsed by Bray";
/// Describes sequential tests.
pub const TEST_SEQUENTIAL: &str = "Run one test at a time";
/// Describes test concurrency.
pub const TEST_JOBS: &str = "Run at most N tests concurrently";
/// Describes test timeout.
pub const TEST_TIMEOUT: &str = "Fail a test that runs longer than MILLISECONDS";
/// Describes inherited test output.
pub const TEST_NO_CAPTURE: &str = "Let test processes write directly to the terminal";
/// Describes discarded test output.
pub const TEST_DISCARD_OUTPUT: &str = "Discard test output instead of capturing it";
/// Describes captured-output bounds.
pub const TEST_CAPTURE_LIMIT: &str = "Capture at most BYTES from each output stream per test";
/// Describes successful test output display.
pub const TEST_SHOW_OUTPUT: &str = "Show captured output for successful tests as well as failures";
/// Describes test filters.
pub const TEST_FILTER: &str =
    "Run tests whose qualified identity contains FILTER; multiple filters are combined";
/// Describes format verification.
pub const FORMAT_CHECK: &str = "Report files that would change without rewriting them";
/// Describes formatter configuration.
pub const FORMAT_CONFIG: &str = "Read formatter configuration from FILE";
/// Describes source-file inputs.
pub const SOURCE_FILE: &str =
    "Read Bray source from FILE; use - for standard input where supported";
/// Describes inspection representation selection.
pub const INSPECTION: &str = "Select the representation to render";
/// Describes a source identity used for inspection.
pub const SOURCE_ID: &str = "Inspect the loaded source with numeric identity N";
/// Describes a source byte offset used for inspection.
pub const SOURCE_OFFSET: &str =
    "Inspect the innermost program element containing UTF-8 byte OFFSET";
/// Describes profile report paths.
pub const PROFILE_REPORT_PATH: &str = "Read the compiler profile from REPORT";
/// Describes baseline profile paths.
pub const PROFILE_BEFORE: &str = "Use BEFORE as the baseline compiler profile";
/// Describes comparison profile paths.
pub const PROFILE_AFTER: &str = "Use AFTER as the comparison compiler profile";
/// Describes language-server target selection.
pub const LANGUAGE_SERVER_TARGET: &str = "Open project documents against target NAME";
/// Describes dependency names.
pub const VENDOR_NAME: &str = "Install the dependency under NAME";
/// Describes Git dependency locations.
pub const VENDOR_REPOSITORY: &str = "Clone the dependency from GIT_REPOSITORY";

/// Describes source inspection.
pub const INSPECT_SOURCE: &str = "Render normalized loaded source text";
/// Describes token inspection.
pub const INSPECT_TOKENS: &str = "Render lexical tokens";
/// Describes syntax inspection.
pub const INSPECT_SYNTAX: &str = "Render the parsed syntax tree";
/// Describes declaration inspection.
pub const INSPECT_DECLARATIONS: &str = "Render collected declarations";
/// Describes symbol inspection.
pub const INSPECT_SYMBOLS: &str = "Render the resolved symbol graph";
/// Describes bound-tree inspection.
pub const INSPECT_BOUND: &str = "Render the semantically bound tree";
/// Describes lowered-tree inspection.
pub const INSPECT_LOWERED: &str = "Render the lowered semantic representation";
/// Describes MIR inspection.
pub const INSPECT_MIR: &str = "Render mid-level intermediate representation";
/// Describes project inspection.
pub const INSPECT_PROJECT: &str = "Render the resolved project model";

/// Describes compiler output file selection.
pub const REPORT_OUTPUT: &str = "Write the inspection report to PATH instead of standard output";
/// Describes package semantic version selection.
pub const PACKAGE_VERSION: &str = "Compile package IDENTITY at semantic VERSION";
/// Describes source package authority selection.
pub const SOURCE_PACKAGE: &str = "Treat source declarations as owned by package IDENTITY";
/// Describes product kind selection.
pub const PRODUCT_KIND: &str = "Compile the selected product as this product kind";
/// Describes dependency product selection.
pub const DEPENDENCY_PRODUCT: &str =
    "Import product PACKAGE/PRODUCT; repeat in the same order as dependency paths";
/// Describes dependency interface paths.
pub const DEPENDENCY_INTERFACE: &str =
    "Load the corresponding dependency package interface from PATH";
/// Describes dependency implementation paths.
pub const DEPENDENCY_IMPLEMENTATION: &str =
    "Load the corresponding dependency implementation archive from PATH";
/// Describes compiler backend selection.
pub const BACKEND: &str = "Generate native code with the selected native-code generator";
/// Describes LLVM backend selection.
pub const BACKEND_LLVM: &str = "Generate native code through LLVM";
/// Describes an explicit runtime artifact.
pub const RUNTIME_ARTIFACT: &str = "Use runtime artifact metadata from METADATA";
/// Describes a runtime profile.
pub const RUNTIME_PROFILE: &str = "Select toolchain runtime profile PROFILE";
/// Describes runtime capability requirements.
pub const RUNTIME_CAPABILITY: &str = "Require the selected runtime to provide CAPABILITY";
/// Describes artifact output directories.
pub const BUILD_OUTPUT: &str = "Write build artifacts under DIRECTORY";
/// Describes test catalog output.
pub const TEST_CATALOG: &str = "Write the emitted test catalog to PATH";
/// Describes requested build artifacts.
pub const ARTIFACT: &str = "Emit ARTIFACT; repeat to request multiple artifact kinds";
/// Describes backend inspection artifacts.
pub const INSPECTION_ARTIFACT: &str = "Retain and report backend ARTIFACT for inspection";
/// Describes package interface output.
pub const EMIT_INTERFACE: &str = "Write the checked package interface to PATH";

/// Describes library products.
pub const PRODUCT_LIBRARY: &str = "A reusable library product";
/// Describes executable products.
pub const PRODUCT_EXECUTABLE: &str = "An executable application product";
/// Describes test products.
pub const PRODUCT_TEST: &str = "A test-host product";
/// Describes Linux GNU x86-64 target selection.
pub const TARGET_X86_64_LINUX_GNU: &str = "64-bit x86 Linux using the GNU platform ABI";
/// Describes Linux GNU AArch64 target selection.
pub const TARGET_AARCH64_LINUX_GNU: &str = "64-bit Arm Linux using the GNU platform ABI";
/// Describes Windows MSVC x86-64 target selection.
pub const TARGET_X86_64_WINDOWS_MSVC: &str = "64-bit x86 Windows using the MSVC platform ABI";
/// Describes Windows MSVC AArch64 target selection.
pub const TARGET_AARCH64_WINDOWS_MSVC: &str = "64-bit Arm Windows using the MSVC platform ABI";
/// Describes macOS x86-64 target selection.
pub const TARGET_X86_64_MACOS: &str = "64-bit x86 macOS";
/// Describes macOS AArch64 target selection.
pub const TARGET_AARCH64_MACOS: &str = "Apple silicon macOS";

/// Describes an assembly artifact.
pub const ARTIFACT_ASSEMBLY: &str = "Backend-generated assembly source";
/// Describes textual backend IR.
pub const ARTIFACT_BACKEND_IR: &str = "Backend textual intermediate representation";
/// Describes backend bitcode.
pub const ARTIFACT_BACKEND_BITCODE: &str = "Backend binary intermediate representation";
/// Describes a relocatable object.
pub const ARTIFACT_OBJECT: &str = "Relocatable native object file";
/// Describes an executable module.
pub const ARTIFACT_EXECUTABLE_MODULE: &str =
    "Native executable module before final product linking";
/// Describes a debug companion.
pub const ARTIFACT_DEBUG_COMPANION: &str = "Platform debug-information companion artifact";
/// Describes a package interface.
pub const ARTIFACT_PACKAGE_INTERFACE: &str = "Portable checked package interface";
/// Describes a package implementation.
pub const ARTIFACT_PACKAGE_IMPLEMENTATION: &str = "Native package implementation archive";
/// Describes dependency metadata.
pub const ARTIFACT_DEPENDENCY_METADATA: &str = "Metadata describing emitted dependency artifacts";
/// Describes an executable.
pub const ARTIFACT_EXECUTABLE: &str = "Final executable product";
/// Describes a static library.
pub const ARTIFACT_STATIC_LIBRARY: &str = "Final static library product";
/// Describes a shared library.
pub const ARTIFACT_SHARED_LIBRARY: &str = "Final shared library product";
/// Describes a linked companion artifact.
pub const ARTIFACT_LINKED_COMPANION: &str = "Platform companion emitted during final linking";

/// Describes memory runtime support.
pub const CAPABILITY_MEMORY: &str = "Allocation, deallocation, and raw memory operations";
/// Describes string runtime support.
pub const CAPABILITY_STRING: &str = "String storage and manipulation operations";
/// Describes character runtime support.
pub const CAPABILITY_CHARACTER: &str = "Unicode character operations";
/// Describes cooperative execution support.
pub const CAPABILITY_COOPERATIVE: &str = "Cooperative task scheduling";
/// Describes local execution lanes.
pub const CAPABILITY_LOCAL_LANES: &str = "Thread-affine execution lanes";
/// Describes migratable execution lanes.
pub const CAPABILITY_MIGRATABLE_LANES: &str = "Execution lanes that may migrate between workers";
/// Describes blocking execution lanes.
pub const CAPABILITY_BLOCKING_LANES: &str = "Execution lanes for blocking operations";
/// Describes compute execution lanes.
pub const CAPABILITY_COMPUTE_LANES: &str = "Execution lanes for compute-bound work";
/// Describes main-thread execution.
pub const CAPABILITY_MAIN_THREAD: &str = "Execution constrained to the process main thread";
/// Describes reactor support.
pub const CAPABILITY_REACTOR: &str = "Event-reactor integration";
