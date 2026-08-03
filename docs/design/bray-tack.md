# Bray Tack Build Tool

Bray Tack is the user-facing `bray` command. It owns project and workspace command policy while
`bray-project` owns manifest semantics and immutable project graphs. The compiler executable
`brayc` remains a loose-file compiler boundary.

## Workspace Boundary

`bray --workspace <directory>` loads exactly
`<directory>/bray-workspace.json`. The default directory is `.`. Bray Tack does not search parent
directories, inspect ambient package locations, or resolve missing dependencies.

Every project compiler request is derived from the already validated `ProjectGraph`. Package,
product, target, source, feature, and dependency selections are therefore explicit and
deterministic before compiler work begins.

## Commands

- `bray init [directory]` creates a minimal workspace and root executable package without invoking
  any toolchain process. `--package <identity>` supplies the package identity explicitly. Otherwise,
  Bray Tack uses the directory name when it is a valid package identity.
- `bray check` requests diagnostics for selected manifest products. Required dependency interfaces
  are produced through `brayc` and retained in a deterministic workspace cache. Check does not
  publish product build outputs.
- `bray build` traverses selected products and their declared dependency products in dependency
  order, then emits only the artifact categories selected by each manifest.
- `bray run` selects exactly one executable product and target, builds it, and runs the published
  executable with the trailing arguments.
- `bray test` builds selected test products and runs their published executables in deterministic
  graph order. Rich test discovery, scheduling, capture, and reports remain owned by the test
  runner.
- `bray fmt` routes explicit files, standard input (`-`), or the sorted root-package source graph
  to `brayfmt`.
- `bray inspect project` renders the immutable graph. Other inspection kinds select exactly one
  manifest product and delegate to existing compiler fact inspection.
- `bray language-server` runs `bray-lsp` for the selected workspace and target and forwards the
  protocol streams without interpreting framed messages.
- `bray vendor install <name> <git-repository>` explicitly clones one repository beneath
  `vendor/<name>` with recursive submodule acquisition disabled.

Package, product, and target filters use `--package`, `--product`, and `--target`. Target names are
workspace-local manifest names rather than host inference.

## Project Initialization

Initialization creates `bray-workspace.json`, `bray-package.json`, and `src/main.bray` beneath the
selected directory. The root package uses project path `.`, source root `src`, output root `build`,
workspace target name `native`, and an executable product named `application`. The target identity
is the current supported native target.

Every generated file has deterministic UTF-8 bytes and follows the project manifest schema. The
command rejects an invalid package identity, an unsupported host target, a non-directory workspace
root, and any existing generated-file path. It never replaces source or manifest contents and does
not invoke Git, acquire dependencies, compile source, or build toolchain artifacts.

## Tool Integration

Bray Tack orchestrates independently installable toolchain executables rather than linking their
implementations. It invokes `brayc` for compilation and compiler inspection, `brayfmt` for source
formatting, and `bray-lsp` for editor protocol service. This boundary lets each executable depend
only on the implementation it owns and allows the tools to move into separate repositories without
changing project orchestration.

Tool discovery first honors the explicit `BRAYC`, `BRAYFMT`, or `BRAY_LSP` environment override,
then checks for a sibling executable beside `bray`, and finally delegates to the host executable
search path. Compiler requests carry exact package, product, target, source, dependency-interface,
and artifact selections. Paths remain native process arguments rather than being embedded in
delimiter-based strings.

Machine-oriented compiler and formatter requests use structured JSON output. Bray Tack may combine
multiple child reports for one project command, but it does not recreate compiler diagnostics or
interpret language-server protocol messages.

## Toolchain Artifacts

Commands that request compiler facts select one immutable toolchain root. An explicit
`--toolchain-root <directory>` takes precedence over `BRAY_TOOLCHAIN_ROOT`. Otherwise, an installed
`bray` beneath `<toolchain>/bin` selects `<toolchain>`. A development executable outside a `bin`
directory selects its containing directory, so development workflows should normally supply the
explicit option or environment variable.

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

The standard-library directory is the bundle root containing its canonical manifest, public
package interface, and target artifacts. Each runtime directory contains metadata and the archive
named by that metadata. Release assembly and development xtasks must publish this layout. Normal
Bray Tack commands only consume it and never compile toolchain source.

Every compiler request receives the selected standard-library root. The compilation resolver reads
and validates its manifest, interface, target artifacts, digests, target identity, and runtime ABI
only when the requested facts demand them. Executable and test builds also receive the exact runtime
metadata path for their selected target. The compiler's existing runtime loader validates metadata,
archive digest, capabilities, target, panic ABI, and native link requirements before emission.

## Dependency Acquisition

Check, build, run, test, inspection, formatting, and language tooling never invoke Git or any
network operation. Only the explicit `vendor install` command invokes Git. It installs exactly the
named repository and does not:

- update manifests,
- acquire dependencies named by the installed package,
- initialize submodules,
- solve versions,
- consult a registry,
- search ambient package locations,
- or publish packages.

The user remains responsible for declaring the installed package directory and exact dependency
edge in project-owned manifests.

## Target Selection

Bray Tack selects targets only by workspace-local manifest name and passes the declared exact
target identity to the compiler. It does not infer a target from the host or substitute another
manifest target.

A command requiring compiler target facts proceeds only when the toolchain provides the declared
target capability. Otherwise it reports that target capability as unavailable without changing
the project graph or falling back to ambient configuration.
