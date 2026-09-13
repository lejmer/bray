# Bray Tack

Bray Tack is the user-facing `bray` command. It owns workspace command policy over the immutable graph supplied by
`bray-project`. The loose-file compiler remains `brayc`. Tool orchestration does not become a second compiler or project
model.

## Explicit selection

One workspace root determines the manifest boundary. Product, package, feature, source, target, and dependency choices
are explicit before compiler work starts. Targets come from the project graph rather than host inference. Initialization
may choose a supported native target when creating a new project, but ordinary commands consume its declared selection.

Normal commands consume installed inputs without network access. Dependency acquisition is an explicit vendor operation
separate from graph loading. Installing a repository does not implicitly resolve its dependencies or rewrite manifests.

## Tool boundaries

Tack invokes independently installable `brayc`, `brayfmt`, and `bray-lsp` executables. Requests carry exact native paths
and selected inputs. Structured compiler and formatter reports preserve their diagnostic data. The language-server
adapter forwards framed streams without interpreting editor protocol messages.

Toolchain discovery is command policy. Once selected, one immutable toolchain root supplies the standard library and
runtime metadata passed to compiler requests. Ordinary commands consume those artifacts, while development and release
automation build and assemble them.

Formatting follows the same ownership boundary. Tack selects source files and one configuration path for the invocation.
The formatter owns configuration decoding and rule interpretation. An explicitly invalid configuration cannot become a
silent request for defaults.

## Build and execution

Builds follow dependency order and use explicit configurations across all selected products. Public artifact paths are
stable and separated by target, configuration, and product. The emitter owns transactional generations behind those
paths. Command results report complete artifact locations so consumers need not inspect compiler-private storage.

Run and test pin the artifacts they execute. Test orchestration consumes immutable catalogs, owns global admission and
budgets, and delegates entry execution to generated hosts. Retained test runs validate complete build provenance before
using an existing generation, then share the ordinary scheduling and reporting path.

## Managed storage

Tack exposes the emitter's managed storage through inspection and cleanup commands. Publication retains bounded history,
while live readers and writers hold ownership locks. Optional cache eviction changes cost rather than product meaning.
Retention and cleanup operate only on indexed owned state and preserve active generations even when a user requests
cleanup explicitly.

Stable public outputs and immutable retained generations have different lifetimes. A public-artifact handle protects the
current companion set against replacement during use. A generation handle permits later publication while retaining the
older immutable files. Execution keeps the appropriate product and dependency handles until shutdown.

## Progress and reports

Workflow records identify package and product work independently of terminal presentation. Text renderers can update
interactive progress or emit stable redirected summaries. JSON projects the same structured records. Localized wording
belongs to `bray-messages`, and volatile timing remains distinct from reproducible compiler data.

## Related documents

- [Command reference](../tools/bray.md)
- [Project graphs](project-manifests.md)
- [Emission](emitter.md)
- [Testing](testing.md)
- [Formatter](formatter.md)
