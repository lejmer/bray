# Runtime ownership and authority proof

Discussion draft, 2026-10-13. Baseline: `e3f56424` on `feature/bra-620-repair-synchronization-authority`.

Implementation is paused for this review. This argument examines the existing language rules before choosing more
compiler machinery. It introduces no syntax, language rule, runtime representation, or implementation mandate. The
earlier [replacement plan](runtime-stack-rule-coverage.md) supplies candidate mechanisms, not proof premises. Its
implementation sequence remains subject to this review. Historical results stay in [BRA-620 remaining
work](BRA-620-remaining-work.md).

The design hypothesis is that ownership, dependency validity and contract evidence admit one compositional transition
semantics. The expected simplification is removal of competing interpretations of those transitions, not removal of
necessary semantic distinctions. No production code growth is authorized by this document. Reconsider the hypothesis
if a required operation needs hidden semantic state, an unsupported preservation rule, or an unbounded proof search.

## Result and limits

The argument now defines shared transition semantics, a finite analysis vocabulary, correlated carrier summaries, and
protocol invariants for acquisition, waiting, Once and retained cleanup storage. The soundness and convergence
arguments are separate from the required acceptance derivations and from implementation performance.

The acquisition-preservation argument is conditional on the closed-dependency interpretation submitted to the user.
That interpretation fixes an acceptance rule omitted from the prose specification. It is not silently adopted here.
The first draft correctly separated non-forgery from preservation, but stopped before defining a concrete solution.
The candidate solution below uses a historical acquisition fact and a live complete carrier.

Some existing negative expectations also need correction or a contrary specification rule. Shared observation with no
relevant effects need not revoke evidence, and a possible panic does not by itself invalidate existing evidence.
Normal postconditions still cannot be imported onto an abnormal path.

This is a written proof of the described model under its stated assumptions, not a machine-checked formalization or
verification of the current compiler. No claim of accepting every semantically safe Bray program is made. Existing
bounded contract and termination reasoning still defines what can be proved.

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

These are semantic distinctions, not a request for five new compiler types or maps. Initialization, active variant and
partial ownership belong to the relevant ownership/storage occurrences. Existing compiler types may represent them.

Each distinction has a separating example. A moved guard retains acquisition authority but an old pointer to
physically relocated inline storage does not become valid at its new location. Two guards with equal fields need not
have equal authority. A borrow can point inside a lock while depending on a different guard. Replacing a reference
slot changes which object it reaches without modifying the former referent. A completed finalizer can leave
destruction pending. Removing any of these distinctions conflates behaviors required by P2–P9.

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

Write T(o, S, k) for the possible successor states of operation o with outcome k. Outcomes distinguish normal return,
panic and cancellation. Result.Error is a normal returned value. Divergence has no completed successor and must still
be considered when proving total execution.

Let I(S) mean:

1. Every live noncopy ownership occurrence has one responsible owner. Its initialized parts and pending obligations agree.
2. Every permitted access has valid initialization and compatible capability for all reached storage it may access.
3. Every usable trusted fact has authorized provenance and valid carrier/dependencies.
4. Each discharged lifecycle step was completed or discharged by its specified rule. Pending work and backing retain an owner.

The desired theorem is conditional on valid trusted implementations:

> Starting from a state satisfying I, every accepted operation produces only successors satisfying I.
> Therefore every finite accepted execution prefix preserves I.

The second sentence follows by induction on execution length once the first is proved for each selected operation. It
does not prove termination, concurrency correctness or absence of application deadlock.

### Evidence can survive by framing

Let Changed(o, S, k) cover every state component that the operation may change on that outcome. Let Support(e) cover
every component whose change can invalidate the evidence, including carrier validity, resource epochs and observation
dependencies.

If e is usable before the operation and Changed is disjoint from Support, e remains usable afterward. Every conjunct
in Usable has the same value, so their conjunction remains true. This is the frame argument.

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
External storage remains external. Borrows into relocated storage must satisfy the movement restrictions. A
heap-owning value can move while its allocation stays in place.

Partial transfer uses the same rule on the selected part, but is rejected if it separates a whole carrier from its
guarantee. Checking this only at eventual destruction is insufficient. A path can diverge after extraction.

### Safe operations cannot forge authority

Assume no trusted source has established an acquisition for a fabricated guard. Consider the first operation claimed
to make that guard authoritative.

Construction of equal fields establishes representation, not trusted provenance. Projection establishes no owner.
Copying ordinary fields carries no witness authority. Whole movement can only transfer existing authority. A
separating partial move is prohibited. Branch joins cannot create a source missing on an incoming alternative. A safe
call can introduce authority only through an authorized producer whose requirements hold.

These cases exhaust the ways value construction, transfer and calls could introduce that authority in this model. None
supplies it under the assumption, a contradiction. Explicit trust assertions and incorrect trusted implementations are
outside this safe-code conclusion. Wrapper checking must still obey P10.

Cross-lock pairing adds a further premise: the producer and accessor contracts bind the complete carrier to the exact
lock, and that relationship cannot be changed independently while authority survives. The witness rules prevent
fabrication. They do not prove that a particular producer API states the correct relationship.

### Borrowing preserves ownership and restricts interference

Borrow creation retains the existing owner and establishes only the selected capability. Reborrowing cannot increase
authority. Mutation, relocation and retirement that conflict with a live borrow are rejected by P2. A reference to a
reference first reaches the reference slot. Updating that slot is distinct from mutating its old target.

An ordinary mutable T borrow does not grant ownership to move out an arbitrary T. Replacement is legal under P8's
restoration rule. Consequently safe payload operations can preserve the proposition "this slot contains a T" while
changing its contents. This does not preserve the old T's identity, old interior references, or an arbitrary predicate
about its value. Raw trusted operations must establish their own poststate rather than inherit that conclusion.

For Uninit, a checked write adds initialized ownership only after commit. Move-out removes it. Ending the protected
storage with an owned T still inside violates P7. No new initialized-storage language type is required to state this
rule.

### Calls and cleanup compose the same rules

A call checks requirements in entry state, performs argument ownership transfer, and applies the callee's effects. It
adds only postconditions established on the reached normal outcome. Guard eligibility uses the captured entry state.
Unchanged evidence can survive through the frame lemma without a redundant ensures clause.

A destructor is this call transition with a consuming receiver, followed by cleanup of remaining initialized parts. A
finalizer uses a mutable receiver and leaves an initialized owner. Its postcondition decides completion separately
from the returned error. with exit retains its capability during the call and handles the pending outcome under P9.
Replacement adds the P8 ordering requirement around old cleanup and installation.

Thus the same precondition/effect/completion rules apply at explicit and implicit calls. Different ordering and
receiver modes remain real semantics. "Shared cleanup" cannot mean erasing those distinctions.

For example, a Writer destructor that initializes a borrowed slot establishes a fact about that external slot on
normal completion. Destroying Writer does not destroy the slot. A fact tied instead to Writer's own destroyed storage
cannot escape. Exact referent identity decides which case applies.

Cleanup conservation is an induction over the selected cleanup sequence. Each step consumes, transfers or retains its
obligation, and records resulting owned outcomes. Remaining represented parts retain their own obligations. By P11,
backing cannot be released while a transferred incident or activation owns it. This proves no duplicate or lost duty
only if the selected sequence covers every live obligation on each outcome.

## Finite analysis and function boundaries

The proof target is a terminating analysis of the required rule families, not discovery of every mathematical truth
about an arbitrary Bray program. The language already distinguishes proven from unknown and requires bounded
implication and supported termination reasoning. Unknown cannot satisfy a trusted requirement.

Use one selected generic body or a mutually recursive component of selected bodies as the analysis boundary. Do not
recursively instantiate runtime call depth or concrete executions. External summaries are finite inputs. Type
selection and concrete specialization retain their own existing termination/resource contracts.

### Fix the vocabulary before solving

The following finite vocabulary is derived from selected operations and imported contracts:

- R contains formal inputs, named storage, results, selected temporaries, allocation sites, and provider/thread roots.
- P contains the finite paths explicitly distinguished by those operations and contracts, including every prefix.
  Dereference is an edge between slots and referents, not a field appended indefinitely.
- D contains demanded initialized/variant/capability/lifecycle distinctions and contract proposition templates.
  Templates refer to current subjects or saved entry subjects. Substitution renames subjects and does not repeatedly
  expand arithmetic expressions or recursively inline callees.
- G contains the discriminant and entry-guard alternatives needed by those templates. An arbitrary execution history
  does not become another guard.
- B contains bounded relative binders for carriers and their fields/dependencies in each value-contract schema.
  An existential binder describes the actual current value, not a global acquisition identifier.

Recursive reference/type structure uses a finite graph. A traversal beyond a distinguished path reaches a summary
region. It does not become equal to a previously visited concrete object. A summary region means any compatible
descendant, including aliases, not a uniquely identified object. Effects on it are weak updates. Explicitly demanded
successive references, such as a reference slot and its referent, remain distinct.

This is a precision boundary based on the selected operations, not a machine-dependent retry count. A recursive path
whose exact relation is not expressible retains a conservative dependency and cannot satisfy an exact-identity
requirement. The language's rule that unknown overlap conflicts still applies. Do not claim acceptance of arbitrary
recursive alias relationships.

### State and join

A body state contains finite sets of possible targets, dependencies and pending obligations, together with definite
initialization, capability and evidence facts over this vocabulary. Facts about alternatives retain their guard. A
carrier fact is one relationship, for example:

```text
there exists c:
    current value owns c
    and c carries acquisition evidence
    and c.resource = current value.lock
```

The three clauses are not three unrelated sets. The binder's scope preserves their correlation. A guard and lock
selected together therefore remain paired after a join. Two independent selections do not inherit that pairing. For an
array of guards, an element-relative schema describes each initialized element's own carrier. It does not assign one
global carrier to every element. A nonconforming write invalidates the uniform schema.

Whole transfers rename the current owner and relative binders consistently. Wrapping and unwrapping change the
projection to that owner without changing its binder. At a join, alpha-equivalent relative facts survive under the
same discriminant. Facts whose relationships disagree are forgotten. Possible effects and dependencies accumulate.
Retain a concrete equality only when every represented alternative supplies it.

At a new iteration or allocation, a site denotes a fresh dynamic occurrence. Existing live occurrences summarized at
that site cannot be identified with the new occurrence. Recency/singleton information permits strong updates only
where uniqueness is established. Unknown or summarized aliases require weak updates and cannot revive stale epochs.

Let gamma(A) be the concrete states represented by A. Each primitive abstract operation must satisfy:

```text
{ S' | S in gamma(A), S' in T(o, S, k) } subset gamma(transfer(o, A, k))
gamma(A) union gamma(B) subset gamma(join(A, B))
```

These inclusions follow componentwise: possible alternatives are retained, unsupported definite facts are removed, and
a complete carrier relation is either transported intact or unavailable. Authorization uses only definite facts.
Treating an unknown referent as unchanged would violate the first inclusion.

### Calls and recursive components

A summary records guarded input-to-output relationships, possible effects for each outcome, and retained dependencies.
It uses the same finite relative subjects as body analysis. At a call, substitute actual subjects consistently. If
actual arguments alias, overlapping effects combine. Formal parameter inequality never proves actual disjointness.

Compute possible behavior by least fixed point from no discovered behavior. Verify definite summary candidates against
all discovered behavior and retain only candidates justified on every relevant return. Candidate dependencies inside a
recursive component form simultaneous equations, not independently published provisional promises.
Reachability/effects and candidates must stabilize together before any result is externally usable. An unproved
requirement must not make a possible execution disappear. Recovery never publishes authority from a rejected
operation. Emit deciding diagnostics from the stabilized state.

Partial correctness of recursive contracts follows induction on finite execution derivations. A finite derivation has
a last returning callee and ultimately a base return. Every base return must establish its claimed evidence. A
recursive cycle without a returning execution cannot manufacture a runtime result. It also cannot prove total. Total
certification separately needs the specified well-founded execution argument, including implicit cleanup.

With fixed finite R, P, D, G and B, the possible bit relations and normalized candidate schemas are finite. Order
abstract states by increasing represented executions. Joins and transfers must be monotone in that order. A worklist
reaches a fixed point because every strict update moves upward in a finite lattice. New selected dependencies must
enter the component before solving, or restart that private component with the enlarged finite vocabulary. Neither
partial results nor an arbitrary iteration limit are publishable semantic evidence.

This proves a termination strategy and a sound representation. It does not establish a cheap worst-case bound: guard
alternatives and relation arity can make the finite domain large. Use demanded schemas and shared relative subjects,
not the powerset of complete execution histories.

### Required precision

The acceptance argument is structural over the operation families used by the runtime:

| Required fact | Why the representation retains it |
| --- | --- |
| Whole guard move and wrapper transfer | One carrier schema undergoes consistent renaming, including its resource relationship. |
| Present nullable acquisition | The present alternative owns the whole carrier. The absent alternative has no carrier or release duty. |
| Payload address versus guard lifetime | The reference graph records the physical target while a distinct edge retains the capability. |
| Nested references | The reference slot and its target are distinguished nodes whenever the selected operation uses both. |
| Uninit commit and destructor completion | Initialization is a fact about the resolved external slot. Retiring a different carrier does not erase it. |
| Safe typed replacement | The replacement transition establishes an initialized destination on every propagated exit after cleanup begins. |
| Bounds on the selected sequence | The proposition template refers to that sequence, index and their observation versions. A different sequence cannot substitute. |
| Generic/source/imported calls | Formal subjects and relative carrier binders retain the same meaning under substitution. |
| Recursive forwarding of a guard | The finite candidate is the input carrier relationship transported to the result. Base returns and recursive transitions preserve the same schema. |

These derivations do not establish acceptance of every valid program. In particular, arbitrary nonlinear arithmetic,
unbounded recursive alias equalities and arbitrary path correlations are not smuggled in as obligations of this
subsystem. Any stronger required acceptance needs a language-defined derivation, not a more expensive solver by
default. Source callers use the same published summary as imported callers. They do not recheck a body for extra
caller-specific precision. This keeps the precision boundary independent of source availability.

## Acquisition under the proposed closed-dependency interpretation

This section depends on the interpretation submitted to the user during this review. It is not yet a new normative
rule or an authorized compiler change.

A predicate's semantic inputs consist of its arguments and the dependency subjects exposed by their contracts.
Distinguish a dependency on a subject remaining alive from a dependency on that subject's contents remaining
unchanged. For a body-defined predicate, the selected observations determine support. For a compiler-known predicate,
its specified meaning supplies additional support. For a user-defined opaque predicate, conservatively retain all
state reachable through its semantic inputs that the contract permits it to observe. Unknown raw-pointer reachability
remains unknown memory, not an empty support set. An undeclared hidden mutable dependency cannot be silently supplied
by the caller.

This reading is consistent with observational predicates, explicit dependencies and the complete foreign contract
rule. The prose specification does not explicitly define this extraction algorithm. Adoption therefore needs the
user's agreement on this acceptance rule. Purity alone was not an adequate argument in the first draft.

Let G own an immutable empty marker M and an immutable lock reference L. The producer introduces evidence about M
carried by the complete G. The semantic meaning of that evidence is historical: this carrier was produced by a
successful acquisition. It need not be a live observation of a changing lock bit or payload. G's live ownership and
dependencies separately determine whether that evidence is usable for an operation.

M has semantic occurrence identity, not a numerical address identity. Equal empty representations do not equate two
occurrences. Its observational borrow reaches M, not every field of its containing G. G's dependency on L keeps the
resource alive. That dependency does not say every byte in L must remain unchanged.

A payload write changes V, not M, G's complete ownership, or L's retained lifetime. Provided it is a legal typed
operation that preserves initialized storage, each conjunct required for G's acquisition evidence remains valid. The
frame lemma therefore preserves the evidence. A predicate that explicitly observes V does not receive this proof.

Release consumes G. Its historical proposition need not become false: there is no longer a live carrier from which
safe code may use it. Reacquisition produces a new carrier occurrence. It cannot revive the consumed occurrence. This
separates mathematical truth from currently available authority without adding a runtime epoch field.

The complete carrier prevents cross-lock substitution. L is immutable while G survives, and partial extraction cannot
transfer the guarantee to a reconstructed carrier. Whole movement transfers G, M and L's relationship together. A
copied marker, an equal constant, or a borrowed marker placed beside a different L supplies no producer provenance. An
internal helper receives no exemption from these requirements.

Try-acquisition attaches the complete G to the present result alternative. Absence introduces no owner. An accessor
used by the contract returns the marker view under the same presence guard. Its checked projection relationship
suffices. Evaluating arbitrary const bodies again at every call is unnecessary.

Once illustrates why authority and protocol eligibility must remain separate. Its attempt marker can prove that this
is the unique admitted attempt, including the right to roll back. A reentry flag can make publication illegal without
destroying that cleanup right. Publication must check the actual attempt state before the commit interval. An opaque
immutable-marker fact must not be used to assert that reentry has not occurred.

Raw-memory predicates do not contradict this argument. initialized_as(pointer) explicitly observes the pointed
storage. A write or move affecting that storage must preserve, re-establish or invalidate that fact according to its
selected semantics. Calling a relation opaque never removes such an exposed dependency.

## Acceptance obligations and existing evidence

Sound rejection of unsafe programs and required acceptance of valid programs are separate obligations. The following
cases derive from P1–P11, rather than from the current failure count.

| Family | Required derivation |
| --- | --- |
| Initialized payload | A checked commit followed by a valid ordinary payload borrow leaves an initialized T after legal replacement and borrow completion. Move-out removes that ownership once. No arbitrary payload predicate is preserved. |
| Destructor effects | Writer's normally completed write establishes initialization on the original external slot. One initialized branch and one uninitialized branch do not establish unconditional initialization. |
| Guard lifecycle | Whole transfer, nullable unwrapping and wrappers preserve a complete carrier. Reconstruction and separating extraction do not. Payload mutation uses the conditional support argument above. |
| Nested references | Rebinding the intermediate reference and mutating its referent produce different effects. Unknown aliases cannot justify disjointness. |
| Bounds | index < length of the selected sequence authorizes indexing only while that sequence/index observation remains valid. A different sequence or replacement breaks the argument. This alone proves neither loop termination nor total cleanup. |
| Outcomes | No normal ensures on panic/cancellation. Result.Error follows the stated normal postconditions. Deferred invocation produces no premature completion evidence. |
| Boundaries | Same relationships survive formal substitution and encoding. Exact-thread and provider requirements follow the entire retained chain. |
| Ownership end | Partial, ordinary, abnormal and static cleanup resolve every initialized owner and retained incident once, with backing live through its last use. |

The test
[borrowed_completion_preserves_the_reached_owner_for_cleanup](../../crates/bray-compilation/src/compilation/trusted_contracts/tests.rs)
expects rejection of a call to an empty source function forget taking a shared reference to Owner, whose only field is
u64. That expectation does not follow merely from the absence of ensures. P5 expressly preserves harmless shared
observation. If the established call effects are empty and no capability/dependency is lost, the frame lemma permits
preservation. Before changing the compiler to reject this case, establish a contrary rule or correct the expectation.
The same test also expects may_fail(&owner) to fail. A possible panic alone does not prove mutation or loss of
authority. Existing evidence survives on the panic path until its dependencies are actually invalidated. Only the
normal ensures must be withheld. The earlier status report treated this failure as evidence of missing revocation too
confidently.

The 15 failures are observations at one checkpoint, not 15 proved defects or a complete acceptance specification. No
compiler tests were changed or run. A temporary Python enumeration checked the packed-ticket invariant for moduli 2, 4
and 8, covering 4, 16 and 64 states. It also illustrated the cross-pairing and generation-wrap counterexamples. These
finite examples check the reasoning but do not replace the general arguments.

## Coverage by composition

A new compiler-wide IR is unnecessary. Each selected operation needs one semantic description composed from the
existing storage, call and outcome transitions. The [closed inventory](runtime-stack-coverage-inventory.md) supplies
the source variants, hooks and roles to map.

| Selected operation family | Composition and preservation argument |
| --- | --- |
| Literal, name, projection, pattern and presence observation | Observe an initialized permitted path. Preserve ownership. Add only the selected ordinary fact. Borrowed observation retains its dependencies. |
| Construction, copies, argument/result/field transfer | Check input modes, admit new local obligations, then commit initialized parts. A copy uses its selected copy contract. Failure cleans committed parts and retains untransferred owners. |
| Borrow, reborrow, address, cast and view | Check reached storage and capability. Borrow retains a loan, address retains provenance without a loan, and a cast cannot invent missing provenance. |
| Typed replacement, raw read/write, Uninit and buffer operations | Use their selected copy/move and commit semantics. Update only resolved targets. Prefix ownership advances after successful element commit and retreats before that element becomes reusable. |
| Unary/binary operations, conversion, indexer, iterator, defaults and lifecycle declarations | Use the same selected-call rule, including implicit argument conversions and cleanup. A name or syntax category grants no exemption. |
| Branch, match, short circuit, loop and propagation | Restrict to the selected edge, join using the finite domain, and perform the ownership exits that edge crosses. Result.Error and panic remain different outcomes. |
| Async invocation, suspension, start, await and terminal observation | Invocation transfers captures into an inactive owner. Execution effects start only when driven. Terminal observation transfers the result once. Suspension retains all dependencies and stable-storage requirements. |
| Ordinary/abnormal/with/static cleanup | Compose selected calls in specified order, keeping pending outcomes and owned incidents. Abandonment is permitted only on its specified abnormal path. |
| Atomics, volatile access, assembly and foreign calls | Apply the validated operation or foreign contract, including changing-memory effects, target availability and explicit trusted assumptions. No generic harmless fallback is valid. |
| Recovery and catastrophic termination | Recovery establishes no authority. Catastrophic termination has no source cleanup promise. Neither proves a normal postcondition or total execution. |

Every row preserves I by the earlier local lemmas and by its selected operation's contract. A sequence preserves I by
induction. A branch preserves I because each successor does. A loop preserves I for every finite prefix by induction,
without a claim that the loop terminates. Calls use the recursive-summary argument. This covers semantic composition.
Mechanical implementation conformance still has to show that no selected operation or outcome is omitted by its
adapter. That is an implementation obligation, not permission to discover new semantics one failed consumer at a time.

## Trusted protocol realization

The compiler proof ends at explicitly trusted operations. Each protocol must then supply an inductive invariant and a
publication point. The following are requirements on the intended implementation, not claims that the current code
already satisfies them.

### Acquisition and synchronization

For an exclusive lock, maintain a one-to-one correspondence between each admitted acquisition and either a pending
position or the current exclusive permit. Only the current permit can produce a guard. Release consumes that permit
once and makes its successor eligible. A failed try-acquire produces no permit. For a reader-writer lock, each reader
guard accounts for one reader contribution. A writer permit excludes all readers. Pending-writer registration blocks
new readers and must itself be released on any path that abandons acquisition.

The linearization points are the atomic transitions that reserve, acquire and release these rights. Atomic
acquire/release edges must carry the protected payload's visibility, as required by [atomic
contracts](../language/async-and-concurrency/atomic-operation-contracts.md). An address or a fabricated counter value does not grant the corresponding right.

The trusted API must expose only authorized protocol transitions and payload views. Returning a pointer plus
permissions sufficient to borrow the entire control state is as unsafe as returning that state directly. A safe
caller could then mutate control atomics without consuming its guard. Internal-access acknowledgement is not a
trust boundary. Address-calculation helpers must not publish those broad permissions, and release helpers must
consume the acquisition rather than expose an independently callable counter update.

Finite counters need an invariant preventing overlapping live identities, not an assumption that wrap is unlikely. For
this proof, use a packed ticket representation with next and serving in one supported atomic word. Reserve by CAS only
while the modular distance is below the number of distinct tickets minus one. Release advances serving modulo that
range. The atomic pair prevents a stale separate-counter snapshot from approving an overfull reservation. Waiting for
admission at capacity creates no ticket. Already admitted tickets remain ordered and distinct. Release must wake the
eligible successor or all competing native waiters. Waking an arbitrary non-successor alone could strand the only
acquisition allowed to proceed. Progress remains conditional on scheduling and preceding owners releasing their locks,
so ordinary lock acquisition is not certified total. Use explicit modular operations, not potentially panicking
ordinary addition in a release path. Use acquire-release CAS for the shared packed state and acquire loads for
permission observation, so intervening reservations carry the preceding release visibility. A 32-bit word with two
16-bit counters uses the atomic width already required by these locks. At most 65,535 tickets can be outstanding.
Additional callers wait before admission, so this is not a bound on the number of calls or a new failure result. This
selects a concrete correct protocol for the proof. Changing the implementation still requires resuming implementation
work.

### Waiting without lost ownership or notifications

Condition wait consumes the old guard and retains the mutex owner dependency. After release there is no old
acquisition left to release again. Only successful reacquisition establishes the new guard. Abnormal failure in
between therefore does not require restoration of the consumed guard.

A finite generation word alone needs a wrap argument. An arbitrarily delayed waiter can observe the same word after a
complete wrap despite intervening notifications. For this proof, use a registered wait occurrence with a monotone
pending-to-signalled state that is not reused during that wait. Register while still protected, then release the
mutex, then park only if still pending. A notification racing parking changes the observed state or wakes the waiter.
Spurious wakes only cause rechecking.

The registration remains alive until every notifier that can touch it has finished. Removal and wake completion are
serialized by the queue lock. Use a stack registration owned by the synchronous wait call. Its storage cannot move
while registered. A short internal queue lock serializes registration, signal selection, native wake completion and
removal. The notifier marks a selected pending record signalled and performs the infallible native wake before
dropping that lock. The waiter removes its record under the same lock before returning, so no notifier can retain its
address. No allocation or application callback occurs under the queue lock. Notify-one selects one pending
registration. Notify-all marks and wakes all pending registrations. Already signalled records are skipped until their
waiter removes them. A spurious return removes its registration by the same rule before reacquiring the mutex. The
logical notification generation advances through those signal transitions. Correctness does not depend on an unbounded
physical integer. Reuse the existing registration machinery only if it has this same lifetime contract. A notification
is not itself an acquire edge for the protected payload. Reacquiring the mutex supplies that edge.

### Once, host ownership and cleanup storage

For Once, the abstract states are Empty, Running(attempt, exact attachment), Failed(attempt), and Ready(value). Only a
successful Empty-to-Running transition creates the attempt. Reentry changes Running to Failed before propagating
panic. That transition is irreversible for that attempt, including when user code catches the panic. Commit requires
the same still-valid attempt and a fully initialized T. Publish Ready with release ordering only after the write
commits. Ready readers acquire before accessing T. Error or panic resolves any local T and returns the cell to Empty
without publishing it. Exclusive destruction resolves Ready's T once and resolves no T in Empty. Generic payload
lifecycle remains subject to admissibility. A synchronous destructor cannot hide an unresolved fallible or
asynchronous finalizer merely because T lives in Uninit. The owner's selected lifecycle or an explicit ownership
transfer must resolve it. If no admissible path exists, the use is rejected under the existing lifecycle rules, as it
would be for an ordinary owned field. Do not infer unlimited T support from the absence of a source bound. This
supplies the abstract ownership transitions for the [specified Once
behavior](../language/async-and-concurrency/standard-library-concurrency.md#one-time-initialization).

For host/attachment/provider state, storage retention, admission and access permission are different rights. A lease
requires the correct live instance and acquisition. Closing rejects new admission while existing disposal rights
remain valid. Quiescence precedes backing release. A TLS or OS thread number cannot substitute for an exact attachment
identity. Foreign callbacks retain code, storage and context until their final return to resident code. Reentry
observes an explicit forming/active/closing state and cannot reacquire a partially committed transition.

For cleanup backing, each region has one storage owner and zero or more live users represented by retained ownership
or dependencies. Admission failure publishes no new obligation. Transfer changes that owner without releasing storage.
The last user, including an incident or activation, must finish before release. Induction on these transfers proves
that source discharge cannot free a still-used region and that two outcomes cannot both own its final release.
Concrete sizes, alignments and placement follow realization. Scalar accounting alone is not physical backing.

These invariants make the producer assumptions auditable. They do not prove an unchanged implementation correct merely
because it has corresponding flags. Verification of the actual atomic operations, unwind edges and emitted lifetimes
belongs to implementation conformance. Neither a new compiler analysis nor a new language feature follows from a
protocol implementation failing one of these invariants.

## Cost and compiler ownership

The semantic state above is proof notation. It does not require a runtime object graph or separate compiler subsystem.
Reuse the existing selected operations, graph, storage identities, dependency terms and immutable results.

A finite analysis with E edges, domain height H and maximum transfer/join cost C has a basic worklist bound
proportional to E times H times C, assuming changed dependencies are scheduled without redundant full rescans. This is
useful only after bounding H and C. Unrestricted relational alternatives can grow exponentially. "One shared pass"
does not establish competitive compilation speed, and this argument supplies no benchmark claim.

Required compiler obligations are a finite demanded vocabulary closed under summaries, a deterministic join preserving
required correlations, and reuse of prepared selection and summary components. Backward liveness and termination
certification answer different questions and need not be flattened into one analysis.

Semantic acquisition identities and provenance can erase if they affect only static judgments. A zero-sized marker
alone does not prove unchanged guard layout or emitted instructions. Actual locks, reader counts, wakeups, rollback
and retained cleanup backing remain runtime work. Concurrency correctness requires a separate proof of each trusted
protocol's state transitions and memory ordering. No authority proof implies that acquire/release synchronization was
implemented correctly.

P11 also prevents claiming zero cost for arbitrary fallible/asynchronous cleanup. Its backing must physically exist
before the obligation is admitted. The cost review must compare equivalent behavior and account for retained storage,
generated cleanup code, LLVM work and linked runtime demand.

## Proof status and implementation boundary

The derivations now provide an explicit finite analysis strategy, correlated carrier transport, common operation
semantics, and protocol invariants. They support the required acceptance families without a separate proof algorithm
for each runtime consumer. They are not a machine-checked verification of the compiler.

One semantic decision remains pending with the user: the closed-dependency interpretation for opaque predicates. The
acquisition preservation derivation above is conditional on that decision. It cannot be marked complete merely because
no implementation experiment has contradicted it.

After that interpretation is settled, remaining work separates as follows:

| Work | What reasoning has established | What implementation must establish |
| --- | --- | --- |
| Evidence support and authority | Conditional framing and carrier argument, with exact resource pairing and invalidation. | Every selected operation uses the agreed support rules, including imports and opaque foreign effects. |
| Analysis | Finite vocabulary, sound conservative transfer/join, recursive partial-correctness argument, required carrier correlations. | Concrete data structures implement those relations and reach the same result independent of query order. |
| Semantic coverage | Every operation family decomposes into the shared transitions with its actual outcomes. | Exhaustive adapters cover the concrete selected variants and preserve diagnostics. |
| Protocols | Ownership, publication, overflow, wait-registration, reentry and backing invariants. | Actual atomics, foreign calls, unwind paths and generated storage implement those invariants. |
| Costs | No runtime witness registry follows from the proof. Bounds and mandatory backing costs are explicit. | Compile time, peak memory, produced layout/code size and native performance meet the numerical targets. |

Do not resume implementation solely because this table distinguishes the remaining tasks. The user paused
implementation to discuss the argument. No language syntax, specification text, compiler code or test expectation has
been changed by this review.
