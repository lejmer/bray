# Bray Tack Project Driver

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

- `bray check` requests diagnostics for selected manifest products. Required dependency interfaces
  are produced through the lazy compilation export fact and retained in memory. Check does not
  publish build outputs.
- `bray build` traverses selected products and their declared dependency products in dependency
  order, then emits only the artifact categories selected by each manifest.
- `bray run` selects exactly one executable product and target, builds it, and runs the published
  executable with the trailing arguments.
- `bray test` builds selected test products and runs their published executables in deterministic
  graph order. Rich test discovery, scheduling, capture, and reports remain owned by the test
  runner.
- `bray fmt` routes explicit files, standard input (`-`), or the sorted root-package source graph
  to the linked formatter service.
- `bray inspect project` renders the immutable graph. Other inspection kinds select exactly one
  manifest product and delegate to existing compiler fact inspection.
- `bray language-server` supplies the validated graph and worker budget to the linked language
  server service.
- `bray vendor install <name> <git-repository>` explicitly clones one repository beneath
  `vendor/<name>` with recursive submodule acquisition disabled.

Package, product, and target filters use `--package`, `--product`, and `--target`. Target names are
workspace-local manifest names rather than host inference.

## Tool Integration

Formatter and language-server implementations are linked through the narrow `TackFormatService`
and `TackLanguageServerService` contracts in the `bray` package. A binary without the owning service
reports that capability as unavailable. Bray Tack does not contain substitute formatting or
language-server logic.

The formatter request distinguishes check and write modes and distinguishes file inputs from
standard-input bytes. The language-server request receives shared ownership of the immutable
project graph and the selected compiler worker budget. The language-server service receives the
process input and output streams as its protocol transport without routing framed messages through
ordinary command output.

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
