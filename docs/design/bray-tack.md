# Bray Tack Build Tool

Bray Tack is the user-facing `bray` command. It owns project and workspace command policy while `bray-project` owns
manifest semantics and immutable project graphs. The compiler executable `brayc` remains a loose-file compiler boundary.

## Workspace Boundary

`bray --workspace <directory>` loads exactly `<directory>/bray-workspace.json`. The default directory is `.`. Bray Tack
does not search parent directories, inspect ambient package locations, or resolve missing dependencies.

Every project compiler request is derived from the already validated `ProjectGraph`. Package, product, target, source,
feature, and dependency selections are therefore explicit and deterministic before compiler work begins.

## Commands

- `bray init [directory]` creates a minimal workspace and root executable package without invoking any toolchain
  process. `--package <identity>` supplies the package identity explicitly. Otherwise, Bray Tack uses the directory
  name when it is a valid package identity.
- `bray check` requests diagnostics for selected manifest products. Required dependency interfaces are produced through
  `brayc` and retained in a deterministic workspace cache. Check does not publish product build outputs.
- `bray build` traverses selected products and their declared dependency products in dependency order, then emits only
  the artifact categories selected by each manifest.
- `bray run` selects exactly one executable product and target, builds it, and runs the published executable with the
  trailing arguments.
- `bray test` builds selected test products, discovers entries from their published test catalogs, and runs their native
  test hosts. Bray Tack owns cross-product filtering, resource budgets, scheduling, cancellation, and report
  aggregation. Each generated host owns entry invocation, per-test capture, timeout delivery, and cleanup completion
  through the shared [testing protocol](testing.md).
- `bray fmt` resolves the selected formatter configuration and routes explicit files, standard input (`-`), or the
  sorted root-package source graph to `brayfmt`.
- `bray inspect project` renders the immutable graph. Other inspection kinds select exactly one manifest product and
  delegate to existing compiler query inspection.
- `bray lsp` runs `bray-lsp` for the selected workspace and target and forwards the protocol streams without
  interpreting framed messages.
- `bray vendor install <name> <git-repository>` explicitly clones one repository beneath `vendor/<name>` with recursive
  submodule acquisition disabled.

Package, product, and target filters use `--package`, `--product`, and `--target`. Target names are workspace-local
manifest names rather than host inference.

## Build Configurations

Build, run, and test commands use the debug configuration by default. `--release` selects the release configuration. The
selected configuration applies to every product built by that command and is forwarded explicitly to `brayc` rather than
inferred from the compiler executable or host environment.

Debug builds use inexpensive optimization, emit source line tables, preserve otherwise unused linked content, and
request any debug companion required by the target. Release builds use the production optimization pipeline, omit debug
information, and permit dead-code and section removal. Both configurations preserve Bray language semantics.

Published product artifacts use one invariant directory beneath `output_root`:

```text
<output_root>/<workspace-target>/<debug|release>/<package>/<target-prefix><product><target-suffix>
```

For example, the native Windows debug executable for package `hello_world` and product `application` is always
`build/native/debug/hello_world/application.exe`. Automation can derive this path from the target output naming contract
without reading manifests, scanning directories, or discovering a generation identity. All compiler-private generations,
manifests, caches, and staging state remain beneath `<output_root>/.bray/`. Configuration separation prevents build,
run, and test commands from reusing or replacing artifacts from another configuration. Check and semantic inspection
remain configuration-independent.

Structured build results report every complete stable artifact path directly. Progress JSON also includes the full
stable product path as one field, while retaining the filename and directory fields used by terminal presentation. Run
and test hold the product's shared publication lock for the lifetime of native execution so a concurrent build cannot
expose a mixed companion set.

## Formatter Configuration

Formatting uses the formatter defaults when the workspace does not select a formatter configuration. The optional
`formatter_configuration` path in `bray-workspace.json` selects one workspace-relative configuration file, and
`bray fmt --config <path>` can explicitly override that selection for one invocation. Relative configuration paths are
resolved against the workspace directory rather than the process working directory.

Bray Tack owns configuration selection and path resolution, but it does not interpret formatting rules or reproduce
formatter policy. It passes the exact selected configuration path to `brayfmt`. The formatter executable owns decoding
and validating the configuration, resolving named rule overrides, applying rule parameters, and reporting structured
configuration diagnostics.

One `bray fmt` invocation applies one resolved configuration consistently to every selected source input, including
standard input. Bray Tack does not search parent directories, infer configuration from a source file's physical
location, or silently fall back to defaults when an explicitly selected configuration is invalid.

The formatter configuration can override the default 120-column width, disable any default rule, and enable rules that
are off by default. This includes explicitly enabled syntax rewrites such as `simplify-nested-if`. Bray Tack forwards
these choices without performing syntax or semantic analysis itself.

## Workflow Progress

Text-mode build workflows present the selected package product and configuration first, followed by one stable line for
every package that performs compiler work. A package line carries its package identity, workspace-relative path,
completed and total compiler invocations, and elapsed time. Interactive terminals update those lines in place through
coordinated spinners and progress bars. Redirected output emits only the final package summaries and therefore contains
no terminal control sequences or repeated transient states.

The default presentation treats checking, code generation, artifact production, and linking as one package-level
`Compiling` operation. `--verbose` exposes the exact compiler action requested for each package without changing the
underlying work. All visible vocabulary is rendered through `bray-messages`, while color and animation are presentation
concerns owned by Bray Tack.

```text
Building hello_world/application [debug]
   ✓ Compiled std               toolchain/standard-library      1/1 units  128 ms
   ✓ Compiled hello_world       examples/hello_world            1/1 units   94 ms
   ✓ Finished application.exe   build/native/debug/hello_world  2/2 units  247 ms
```

JSON output carries the same workflow records as structured data rather than terminal-rendered strings. Package
completion order remains deterministic even when compiler work becomes more parallel internally.

## Project Initialization

Initialization creates `bray-workspace.json`, `bray-package.json`, and `src/main.bray` beneath the selected directory.
The root package uses project path `.`, source root `src`, output root `build`, workspace target name `native`, and an
executable product named `application`. The target identity is the current supported native target.

Every generated file has deterministic UTF-8 bytes and follows the project manifest schema. The command rejects an
invalid package identity, an unsupported host target, a non-directory workspace root, and any existing generated-file
path. It never replaces source or manifest contents and does not invoke Git, acquire dependencies, compile source, or
build toolchain artifacts.

## Tool Integration

Bray Tack orchestrates independently installable toolchain executables rather than linking their implementations. It
invokes `brayc` for compilation and compiler inspection, `brayfmt` for source formatting, and `bray-lsp` for editor
protocol service. This boundary lets each executable depend only on the implementation it owns and allows the tools to
move into separate repositories without changing project orchestration.

Tool discovery first honors the explicit `BRAYC`, `BRAYFMT`, or `BRAY_LSP` environment override, then checks for a
sibling executable beside `bray`, and finally delegates to the host executable search path. Compiler requests carry
exact package, product, target, source, dependency-interface, and artifact selections. Formatter requests carry exact
source inputs, operation mode, and selected configuration path. Paths remain native process arguments rather than being
embedded in delimiter-based strings.

Machine-oriented compiler and formatter requests use structured JSON output. Bray Tack may combine multiple child
reports for one project command, but it does not recreate compiler diagnostics or interpret language-server protocol
messages.

## Toolchain Artifacts

Commands that request compiler queries select one immutable toolchain root. An explicit `--toolchain-root <directory>`
takes precedence over `BRAY_TOOLCHAIN_ROOT`. Otherwise, an installed `bray` beneath `<toolchain>/bin` selects
`<toolchain>`. A development executable outside a `bin` directory selects its containing directory, so development
workflows should normally supply the explicit option or environment variable.

The installed artifact layout is:

```text
<toolchain>/
├─ bin/
└─ lib/
   └─ bray/
      ├─ standard-library/
      └─ runtime/
         └─ <target>/
            ├─ bray-runtime.brayrt
            └─ <runtime archive>
```

The standard-library directory is the bundle root containing its normalized manifest and target-selected package
interfaces, implementation payloads, and native artifacts. Each runtime directory contains metadata and the archive
named by that metadata. Release assembly and development xtasks must publish this layout. Normal Bray Tack commands only
consume it and never compile toolchain source.

Every compiler request receives the selected standard-library root. The compilation resolver reads and validates its
manifest, target-selected interface and implementation, native artifacts, digests, target identity, and runtime ABI only
when the requested outputs demand them. Executable and test builds also receive the exact runtime metadata path for
their selected target. The compiler's existing runtime loader validates metadata, archive digest, capabilities, target,
panic ABI, and native link requirements before emission.

## Dependency Acquisition

Check, build, run, test, inspection, formatting, and language tooling never invoke Git or any network operation. Only
the explicit `vendor install` command invokes Git. It installs exactly the named repository and does not:

- update manifests,
- acquire dependencies named by the installed package,
- initialize submodules,
- solve versions,
- consult a registry,
- search ambient package locations,
- or publish packages.

The user remains responsible for declaring the installed package directory and exact dependency edge in project-owned
manifests.

## Target Selection

Bray Tack selects targets only by workspace-local manifest name and passes the declared exact target identity to the
compiler. It does not infer a target from the host or substitute another manifest target.

A command requiring compiler target properties proceeds only when the toolchain provides the declared target capability.
Otherwise it reports that target capability as unavailable without changing the project graph or falling back to ambient
configuration.
