# BRA-554 implementation record

## Objective and baseline

Complete synchronous/source and foreign entry authority in Bray, then continue BRA-558.
Branch `feature/bra-554-implement-synchronous-and-foreign-entry-authority-in-bray` starts at
`f5e00436`, the reviewed BRA-502 tip in [PR #608](https://github.com/lejmer/bray/pull/608).
BRA-502 is In Review. Its final [published review](https://github.com/lejmer/bray/pull/608#pullrequestreview-5528576171)
has no remaining actionable findings. The complete native workflow and final-link storage checks passed.
BRA-554's PR must target the BRA-502 branch. BRA-558 follows this branch without waiting for merges.

## Behavioral requirements

Acquire entry and exact-thread ownership before source execution, serialize entry with closure,
and preserve inputs on rejection. Admit bounded outgoing records before arbitrary callbacks.
Restore the enclosing context on nested normal and abnormal exits. Keep locks out of callbacks
and disposal. Preserve checked cleanup and returned-owner dependencies. Return the native zero
representation on failure without native unwinding. Identity observation supplies no authority.
Do not create or retain a scheduler merely to inspect context.

## Existing implementation and concrete gaps

| Obligation | Existing owner and evidence | Required work |
| --- | --- | --- |
| Resident storage and exact-thread epoch | Bray host/attachment owners, production forgery tests and native bootstrap smoke | Reuse enter/leave and checked acquisition contracts |
| Concrete outgoing storage | BRA-502 pool, storage bundle and typed incident lease | Reuse actual records and backing, no second allocator or scalar admission |
| Entry/closure serialization | Product host registry control and its existing closure tests | Make generated entry consumers acquire/release the established product contract |
| Nested context | Rust native state and context scopes, restoration tests | Clear the complete native execution context, then restore it |
| Synchronous/foreign policy | Rust native callback boundary and adapters | Switch actual roles to Bray and remove replaced Rust entry policy |
| Foreign abnormal result | LLVM native trampoline and body callback | Preserve the body's checked panic outcome and zero its ABI result on failure |
| Native function/provider validity | Existing role binding, product formation and descriptor validation | Validate at the boundary, reuse results downstream |
| Returned synchronous owner | Existing checked result transfer, storage lease and inferred dependencies | Preserve actual ownership through native transfer, never replace it with an entry count |
| Remaining Rust execution contexts | Task execution and native test-host consumers | Keep any required context bridge restricted to those consumers |
| Typed incident consumption | Existing report consumer, followed by BRA-558 | Share the same incident owner and preserve failure ordering |

The foreign trampoline currently calls the boundary without acquiring a product entry. Its
callback helper also copies every body parameter from the native argument state and overwrites
the outcome with completion, which needs correction for the hidden panic context.
The independent-context helper clears only the task, leaving its runtime visible.
Product formation currently retains a runtime unconditionally, including synchronous products.

## Design hypothesis and removals

Bray owns admission, attachment lifetime and callback sequencing over the resident host.
Generated adapters supply the already checked callable and native result transfer.
Existing product control remains the formation/closure contract while BRA-555 owns its complete
provider migration. BRA-557 owns full attachment membership and static cleanup. BRA-507 owns
suspending disposal. These boundaries do not excuse a failing synchronous entry requirement.

Reuse the existing record service and typed native outcome. Any bridge for still-Rust task or
test-host context must use the admitted records and remain absent from ordinary synchronous
products. Remove the old synchronous and foreign Rust entry exports and their policy after switching
consumers. Preserve native-thread execution until its owning migration.

No new syntax, language feature, erased returned-owner protocol, witness registry, second host
registry or downstream semantic validation is planned. Keep changes in existing owning analyses
and runtime services. Reconsider the design if it needs duplicated admission, per-entry heap
allocation for ordinary synchronous calls, or another interpretation of checked lifetime facts.

## Acceptance matrix

Cover source and imported functions, direct and indirect native results, cold and nested entry,
entry rejection before source access, recursive entry, concurrent closure, body panic and cleanup
failure, returned synchronous owners, and lost/fabricated authority. Verify native zero results,
ordered incidents, exactly-once release, provider/backing validity, and restoration after failure.
Reuse the existing production declaration and native smoke facilities. Include real generated
foreign trampolines, not only runtime callbacks. Record TLS/atomic/bridge costs and link-map
retention for a tiny callback and ordinary synchronous entry.

Run batched focused checks after coherent edits, then required native checks, style and format.
Audit correctness and structural cost separately. Publish a stacked draft PR, obtain an independent
published review, fix findings, and ready it only after acceptance is complete.


## Outcome and context checkpoint

All native callback bodies now receive the explicit panic/cancellation destination. The generated
C invocation helper supplies that hidden argument separately from native arguments and publishes a
result only on completion. Native entry addresses still expose the original C signature.
The independent Rust execution scope now saves and clears both the runtime and task, while an
explicitly bound cleanup runtime keeps its deliberate binding. Both scopes restore their callers.

Evidence:

- The generated native scalar/aggregate test covers normal return, cancellation and panic. Both
  abnormal cases return zero. It fails immediately if native panic/cancellation propagation is used,
  and checks exactly two panic observations and balanced cleanup shielding. The isolated C provider
  tests ABI outcome transfer, not admission or production report ownership.
- The product code-generation group passed 116 checks. Its one failure is the retained BRA-619
  `MissingRole(CleanupIncidentTransfer)` finding, still enabled for Step 6. Log: `bra-554-codegen.log`.
- Complete outer-context restoration passed on normal return and native unwind, including runtime,
  task, lane, cancellation and output. Log: `bra-554-context.log`.
- `cargo xtask style` and `cargo xtask format` passed. Only existing module-size warnings remain.
  The changed Rust files also passed the installed Rust formatter and `git diff --check`.

The callback assertion in the shared mapping helper also covers existing native export/import and
cross-target checks. The two Windows native caller tests now share their compile/link/run helper.
No source entry migration or BRA-554 completion is claimed by this checkpoint.

## Entry migration constraints established by the audit

The native callback has already transferred its typed result when it returns to the entry service.
A later entry-cleanup failure cannot justify erasing that result without its selected cleanup.
Keep body completion and terminal host incidents distinct, as specified for static/thread terminal
cleanup. Body panic and cancellation must still produce ABI zero. Reporting a consumed root panic
must not return a second owning copy of its report.

Product formation currently retains a runtime for static cleanup and worker attachment coordination.
An optional-runtime edit was investigated and removed: removing retention alone leaves synchronous
cleanup without the attachment/context contract previously supplied by that runtime. Replace those
operations together when switching entry consumers. Do not add a fallback that invokes cleanup
without the required attachment.

The remaining implementation must provide the Bray entry owner, product acquisition/closure,
pre-admitted outcomes, exact attachment lifetime and an optional bridge for callers with Rust task
or test-host context. That bridge must contain only Rust context management and native Rust unwind
conversion, with admission and attachment policy owned by Bray. It must be absent from ordinary
synchronous products. Production authority, returned-owner, closure, report-order and cost evidence
remain outstanding. BRA-558 follows after this issue and Step 5 remains active.
