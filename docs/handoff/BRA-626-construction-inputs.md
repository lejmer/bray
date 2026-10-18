# BRA-626 construction inputs

## Resume contract

Continue the authorized stack through BRA-619. Questions and checkpoints do not end it.
After compaction reread AGENTS.md, coding-conventions.md, crates.md and this record, plus
BRA-505-retained-incidents.md. Use the Bray issue, debugging and language skills. No new
syntax, language feature, weaker authority, Rust frame, compatibility path or validation pass.
Use release xtask, batch tests before formatting and preserve the user's fixture formatting.
Delete this temporary record after acceptance and review. Do not merge without authorization.

## Baseline and active work

Worktree C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch feature/bra-626-consistent-construction-input-conversions, based on edef8c037.
BRA-505 remains draft PR #612 on feature/bra-505-admitted-ordinary-finalization. Its full native
smoke passes, but the native adapter audit remains explicit and incomplete. Do not infer its
completion from BRA-626. BRA-626 is In Progress and actually blocks BRA-619 in Linear.
BRA-619 recovery is 6bbcb4a3. Do not replay already included selected-scope work.

Publication is authorized to PUBLIC lejmer/bray, origin git@github.com:lejmer/bray.git,
authenticated lejmer. Worktree mutations and native processes run outside the sandbox. The
main checkout is shared build storage only. Never delete shared artifacts or logs.

## Contract and connected implementation

Apply the existing identity and nullable-present initialization rules to structure, union
and type-form inputs. Preserve literal adaptation, source order, defaults, ownership, partial
cleanup, dependencies, exact references and authority. Insert exactly one nullable layer,
including for already-nullable sources. Reject extraction and mismatched contained types.

One existing conversion module now owns call/construction admission. Explicit construction
inputs retain SelectedConversion instead of target-only ty. Defaults retain their type.
Lowering applies that conversion before retaining the input. Constants evaluate converted
terms once in source order and preserve the selected type-form witness. Source templates
share converted expression emission with calls, and imported evaluation supports insertion.

ValueInputs aggregate edges store the source with an optional inserted projection. Identity
has none and insertion has NullableValue. Neither allocates a suffix. Projection resolution
consumes only a complete matching prefix. Reference preparation uses that complete path and
type. Trusted type-form arguments normalize to their accepted state, without adding authority.

LLVM now consumes converted MIR inputs directly. Delete its second constructor coercion,
EvaluatedConstructionInput wrapper and extra type-form vector copy. Input identity owns its
symbol projection, replacing dependency forwarding helpers. Implicit conversion adds no callees.
Existing field/expression types retain their components. Cleanup shapes consume accepted types.

The existing expression/operation inference loop owns convergence. Do not add a construction
source-type check to its intermediate table. Provisional literal/await evidence can still refine.
Do not add another finalization or validation pass. Reconsider unexpected layers, repeated work,
new ownership representations or substantial growth. The target was a few hundred production
lines, with tests measured separately. Do not compress formatting or delete useful coverage.

## Review matrix

| Boundary | Owning implementation and evidence |
| --- | --- |
| Admission | Shared selector. Structure/union identity and insertion, rejected extraction and scalar coercion, type-form policy arguments/defaults. |
| Order and ownership | Existing input guards. Converted input survives a later await. Native partial construction requires async cleanup after a later input panics. |
| References | ValueInputs and reference preparation. Nested nullable depths and imported nullable field lifetime acceptance/rejection. |
| Authority | Existing condition coercion. Valid paired owners survive wrapping. Reconstructed and differently paired owners remain rejected. |
| Constants/imports | Converted input evaluator, selected witness, shared template Convert. Structure, union and imported constant callables. |
| MIR/LLVM | Exactly one nullable aggregate per construction. No backend re-coercion. All lifecycle native emission cases. |
| Dependencies/cleanup | Existing cached shapes and input declaration identities. Imported defaults, lifetime contracts and phase metadata. |
| Diagnostic ownership | Range regression exposed provisional expression diagnostics copied by iteration selection. Final checking already had valid type/selection. Remove that premature propagation in iteration, without filtering diagnostics. |

The heap regression assumed every cleanup needed CheckCallOutcome. Collected cleanup now
reports directly to its shared outcome. Assert checks for uncollected cleanup and require
collection use for failed creation. No production heap behavior was changed.

## Workaround audit

Audit tracked Bray and Rust inline source under runtime, standard-library, xtask, crates and
examples. Search names/comments, 312 annotated nullable declarations, subsequent field/payload
assignment, explicit nullable casts and split initialization. Remove fail()'s captured and pending
locals, using the async call directly. Remove two wrapped-reference construction detours in
returned_values.rs. Add the direct nullable partial-construction native case.

Retain nullable-state bindings because repeated discriminant/borrow checks test those bindings.
Iterator optionals branch, finalize or advance cursor state. Tree optionals carry traversal state
or leave separately borrowed scopes. Bootstrap stored AttachmentExit feeds a raw write rather
than construction. Replacement temporaries retain old values for independent witness/release
ordering. No additional labelled workaround or production explicit nullable cast detour was found.

## Current evidence and next action

All 1,184 affected tests pass. Counts: 124 bound-tree, 331 checker, 38 lowering, 93 LLVM, 315 source
queries, 93 source lowering, 7 constant import/export, 28 returned-dependency import, 5 imported
default, 18 lifecycle export, 112 source authority, 4 nullable authority, 5 heap and 11 native
lifecycle emission. Logs: target/bra-626-final-compile.log and bra-626-final-consumer-tests.log.

The required mechanical audit reread AGENTS.md, reviewed changed paths and searched equivalent
policies, projection transfer, constant/template conversion, symbol projection and workaround
shapes outside changed files. Growth is 105 production physical lines and 330 test lines,
excluding this record. No new type, framework or runtime owner. Rustfmt, mutating release style
and Bray formatting pass. The combined release build passed in 2m48s. Mandatory cargo xtask
style/format and full native smoke are running in session 38254. Earlier sessions are finished.

Next: finish native acceptance and cost evidence, publish the branch and its own stacked draft
PR, then an independent PR review with findings published by the reviewer. Fix and push valid
findings, obtain clean review, move BRA-626 to In Review and remove this handoff. Continue the
remaining BRA-505 boundary and BRA-619. Do not mark issues Done before the user's merge.

Shared CARGO_TARGET_DIR: C:/Users/anton/source/repos/bray/target.
CARGO_BUILD_BUILD_DIR: C:/Users/anton/source/repos/bray/target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX: target/toolchains/llvm/active in the main checkout.
Build tools with default Rust 1.99. Native invocation uses process-local
RUSTUP_TOOLCHAIN=1.98.1-x86_64-pc-windows-msvc and TMP/TEMP=target/native/bra-505.
Use rg --files before unknown paths. PowerShell does not expand wildcard paths passed to rg.
