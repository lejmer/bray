# Native runtime issue drafts

## Status

This file provides a worked runtime issue for reviewing the [issue-writing plan](bray-native-runtime-issue-plan.md). It is a proposed replacement for BRA-555, not its published Linear description. The remaining issue bodies and exact dependency graph still need authoring. Do not implement from this draft while its publication prerequisites remain unresolved.

## BRA-555 proposed metadata

| Field | Proposed value |
| --- | --- |
| Title | Implement independently loaded provider ownership in Bray |
| Label | Feature |
| Assignee | Requesting user, resolved through Linear at publication |
| Project | Execution Runtime |
| Parent | BRA-547 |
| Native prerequisites | Synchronous product completion, and BRA-502 for physical mandatory cleanup backing |
| Related issues | BRA-501 for historical fixtures, BRA-556 for static cleanup, BRA-558 for reporting |

The new synchronous completion issue must have an actual ID before publication. Revalidate its exported host and shutdown contracts and the admission contract before replacing this draft's metadata with Linear relationships. Verify BRA-501's PR state independently. Its useful fixtures do not authorize code-only retention.

## Proposed issue body

### Outcome

Complete provider formation and retained ownership for independently loaded Bray libraries over the Bray host and cleanup service. Each load has its own activation identity. A surviving report, symbol, callback or borrowed value retains the provider's code and product statics together. Final release follows dependent disposal and the last entered callback's return.

Move all current dynamic formation, retention, lookup, retirement and release callers to this owner. Delete their replaced Rust registry and retention policy. Do not translate Rust's process-wide map, `Arc<RetainedRuntime>` or lock boundaries into a required Bray representation.

### Implementation contract

Implement linked project-owned policy in Bray. Use the existing Rust code as a caller and behavior inventory. Compiler and external test coordination may remain Rust. Documented system ABI dependencies and the pinned temporal provider remain permitted.

Deliver the complete loaded-provider contract in this issue. A Bray wrapper over Rust formation, a second registry or a success-only implementation does not complete it. Preserve execution-speed, binary-size and compilation-speed goals. Record necessary intermediate costs with the enabling dependency and later measurement owner.

Before coding, record the contract, live consumers, ownership hypothesis, relevant Bray features, expected removals and blockers in a short repository task record. Re-read it after compaction.

### Baseline and required reading

The research inventory uses source revision `a65cc2f0`. Revalidate every listed caller against the implementation baseline and record that revision before edits. Read these sources explicitly:

- `AGENTS.md`, `.agents/skills/use-bray-language/SKILL.md` and `.agents/skills/unslop/SKILL.md`.
- [Selected design](https://github.com/lejmer/bray/blob/c3a8765e/docs/wip/bray-native-runtime-design.md), sections Ownership and authority, Product hosts and service domains, Provider lifetime and static cleanup, Cleanup storage and admission, and Shutdown ownership and lifecycle.
- [Consumer inventory](https://github.com/lejmer/bray/blob/c3a8765e/docs/wip/bray-native-runtime-consumer-inventory.md), product hosts, provider lifetime, native boundaries and fixture sections.
- [Cost analysis](https://github.com/lejmer/bray/blob/c3a8765e/docs/wip/bray-native-runtime-cost-analysis.md), host lifetime and retained-provider costs.
- `docs/language/ownership-and-borrowing/dependency-contracts.md` and `contract-and-borrow-validity.md` in that directory.
- `docs/language/declarations/visibility-and-reachability.md`.
- `docs/language/contracts-and-trust/contract-reasoning.md`, `obligation-propagation.md` and `execution-guarantees.md` in that directory.
- `docs/language/declarations/static-storage-declarations.md` and `docs/language/async-and-concurrency/execution-roots-and-product-shutdown.md` after their explicitly tracked documentation alignment.
- `docs/language/targets-layout-abi-and-raw-memory/foreign-data-and-symbols.md`, `extern-declarations-and-ffi.md` and `uninitialized-storage-and-anchored-borrows.md` in that directory.
- `docs/language/lifecycle/finalization.md` and `docs/language/async-and-concurrency/low-level-runtime.md`.

The design documents must be available on the implementation baseline before this issue becomes ready. The documentation prerequisite replaces old mandatory-wait and code-only retention wording. Existing finalization and execution guarantees remain the source lifecycle mechanism. Report any unresolved source conflict before implementing a different rule.

### Behavior and ownership

Formation validates demanded bindings, the stable shared cleanup service domain and dynamic dependency cycles before publishing entry availability. Secure host and mandatory cleanup backing before accepting ownership. Formation does not run lazy user initializers. Failure preserves caller inputs and rolls back only accepted local state through the services it retained.

Two loads of the same artifact have independent identities, static domains and lifecycle state. Addresses, descriptor equality and artifact hashes cannot merge them. Static archives and imported MIR contribute to their final consumer product rather than forming artificial loaded instances.

Owned transfers preserve the stable service binding and existing backing. Do not rebind to the active runtime or a provider-local fallback. The resident service owner outlives provider callbacks and all transferred owners that use its backing.

Whole-product retention covers reports and typed errors, symbols and callbacks, and borrowed provider data. Keep code and statics live together. Entered callbacks retain their context through return. Stop new entry before retiring the load. Invalidate its external identity before releasing the image so a reload cannot authorize a stale handle.

A blocked graceful shutdown preserves the caller's shutdown owner and returns without waiting for a dependency held by that caller. Resident fallback retains the product and terminal cleanup state. It uses admitted storage and compatible continued execution after the caller leaves. Exactly one terminal owner performs cleanup. Static-owned edges in the teardown set do not create a self-wait.

Existing Bray static cleanup handles dependency order and exact-thread obligations. Provider release must keep that machinery and required services live until disposal completes. Continue independent cleanup after a failure. Preserve primary and suppressed incident order, typed-error destruction and backing release as distinct obligations.

Independent diagnostic history is ordinary host-owned data copied through existing library operations. Dispose the original provider-dependent report to permit provider cleanup. Keeping a typed payload or another provider dependency deliberately keeps the whole provider live. Do not add a diagnostic framework to this issue.

### Bray features and compiler boundary

Use ordinary owners and inferred dependencies for leases and transferred backing. Use typed borrows where an enclosing host already proves lifetime. Do not require a retained handle lookup for every internal call. Explicit callback state must stop new callbacks and drain entered calls before freeing it.

`Uninit<T>` and anchored views are candidates for prepublication storage and initialization epochs. Storage policy projection must meet its pure and total contract. Synchronization guards end before user callbacks, suspension and disposal.

Declarations and fields are public by default. Internal-use acknowledgement does not establish authority. Construction, callable requirements and lifecycle contracts must preserve validity against safe source access and mutation. Test stale identities and invalidated guarantees through the supported boundary instead of relying on private fields.

The runtime consumes immutable product descriptors and validated roles. It does not reconstruct compiler initialization or cleanup plans. No new protected host or provider type is permitted. If generated metadata cannot express a required fact, identify the missing fact and owning compiler phase with a failing consumer. Read `docs/design/compiler-architecture.md` and the relevant checker, lowering, codegen and package-interface documents before changing that phase. Preserve typed demand, immutable publication, target context, exact dependencies and structured diagnostics.

### Consumers and removals

Trace `crates/bray-runtime/src/product/host/formation.rs`, `model.rs` and `operations.rs` in that directory, the retained execution owner in `crates/bray-runtime/src/native/state/core.rs`, generated product formation and foreign callback callers, and their current native library fixtures. These are inventory starting points, not permission to leave unlisted live callers behind.

Migrate dynamic provider policy and its adapter calls in the same change. Reuse the previously migrated host, static cleanup, attachment, report storage and admission owners. No independent registry, report arena or lifecycle planner may remain. Name any bridge still required by a separate unmigrated consumer, its exact lifetime contract and the active issue that removes it. The loaded-provider consumer completed here may not remain Rust-backed.

### Acceptance evidence

| Consumer or failure | Required observable result |
| --- | --- |
| Two simultaneous loads of identical bytes | Distinct statics and identities. Releasing one does not corrupt or terminate the other. |
| Rejected binding, cycle or backing acquisition | No published entry or consumed caller input. Rollback releases every accepted local resource once. |
| Static archive and imported MIR contribution | Contributions use the final consumer host with the same dependency and cleanup semantics. |
| Report or typed error survives producer-thread exit | Formatting, disposal and backing release remain valid. Provider statics stay live while its dependency survives. |
| Caller holds an escaping report during shutdown | Shutdown preserves its owner without circular waiting. Disposing the report later permits terminal cleanup through the retained owner. |
| Callback races with retirement | Entered callback returns before provider/context release. New entry rejects after closure. |
| Reload after terminal release | A new identity forms. Stale external handles cannot enter or release it. |
| Copy independent diagnostic history, then dispose original | History remains usable while provider resources can release. Retaining typed provider data instead keeps the provider live. |
| Allocation denied after admission | Mandatory cleanup, incidents, typed-error destruction and backing release finish using actual admitted storage. Application allocation failure follows its ordinary contract. |
| Cleanup failure | Remaining independent cleanup proceeds. Preserve original and ordered distinct incidents. Release each owner exactly once. |
| Synchronous loaded consumer | Native demand and retained-unit evidence contains no scheduler introduced by provider formation or reporting. |

Update BRA-501's code-only cleanup fixture to the agreed whole-product behavior and preserve its other useful coverage. Inspect retained native units, archive members and link maps to prove that migrated provider paths contain no project-owned Rust implementation. Run the real loaded-library consumer on available supported hosts and record missing required host evidence under final cross-target acceptance.

### Costs, boundaries and completion

Record formation allocation, lookup and synchronization costs, retained code/data, release work and compiler metadata changes. Use a fixed synchronous load/retain/release fixture for comparable before-and-after evidence. Compare an async consumer separately. Explain unavoidable whole-product retention costs and demonstrate release after independent history replaces the original report.

This issue does not implement a scheduler, worker pool, report representation, general cleanup engine or a new language visibility rule. Its prerequisites must supply physical admission and the completed synchronous host contracts. It extends those owners for dynamic loads instead of building provisional replacements.

Completion requires every listed consumer and failure contract, one authoritative provider owner, migrated live callers, deleted replaced policy and the stated evidence. A compiler gap blocks completion until its coherent prerequisite lands. A proposed semantic change requires an explicit design amendment. Type names, coallocation and lookup representation remain implementation choices within this contract.
