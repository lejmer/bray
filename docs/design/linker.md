# Linker design

`bray-linker` turns a resolved native link or archive plan into staged artifacts. It owns driver selection, invocation,
and tool-specific behavior. The emitter owns final output policy and publication.

## Typed plans

The emitter constructs the linker-owned plan from completed artifacts and resolved product requirements. The plan
carries ordered inputs, target and product identity, entry and startup requirements, runtime components, native
dependencies, exports, debug companions, and staging destinations.

Source directives have already become typed requirements. The linker does not discover reachability, choose language
semantics, infer an entrypoint, or inspect MIR.

Static-library creation belongs to this domain alongside executable and shared-library linking. Target-required
companions remain explicit outputs in the same operation.

## Drivers and toolchains

Embedded, external, system, and archiver drivers implement a common coarse boundary. Each declares the target, input,
product, runtime, cancellation, and determinism capabilities it supports. Selection validates the complete plan rather
than relying on an executable name or a successful probe.

Compiler-host configuration supplies compatible tools. Platform compiler drivers can own their native startup and
C-runtime contract, while raw-linker plans must supply those requirements explicitly. Runtime artifacts carry their
ordered platform-library dependencies.

Driver and tool revisions participate in reproducibility identity. A supported cross-target plan does not imply that its
output can run on the current host.

## Invocation

An injectable process boundary receives an executable, explicit argument vector, environment, working directory, and any
driver-owned response files. It does not invoke a shell to interpret constructed text.

The driver owns tool syntax and deterministic encoding. The process boundary controls inherited environment and returns
typed host failures plus uninterpreted tool output. This compiler-host mechanism is separate from Bray's source-level
process APIs.

External processes share an explicit host process budget. A task retains its permit through termination and reaping.
Independent products can link concurrently without hidden worker pools or premature release of process capacity.

## Runtime and artifact boundaries

Runtime compatibility is selected upstream and validated against the link target. The linker consumes exact components
and host requirements instead of inferring runtime needs from unresolved symbols.

Synchronous cleanup-report support remains separate from the async scheduler. Standard-library native-thread and
child-process services arrive through their trusted product dependencies.

Semantic package interfaces accompany library products through the emitter but are not native linker inputs.

## Staging and publication

Drivers write only to emitter-owned staging destinations. Normalized destination identity supports collision checks even
when paths have different spellings.

A successful invocation returns validated output records. The emitter includes them in its product-generation
transaction. Invocation, validation, or cancellation failure cannot publish a new partial product, and completed pure
compiler inputs remain reusable.

## Determinism and diagnostics

Input order, arguments, response-file bytes, environment, and metadata follow the plan. Drivers normalize
nondeterministic tool metadata where supported and report limitations through capabilities where it is not.

Structured failures retain target, driver, product, artifact, and process context. Tool stdout and stderr remain labeled
external detail. Available symbol provenance can connect a link failure to a declaration without guessing from a mangled
name.

Cancellation stops supported child processes and prevents a cancelled result from being published. Staging cleanup and
final commit remain emitter responsibilities.

## Related documents

- [Emitter](emitter.md)
- [Code generation](codegen.md)
- [Async runtime](async-runtime.md)
- [Compiler diagnostics](compiler-diagnostics.md)
