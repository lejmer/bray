# Runtime ownership and authority proof

Discussion draft, 2026-10-13. Baseline: `e3f56424` on
`feature/bra-620-repair-synchronization-authority`.

Implementation is paused for this review. This argument examines the existing language rules before choosing more
compiler machinery. It introduces no syntax, language rule, runtime representation, or implementation mandate.
The earlier [replacement plan](runtime-stack-rule-coverage.md) supplies candidate mechanisms, not proof premises.
Its implementation sequence remains subject to this review. Historical results stay in
[BRA-620 remaining work](BRA-620-remaining-work.md).

The design hypothesis is that ownership, dependency validity and contract evidence admit one compositional transition
semantics. The expected simplification is removal of competing interpretations of those transitions, not removal of
necessary semantic distinctions. No production code growth is authorized by this document. Reconsider the hypothesis
if a required operation needs hidden semantic state, an unsupported preservation rule, or an unbounded proof search.

## Result and limits

The specification supports a common ownership and evidence model. Whole moves, ordinary borrows, calls and cleanup
can be described using the same identities and validity rules. The derivations below explain why.

The previous plan did not finish two essential arguments:

- A complete carrier prevents authority forgery, but this alone does not prove that an opaque acquisition predicate
  survives mutation of the protected payload. An unchanged empty marker is insufficient evidence by itself.
- Finite conservative analysis can be sound and terminate while rejecting required programs. Finite paths, widening
  and may/must joins are not a proof of the required acceptance behavior.

There is also a concrete disagreement with a current test expectation. Harmless shared observation is explicitly
allowed to preserve guarantees. Absence of a preserving postcondition alone does not require revocation.

These findings do not establish that new language features are necessary. They prevent treating the previous
implementation choices as already proved. This is a written, conditional proof argument, not a machine-checked
formalization or a proof of the current compiler.

## Premises and source authority

Each premise below comes from the language specification. The proof notation introduced later is explanatory only.

| Premise | Required rule and source |
| --- | --- |
| P1 | Ownership does not imply mutation authority. Aliases are non-owning and capability checked. Moves transfer ownership. [Axioms 1–6, 9 and 11](../axioms/core-axiom-set.md). |
| P2 | Paths compose one component at a time. Unknown overlap is potential overlap. Every nested borrow layer has its own capability. [Paths](../language/ownership-and-borrowing/storage-and-access-paths.md), [reborrowing](../language/ownership-and-borrowing/reborrowing-and-borrow-values.md). |
| P3 | Moving an aggregate transfers its initialized subvalues and obligations. Borrow movement does not move its referent. Every boundary must preserve all carried dependencies. [Moves](../language/ownership-and-borrowing/moves-copies-and-consumption.md), [boundaries](../language/ownership-and-borrowing/scope-exits-and-ownership-boundaries.md). |
| P4 | A trusted guarantee needs an authorized source. A witness retains guarantees only while valid. Whole moves transfer them. Ordinary field copies do not. Separating a guarantee from its carrier by partial movement is rejected. [Caller obligations](../language/contracts-and-trust/trusted-caller-obligations.md), [witnesses](../language/contracts-and-trust/trusted-witness-values.md). |
| P5 | A condition is available only on every relevant incoming path. A change invalidates it when it can affect its truth. Harmless shared observation preserves guarantees. [Reasoning](../language/contracts-and-trust/contract-reasoning.md), [validity](../language/ownership-and-borrowing/contract-and-borrow-validity.md). |
| P6 | Normal postconditions are unavailable on abnormal completion. Entry guards refer to entry state. Async invocation does not establish deferred completion postconditions. [Postconditions](../language/contracts-and-trust/preconditions-and-postconditions.md), [execution](../language/contracts-and-trust/execution-guarantees.md). |
| P7 | Uninit contains either no T or tracked initialized ownership. Committing, moving out and destroying change that state. An anchored borrow has a physical referent and separately retains its exact authority root. [Protected storage](../language/targets-layout-abi-and-raw-memory/uninitialized-storage-and-anchored-borrows.md). |
| P8 | Destruction consumes its owner and then remaining initialized parts. Finalization retains an initialized owner. Replacement restores the destination before abnormal propagation once old cleanup starts. [Destruction](../language/lifecycle/destruction.md), [finalization](../language/lifecycle/finalization.md), [replacement](../language/lifecycle/partial-values-and-replacement.md). |
| P9 | Scope cleanup resolves live obligations in the specified order on each outcome. Cancellation request is distinct from resolution. Scope exit can transfer an obligation only under its contract. [Exits](../language/lifecycle/scope-exits-panics-and-cancellation.md), [with](../language/lifecycle/with-expressions.md). |
| P10 | Inferred dependencies cross interfaces. Foreign contracts are explicit trust assumptions. Trusted obligations cannot disappear inside wrappers. [Dependencies](../language/ownership-and-borrowing/dependency-contracts.md), [foreign boundaries](../language/contracts-and-trust/lifecycle-and-foreign-boundaries.md), [propagation](../language/contracts-and-trust/obligation-propagation.md). |
| P11 | Cleanup admission secures physical backing before establishing an obligation. Moves preserve it. A current completion fact cannot remove a type-wide allowance. [Capacity](../language/async-and-concurrency/async-representation-and-storage.md#cleanup-capacity). |

[Checker design](../design/checker.md) and [compiler architecture](../design/compiler-architecture.md) constrain
implementation ownership. They require shared control flow, a composite storage domain, immutable publication and
bounded fixed points. They do not prove any particular transfer function or summary representation correct.

An existing test, passing implementation, issue description or accepted planning proposal cannot override P1–P11.

## The semantic objects we actually need

Use S for a concrete semantic state. Concrete identities distinguish dynamic occurrences, including successive
allocations or acquisitions at the same source site. They need not exist as runtime fields.

- An ownership occurrence identifies the live value and its current owner. Moving it changes its owner.
- A storage occurrence identifies a location and its current incarnation. A physical move can change the location.
  Address equality does not identify incarnations.
- A reference relates its own value to reached storage and to the capability and lifetime dependencies needed to use it.
  Each dereference crosses a reference value. It is not another inline field.
- An obligation identifies unresolved lifecycle work and any backing or provider dependency retained for it.
- An evidence record identifies a proposition, its authority source where required, its carrier where present,
  and its validity dependencies. Its observation dependencies describe changes that can affect truth.

These are semantic distinctions, not a request for five new compiler types or maps. Initialization, active variant
and partial ownership belong to the relevant ownership/storage occurrences. Existing compiler types may represent them.

Each distinction has a separating example. A moved guard retains acquisition authority but an old pointer to physically
relocated inline storage does not become valid at its new location. Two guards with equal fields need not have equal
authority. A borrow can point inside a lock while depending on a different guard. Replacing a reference slot changes
which object it reaches without modifying the former referent. A completed finalizer can leave destruction pending.
Removing any of these distinctions conflates behaviors required by P2–P9.

This establishes necessary distinctions, not global minimality of a compiler representation.

For an evidence record e, write:

```text
Usable(e, S) =
    authorized_source(e)
    and valid_carrier_if_present(e, S)
    and valid_dependencies(e, S)
    and proposition_holds(e, S)
```

For ordinary branch facts, authorized_source means the selected ordinary reasoning rule. Trusted evidence additionally
requires P4's provenance. Predicate equality does not manufacture that provenance. Some evidence is a parameter
assumption or a completion fact rather than an owned returned witness. Do not make every proposition an owned token.

## The transition proof

Write T(o, S, k) for the possible successor states of operation o with outcome k.
Outcomes distinguish normal return, panic and cancellation. Result.Error is a normal returned value.
Divergence has no completed successor and must still be considered when proving total execution.

Let I(S) mean:

1. Every live noncopy ownership occurrence has one responsible owner. Its initialized parts and pending obligations agree.
2. Every permitted access has valid initialization and compatible capability for all reached storage it may access.
3. Every usable trusted fact has authorized provenance and valid carrier/dependencies.
4. Each discharged lifecycle step was completed or discharged by its specified rule. Pending work and backing retain an owner.

The desired theorem is conditional on valid trusted implementations:

> Starting from a state satisfying I, every accepted operation produces only successors satisfying I.
> Therefore every finite accepted execution prefix preserves I.

The second sentence follows by induction on execution length once the first is proved for each selected operation.
It does not prove termination, concurrency correctness or absence of application deadlock.

### Evidence can survive by framing

Let Changed(o, S, k) cover every state component that the operation may change on that outcome.
Let Support(e) cover every component whose change can invalidate the evidence, including carrier validity,
resource epochs and observation dependencies.

If e is usable before the operation and Changed is disjoint from Support, e remains usable afterward.
Every conjunct in Usable has the same value, so their conjunction remains true. This is the frame argument.

Disjointness is sufficient, not necessary. A verified transition can change a component and re-establish the same
proposition. For example, replacing an initialized T with another T changes its contents but restores initialization.
A rule that deletes all evidence on every write is therefore too coarse. A rule that preserves all evidence whenever
the address is unchanged is unsound.

This lemma is conditional on sound Changed and Support descriptions. Calling a predicate opaque or a parameter
immutable does not supply that missing proof.

### Ownership transfer transports evidence

A legal whole move transfers the initialized value, its carried evidence, and obligations to the destination by P3–P4.
The destination must preserve every dependency by P3. The source becomes moved-from and cannot supply a second owner.

This is a change of ownership names for the transferred value, not permission to rename every physical referent.
External storage remains external. Borrows into relocated storage must satisfy the movement restrictions.
A heap-owning value can move while its allocation stays in place.

Partial transfer uses the same rule on the selected part, but is rejected if it separates a whole carrier from its
guarantee. Checking this only at eventual destruction is insufficient. A path can diverge after extraction.

### Safe operations cannot forge authority

Assume no trusted source has established an acquisition for a fabricated guard. Consider the first operation claimed
to make that guard authoritative.

Construction of equal fields establishes representation, not trusted provenance. Projection establishes no owner.
Copying ordinary fields carries no witness authority. Whole movement can only transfer existing authority.
A separating partial move is prohibited. Branch joins cannot create a source missing on an incoming alternative.
A safe call can introduce authority only through an authorized producer whose requirements hold.

These cases exhaust the ways value construction, transfer and calls could introduce that authority in this model.
None supplies it under the assumption, a contradiction. Explicit trust assertions and incorrect trusted implementations
are outside this safe-code conclusion. Wrapper checking must still obey P10.

Cross-lock pairing adds a further premise: the producer and accessor contracts bind the complete carrier to the exact
lock, and that relationship cannot be changed independently while authority survives. The witness rules prevent
fabrication. They do not prove that a particular producer API states the correct relationship.

### Borrowing preserves ownership and restricts interference

Borrow creation retains the existing owner and establishes only the selected capability. Reborrowing cannot increase
authority. Mutation, relocation and retirement that conflict with a live borrow are rejected by P2.
A reference to a reference first reaches the reference slot. Updating that slot is distinct from mutating its old target.

An ordinary mutable T borrow does not grant ownership to move out an arbitrary T. Replacement is legal under P8's
restoration rule. Consequently safe payload operations can preserve the proposition "this slot contains a T" while
changing its contents. This does not preserve the old T's identity, old interior references, or an arbitrary predicate
about its value. Raw trusted operations must establish their own poststate rather than inherit that conclusion.

For Uninit, a checked write adds initialized ownership only after commit. Move-out removes it. Ending the protected
storage with an owned T still inside violates P7. No new initialized-storage language type is required to state this rule.

### Calls and cleanup compose the same rules

A call checks requirements in entry state, performs argument ownership transfer, and applies the callee's effects.
It adds only postconditions established on the reached normal outcome. Guard eligibility uses the captured entry state.
Unchanged evidence can survive through the frame lemma without a redundant ensures clause.

A destructor is this call transition with a consuming receiver, followed by cleanup of remaining initialized parts.
A finalizer uses a mutable receiver and leaves an initialized owner. Its postcondition decides completion separately
from the returned error. with exit retains its capability during the call and handles the pending outcome under P9.
Replacement adds the P8 ordering requirement around old cleanup and installation.

Thus the same precondition/effect/completion rules apply at explicit and implicit calls. Different ordering and receiver
modes remain real semantics. "Shared cleanup" cannot mean erasing those distinctions.

For example, a Writer destructor that initializes a borrowed slot establishes a fact about that external slot on
normal completion. Destroying Writer does not destroy the slot. A fact tied instead to Writer's own destroyed storage
cannot escape. Exact referent identity decides which case applies.

Cleanup conservation is an induction over the selected cleanup sequence. Each step consumes, transfers or retains
its obligation, and records resulting owned outcomes. Remaining represented parts retain their own obligations.
By P11, backing cannot be released while a transferred incident or activation owns it.
This proves no duplicate or lost duty only if the selected sequence covers every live obligation on each outcome.

## Joins and function boundaries

An abstract state A denotes a set of possible concrete states, written gamma(A).
A sound transfer must satisfy:

```text
{ S' | S in gamma(A), S' in T(o, S, k) } subset gamma(transfer(o, A, k))
gamma(A) union gamma(B) subset gamma(join(A, B))
```

A check may authorize an operation only when its preconditions hold for every represented state.
Possible targets, conflicts and pending duties therefore accumulate. Unconditional evidence must hold on every
represented alternative. Nullable evidence is conditional on the present alternative.

Correlations are essential. If one branch returns the valid pair (guard1, lock1) and another returns (guard2, lock2),
the join cannot assert every pairing between {guard1, guard2} and {lock1, lock2}. It may retain their relationship through
the returned carrier. If it forgets the relationship, it must reject uses needing it. Forgetting can be sound and still
violate required acceptance. Separate may/must sets alone do not settle this problem.

A function summary is a relation between formal input state, outcomes and output/dependency state.
It must overapproximate actual effects and underclaim available guarantees, with carrier correlations retained.
Substitution maps formal identities to actual identities consistently, including aliasing between actual arguments.
A write cannot use a strong single-target update unless uniqueness is proved.

If the body satisfies that relation, replacing its execution with the summary preserves the local proof.
Source, imported, generic and indirect calls then need adapters for identity and substitution, not different semantics.
Interface decoding validates external representations once. Lowering consumes the established decisions.

Recursive summaries require a separate convergence argument. A finite-height domain and monotone worklist terminate,
but only after identifying the actual finite subjects, projections, guards and relations and proving closure under
substitution and transfer. An arbitrary depth cutoff is not a language acceptance argument.
Recursive proof of total execution is separately restricted by P6. A fixed point cannot certify termination merely
because its own provisional summary says total.

## Apply the argument to acquisition

Let G be a complete guard, L its retained lock, M its owned immutable marker and V the protected payload.
These are mathematical names, not proposed Bray syntax. A producer must establish the complete G's authority and
its relationship to L for one acquisition. A returned payload borrow reaches V and depends on G's live capability.

The intended sequence is acquire G, borrow V, mutate V, end the borrow, release G, and eventually destroy L.
Non-forgery follows from the earlier induction if the producer binds G to L.
Release conflicts with a still-live dependent payload borrow. Whole movement preserves G's relationship.
Reacquisition creates a new ownership occurrence and cannot revive an earlier guard.

The unproved step is preservation through payload mutation. The frame lemma needs:

```text
Changed(payload operation) disjoint Support(acquisition evidence)
```

An unchanged M establishes only that M's own state did not change. A trusted predicate has no body.
[Predicate declarations](../language/declarations/predicate-declarations.md) call it an opaque relation.
[Predicate expressions](../language/contracts-and-trust/predicates-and-predicate-expressions.md) restrict observations,
but do not provide an explicit dependency-extraction rule for every opaque relation.

The compiler-known predicate initialized_as(pointer) already illustrates why unchanged arguments are insufficient:
the pointer value can be unchanged while the predicate's truth changes. Its
[defined semantics](../language/targets-layout-abi-and-raw-memory/raw-memory-predicates-and-capabilities.md) supply the
missing dependency on pointed storage.

The marker proposal needs an equally justified account of the custom acquisition relation's support.
It cannot assume that all carrier dependencies are observed state, because that needlessly revokes acquisition on
payload writes. It cannot assume that only the marker's bytes matter, because that is not established by opacity.
The earlier plan's unconditional claim that the marker solves preservation is therefore not proved.

This is not a proof that the marker design is impossible. Resolve the interpretation of existing opaque-predicate and
dependency rules, or derive preservation from an existing checked contract, before choosing its implementation.
No runtime identity registry, field privacy assumption, or new preservation syntax is authorized here.

## Acceptance obligations and existing evidence

Sound rejection of unsafe programs and required acceptance of valid programs are separate obligations.
The following cases derive from P1–P11, rather than from the current failure count.

| Family | Required derivation |
| --- | --- |
| Initialized payload | A checked commit followed by a valid ordinary payload borrow leaves an initialized T after legal replacement and borrow completion. Move-out removes that ownership once. No arbitrary payload predicate is preserved. |
| Destructor effects | Writer's normally completed write establishes initialization on the original external slot. One initialized branch and one uninitialized branch do not establish unconditional initialization. |
| Guard lifecycle | Whole transfer, nullable unwrapping and wrappers preserve a complete carrier. Reconstruction and separating extraction do not. Payload mutation requires the unresolved support argument above. |
| Nested references | Rebinding the intermediate reference and mutating its referent produce different effects. Unknown aliases cannot justify disjointness. |
| Bounds | index < length of the selected sequence authorizes indexing only while that sequence/index observation remains valid. A different sequence or replacement breaks the argument. This alone proves neither loop termination nor total cleanup. |
| Outcomes | No normal ensures on panic/cancellation. Result.Error follows the stated normal postconditions. Deferred invocation produces no premature completion evidence. |
| Boundaries | Same relationships survive formal substitution and encoding. Exact-thread and provider requirements follow the entire retained chain. |
| Ownership end | Partial, ordinary, abnormal and static cleanup resolve every initialized owner and retained incident once, with backing live through its last use. |

The test
[borrowed_completion_preserves_the_reached_owner_for_cleanup](../../crates/bray-compilation/src/compilation/trusted_contracts/tests.rs)
expects rejection of a call to an empty source function forget taking a shared reference to Owner, whose only field is u64.
That expectation does not follow merely from the absence of ensures. P5 expressly preserves harmless shared observation.
If the established call effects are empty and no capability/dependency is lost, the frame lemma permits preservation.
Before changing the compiler to reject this case, establish a contrary rule or correct the expectation.
The earlier status report treated this failure as evidence of missing revocation too confidently.

The 15 failures are observations at one checkpoint, not 15 proved defects or a complete acceptance specification.
No tests were changed or run for this argument.

## Cost and compiler ownership

The semantic state above is proof notation. It does not require a runtime object graph or separate compiler subsystem.
Reuse the existing selected operations, graph, storage identities, dependency terms and immutable results.

A finite analysis with E edges, domain height H and maximum transfer/join cost C has a basic worklist bound proportional
to E times H times C, assuming changed dependencies are scheduled without redundant full rescans.
This is useful only after bounding H and C. Unrestricted relational alternatives can grow exponentially.
"One shared pass" does not establish competitive compilation speed, and this argument supplies no benchmark claim.

Required compiler obligations are a finite demanded vocabulary closed under summaries, a deterministic join preserving
required correlations, and reuse of prepared selection and summary components. Backward liveness and termination
certification answer different questions and need not be flattened into one analysis.

Semantic acquisition identities and provenance can erase if they affect only static judgments.
A zero-sized marker alone does not prove unchanged guard layout or emitted instructions.
Actual locks, reader counts, wakeups, rollback and retained cleanup backing remain runtime work.
Concurrency correctness requires a separate proof of each trusted protocol's state transitions and memory ordering.
No authority proof implies that acquire/release synchronization was implemented correctly.

P11 also prevents claiming zero cost for arbitrary fallible/asynchronous cleanup. Its backing must physically exist
before the obligation is admitted. The cost review must compare equivalent behavior and account for retained storage,
generated cleanup code, LLVM work and linked runtime demand.

## What remains to prove before implementation resumes

| Obligation | Exact missing argument | What would close it |
| --- | --- | --- |
| O1. Predicate support | How the existing rules determine observation/invalidation dependencies for an opaque acquisition predicate, independently of its whole carrier. | A specification-grounded derivation of preservation and revocation, including payload mutation, release, replacement and cross-lock pairing. If the specification is insufficient, present the precise ambiguity for discussion. |
| O2. Finite abstraction with required precision | A concrete finite representation closed under nested references, recursive calls, substitution and guarded carrier relationships. | Define abstraction, join and transfer for that representation. Prove soundness and convergence, and derive the acceptance families above without arbitrary precision cutoffs. |
| O3. Operation coverage and summary adequacy | Every selected source, implicit, deferred and lifecycle operation refines the shared transitions with correct outcomes. | Discharge each transition class against the existing operation inventory, including actual alias substitution and external summary transport. Do not equate an inventory entry with a completed proof. |
| O4. Trusted implementation and realization | Producer contracts match actual lock/host transitions, cleanup ownership and physical backing. | Protocol invariants and outcome arguments for the required consumers, then implementation verification. Source authority and protocol correctness remain distinct. |

O1 decides whether the proposed acquisition API has a valid preservation argument. O2 decides whether the compiler
proposal is both bounded and sufficiently precise. These are prerequisites to approving more analysis machinery.
O3 and O4 expose the assumptions of the compositional theorem and prevent mistaking a local lemma for stack completion.

The next discussion should settle O1 and the precision contract needed by O2. It should not authorize another general
framework on the strength of these conditional lemmas. Once those arguments close, map the necessary state and
transitions to existing owners and remove competing mechanisms. Only then revise implementation issues.
