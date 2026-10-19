# BRA-619 completion acceptance

## Contract and baseline

Continue through BRA-619, including reviewed PRs and pushed fixes. No merge is authorized.
Read AGENTS.md, coding-conventions.md, crates.md and this record after compaction.
Use only release xtask. Runtime ownership belongs in Bray. No new syntax, language feature,
Rust frame, authority relaxation, post-failure allocation or downstream revalidation.

Branch feature/bra-619-scoped-cleanup-acceptance starts at d69592ca0 on reviewed PR #613.
BRA-505 PR #612 is ready with its scoped native boundary audit recorded. BRA-626 PR #613
is implemented and independently reviewed. Preserve the older recovery branch at 6bbcb4a3.
The selected calls and native transfer are already implemented. The old MissingRole regression
now passes. Complete acceptance rather than replaying old code or adding another mechanism.

## Coverage matrix

| Obligation | Existing implementation and evidence | Remaining acceptance |
| --- | --- | --- |
| Selected inputs and exactly-once entry/exit | lowering/scoped.rs, lifecycle lowering tests | Native sync/async success and failed entry |
| Normal and yield exit error | replace_scoped_result and checked scoped failure type | Native owned error disposal |
| Pending return/break/continue | retain_incident and shared CleanupOutcome | Native original outcomes and retained ownership |
| Nullable/result/run-result propagation | Existing propagation selection and guarded cleanup | Native composition with fallible exit |
| Panic/cancellation and shielding | Checked scoped suspension, CleanupShield, shared broadcasts | Native shielding and precedence |
| Physical backing and error quiescence | BRA-502/505 admitted transfer and abandoned destruction | Inactive Future capture, delayed disposal and denied allocation |
| Borrowed capability and trusted authority | Existing source/imported lifecycle and trusted tests | Reuse checks, add missing imported authority variants if absent |

## Design and limits

Add a focused native scoped-use fixture to the existing execution smoke. Use source counters,
existing catch/report ownership and the allocation probe. No new runtime object or framework.
Change production only for concrete gaps exposed by the matrix, in the owning phase, and audit
sibling operations. Tests are batched before mandatory format/style. Measure production/test
changes separately. Unexpected runtime adapters or substantial growth require reconsideration.

Shared Cargo/LLVM storage is C:/Users/anton/source/repos/bray/target. Native workflows run
outside the sandbox. Do not delete shared artifacts. Delete this and completed stack handoffs
when their inputs are no longer needed. Keep durable behavior in specs/tests and open issues.

## Native gap exposed by the matrix

The first native run passed old products, then hit the retained suspending-helper assertion.
Scoped exit did not attach the error-quiescence retention already used by finalizers. Immediate
fallible exits also lacked checked body-exit suspension for quiescent error payloads. The fix
publishes those scopes in the existing CleanupPlan, reuses it for CFG/async retention, shares
lifecycle_retention with finalizers, and carries exact exit identity in abnormal scoped steps.
No synchronous generated helper may receive a suspending action. Keep the invariant assertion.

Current uncommitted acceptance fixture has 21 entries, 17 exits, 19 errors, 22 captures and 8
ordinary finalizers, including cancellation and cleanup-panic precedence. Assertions must stay
outside expected catch containment, or set a checked flag observed outside. Inactive body and
ordering violations have outer assertions. Native run before these assertion-strengthening edits
is target/bra-619-native-smoke.log. Rebuild tools after the compiler fix and rerun acceptance.
19 scoped tests passed before this repair. Source checking passed the fixture before the last
assertion-strengthening edits. Rustfmt, mutating style and format passed before this repair.

Latest repair checks pass 22 scoped, 95 lowering, 21 exported lifecycle and 315 source-query
cases. An empty unit/lowering selector is not evidence and was replaced by the 95-case suite.
Style found the async analysis function at 267 production lines. Context checking now belongs
to its existing selected-async-operation helper. CleanupPlan retains the exact body/expression
relation once. Async exits reuse their existing awaited retention, so only immediate exits add
an implicit error-quiescence scope. The negative sync-caller diagnostic is covered.

The attempt to copy the full proof into a new Linear document was auto-review rejected as an
external disclosure. No document was created. Use the verified already-public immutable proof
at https://github.com/lejmer/bray/blob/d69592ca0/docs/handoff/runtime-stack-semantic-proof.md
as the safer alternative in tracked plan links. The three existing Linear plan/inventory/sequence
documents already preserve their own content. Do not retry full proof publication or ask for it.

Active build/test/style session 60956. No native/debug process remains. Native acceptance must
be rerun with the repaired release tools. Do not claim completion from MIR tests alone.

## Remaining ownership obligation

The audit found that storage-flow invocation_result(ScopedExit) returns None. A source-only
check accepts a retained exit error whose destructor reads a borrow of the dying resource.
Reproducer: target/bra-619-borrowed-exit-error.bray, log .log, exit 0. This differs from a bare
unused borrow, whose dependency may legitimately end at last use. The concrete Failure has
value: &Value and destruct() calls self.value.read(). Resource.enter returns &self.value,
exit returns Error(Failure { value = lease }), and the caller owns Resource and returns Ok
inside with. The retained incident can outlive that Resource. Native continuation acceptance
alone does not discharge this obligation. Do not claim BRA-619 complete.

Trace the shared retained cleanup-result ownership boundary, including finalizers, through
existing storage/dependency and retirement mechanisms. Represent the actual selected result
and guarded normal/error/retained transfer in its owning analysis. Reuse established results
in liveness/lowering. Do not patch the final descriptor, assume unused fields, ban all borrows
by type, introduce a new language contract or grow another runtime owner/framework.

Current checks: 22 scoped tests, 95 lowering, 21 exported lifecycle, 315 source query pass.
Rustfmt and mutating structural style pass after moving the context check into the existing
selected-async-operation helper. Bray formatting passes. No task-owned process remains.
Next: checkpoint current continuation repair, native run with fresh release xtask, then close
the retained error dependency boundary before final review. The source check used brayc from
BRA-626, whose relevant invocation-result behavior is unchanged by this continuation repair.
