# Cleanup storage and panic report ownership

This document refines [composed async execution](composed-async-execution.md) under the uniform cleanup reservation
policy. Cleanup capacity and report storage have concrete owners through construction, suspension, propagation and disposal.

## One cleanup description

The checker owns selected lifecycle actions, completion evidence and semantic effects in its existing checked results.
Guarantee certification and failure-edge planning consume those facts without depending on generated MIR or frame layout.
Admission effects reach both analyses. A fallible construction has a checked failure exit covering already initialized
inputs before lowering starts.

The existing lowering lifecycle domain has one implementation for expanding those selected actions. It realizes
immediate-parent continuations and symbolic storage requirements together. Source lowering supplies initialization guards,
pending values and source correlation. Specialization supplies substitutions and repairs resulting frame descriptors.
Neither adapter repeats semantic action selection.

Storage requirements identify callable frames, result layouts, outgoing incident records and existing child capacity.
They preserve multiplicity. Two simultaneous requirements of the same shape require two records. Concrete references stay
symbolic until ordinary target layout is demanded after the necessary semantic certification. No separate type traversal
predicts capacity. Recursive owned indirection refers to a child's local allowance instead of recursively expanding an
unbounded reservation. Erased ownership retains the descriptor and provider for its concrete allowance.

Checking accounts for semantic admission and discharge before lowering. Concrete realization must preserve the selected
actions' resource and failure contracts. It cannot certify a finalizer by first asking for a layout whose cleanup depends
on that same certification. Existing cycle validation rejects unproved semantic cycles. Published queries remain immutable,
dependency-tracked and demand-driven. Private builders assemble MIR and layouts.

Structural cleanup uses enclosing activation state where its bounded representation permits it. Arrays and dynamic
buffers use checked element traversal rather than duplicating a cleanup body for every element. Actual async finalizer
calls and recursive or erased boundaries retain their necessary activation storage. Capacity belongs to those actual
storage lifetimes, not to a helper introduced merely to separate compiler code.

## Shared admission-domain binding

Providers that exchange owned Bray values bind their cleanup-capacity operations to one shared admission-domain service
before accepting transferable ownership. Host/provider formation validates the service identity and ABI contract. The
binding remains stable while obligations or transferred storage depend on it. Incompatible providers cannot exchange
ownership through an entry that assumes this contract.

Generated admission, activation and discharge operations all use the validated binding. This includes local construction,
imported nominal types, generic instantiations and locally generated cleanup. An ordinary value needs no capacity-domain
flag, and an ordinary move performs no fallible rebinding. Neither the current scheduler nor an image-local registry may
implicitly select a different domain for cleanup.

The service owns shared cleanup-capacity routing. Independent runtime schedulers retain their own scheduling state and
admission. Binding does not reserve every frame in every scheduler. Concrete storage keeps its provider-owned release
operation and lifetime dependency. Domain metadata must not retain unloaded providers after their dependent obligations
and storage have been released.

Provider formation establishes the binding before initialization can publish relevant owners. Lazy registration of a
concrete shape may occur during its first fallible owner admission, before that obligation is established. Mandatory
activation and discharge use existing domain and shape registration without allocating or growing routing metadata.

## Provider dependencies and formation

Code realized into one final product shares that product's admission-domain binding. This includes imported MIR and
static-library contributions under the language's [product-instance rules](../language/modules-and-packages/library-and-executable-products.md#library-products).
Archive packaging does not create a separate product instance. Precompiled native contributions must preserve their
association with the consuming product's binding. Resolving native symbols alone does not establish that association.

A separately formed provider declares its required provider dependencies and service requirements in product metadata.
Before initialization or entry can invoke a provider, the host validates and binds the transitive set of required
providers to the same resident services. The set comes from demanded code, static materialization and cleanup, not every
available package. A newly loaded provider establishes the same contract before its entries become available.

Reuse existing product dependency analysis and lifecycle ordering. Closed initialization or cleanup dependency cycles
are rejected under the existing language rules. Binding does not introduce a second dependency planner or a runtime
mechanism for resolving such cycles. A foreign host must complete provider formation before invoking its Bray exports.

Formation validates required service components across the dependency set before publishing bindings, then establishes
all required bindings before materialization or callbacks can depend on them. Binding itself invokes no application
initializer. Static materialization retains its existing timing, and explicit lazy initialization such as `Once` runs
only when requested by source. No entry becomes available through a failed formation. Failure resolves any state already
initialized by that attempt in dependency order while retaining the services and providers needed for cleanup. Existing
independently live providers remain live. Published bindings remain fixed until unload, including after failure.

## Resident execution routing

Providers bind generated runtime calls to the same resident implementation through fixed, typed native service tables.
The host keeps those tables and their implementation image alive independently of provider retirement. Formation validates
the required service components before provider initialization can publish owned values. A provider load has one permanent
binding and a resident-assigned identity, so unloading and reloading at the same address cannot reuse an earlier host.
Binding survives failed formation: retry uses the same live domain, and the resident image stays loaded until the bound
provider unloads. Synchronous products require only
the host and capacity services. Their service tables must not retain scheduler entry points merely because asynchronous
products use them. Runtime component selection continues to determine which native code is linked.

The resident implementation owns frame reservations and the run interpreter that consumes them. Prepared Rust frame,
activation and terminal-state owners never cross the native ABI as Rust values or foreign raw object storage. Generated
frame callbacks keep their fixed native ABI and operate on admitted context backing. Admission, activation, frame claims
and release all return to the resident owner of that backing.

An independent execution entry selects an explicit retained runtime context. Each context keeps its own scheduler, limits,
worker pool and Main-thread authority. Entering another context scopes and restores task identity, cancellation, incident
ownership, product execution acquisition and root/output context together. It does not inherit a different root's task or
incident destination. Entering cleanup on an already selected context preserves the current run's ownership and lane rules.
A context remains available through its retained cleanup owners after the initiating host stops accepting new work.

While a resident run invokes a generated callback in another provider, that provider's native calls return through its
bound service table to the resident implementation's active thread-local context. Generated frames therefore need no
separate scheduler handle. Every operation that reads runtime thread-local state follows this route, including cancellation,
incident transfer, product execution acquisition, thread identity and attachments, synchronous root callbacks, and panic
and output handling. Routing only task start and polling would split one execution across different runtime states.

### Establishing services before bound calls

A small explicit set of host operations establishes resident services and obtains their validated tables before a product
binding exists. These calls select the resident implementation directly. Ordinary generated runtime calls use the
product's published binding. Whether a startup operation runs before binding is a property of that invocation, not a
blanket exemption for every call to the same operation.

Initial product control selects services from its supplied binding and validates the required host or execution component
before provider initialization. Later control without a supplied binding uses the published binding. An invalid supplied
binding is rejected even when a cached binding exists. A supplied binding cannot change a published domain. No path
silently falls back to a different resident implementation or an image-local runtime.

The typed service tables define one callback inventory shared by native dispatch and compiler service-demand analysis.
Synchronous root observation, completion resolution and incident reporting belong to host services. Execution services
are required only by reachable execution or cleanup that needs them. Sharing host services does not combine independent
schedulers or their limits. Ordinary calls and moves neither repeat provider formation nor acquire per-value domain flags.

### Consuming reserved frame storage

Activation validates the reservation identity, concrete layout and provider compatibility before changing ownership.
A successful activation transfers backing storage, its release responsibility and provider retention together to one
active-frame owner. The reservation becomes empty and can no longer release that storage. Rejection leaves the original
reservation unchanged. Validation of malformed requests does not introduce a new allocation or admission failure during
mandatory cleanup of a valid admitted owner.

Reservation identity distinguishes allocation lifetimes even when an address is reused. Identity exhaustion fails at
fallible admission before accepting an obligation. A stale reservation cannot activate or release a later allocation.
These identities belong to runtime storage owners, not ordinary source values. Existing frame claims and generated frame
release operate on the transferred owner without installing a second independently armed storage-release path.

Final release waits for all frame and result borrows, releases backing through its owning provider, and only then releases
provider retention. Release callbacks and provider destruction execute outside registry locks. Logical-credit discharge
remains separate from physical release and cannot reclaim backing transferred to a frame or retained outcome.

## Admission and outgoing incidents

Each local mandatory cleanup action that can produce a retained failure has an outgoing incident record in its uniform
allowance. The record includes linkage and backing sufficient for the concrete error payload or an escaping panic's
primary data. An Error result and an escaping panic are alternative outcomes of one valid Bray invocation. Independently
owned children, locals and separate actions supply their own records. A structural forwarding step does not reserve
another record merely to relay those failures.

Admission prepares the complete local bundle privately and publishes it atomically with respect to availability. Failure
publishes neither a new owner nor partly available cleanup capacity. Initialized inputs keep their existing ownership
and allowances until construction commits. Ordinary explicit finalizer calls and retries retain their own execution
admission and do not repeatedly spend the terminal cleanup allowance.

A successful action leaves its unused outgoing record for release through the local allowance's discharge. A failed action
transfers the record and payload to the run, panic report or host sink. That storage is no longer unused capacity and
cannot be reclaimed when the original owner ends. Activation records spent logical credits, and whole-owner discharge
consumes spent credits before unused records, preserving the uniform allowance independently of transferred storage. No whole completed frame is retained merely to keep a small error alive.

Result destinations and payload backing may share storage only where result movement and the absence of remaining borrows
prove that the storage can transfer. The error payload's concrete alignment and destruction descriptor remain valid.
Quiesce owned child runs before converting the error to an incident with a synchronous destruction callback. Incident
conversion abandons graceful finalization of the error. Disposal does not start a recursive series of error finalizers.
If quiescence encounters a failure, retain the original Error in its action's outgoing record while the child or quiescence
action supplies its own record for the later failure. Continue settlement through the immediate-parent continuation.
Do not leave the path before retaining the original Error or install a synchronous destruction callback prematurely.

Trusted host or foreign bridges that admit multiple distinct outgoing outcomes use their own explicit bounded allowance.
They cannot consume a sibling action's record or assume the single-result bound of a valid Bray invocation applies.
The native callback bridge that can retain both a published ABI outcome and a caught host panic reserves two outgoing
records. Nested invocations own their own allowance.

## Report values and repeated allocation failures

A protected `PanicReport` is a movable owned header containing its primary cause, source context, message ownership and
head/tail ownership for suppressed entries. The primary header lives in the report value's destination storage. It does
not require a separately allocated primary node. No link points back into this movable header.

A report leaving a mandatory action can also own that action's empty, detached outgoing record. Binding transfers the
record and spends the action's credit once. If the report already carries a child's record, the enclosing action leaves
its own record unused. Structural forwarding moves this ownership with the header. When the primary becomes a suppressed
entry, suppression takes the detached record, moves only the primary data into it and splices the separately owned chain.
The record never contains another complete report. Report destruction releases an unused detached record, while its
original owner's discharge cannot reclaim it. Ordinary catches and allocation-failure construction do not acquire a new
record. Record backing and message backing retain their own provider dependencies through their final release callbacks.

The message representation distinguishes immutable retained bytes from owned backing with its release operation. Retained
provider bytes keep the provider alive. Owned backing transfers exactly once and is released by its owning provider.
Acquiring a provider dependency uses already available ownership state and cannot allocate during construction or transfer.
An allocation-failure report uses its inline cause and source with an immutable runtime message. Initializing that report
requires neither allocation nor a unique emergency report object. Materializing this protected report introduces no new
mandatory cleanup allowance. Its synchronous infallible destruction needs no fresh execution capacity. Existing suppressed
records bring their own backing, which report movement transfers.

A catch result supplies storage for its `PanicReport` payload. Several catches can therefore retain independent reports of
allocation failure in their respective result values. Storing more results in a growing application collection remains an
ordinary application allocation. Its failure does not prevent constructing a report in an already available call/catch
destination. No still-live report storage is recycled to raise a later failure.

[Panic message preparation](../language/expressions/panic-expressions.md) precedes the requested panic. Literals and other
immutable retained bytes need no copy. Exclusively owned immutable backing can transfer when source semantics permit it.
A borrowed message that may change or expire is snapshotted. Snapshot allocation failure initializes an allocation-failure
report at the panic expression's source. It does not truncate the requested message or misidentify which panic occurred.

## Ordered suppression and reporting

Outgoing incident records form an incident-specific owned linked sequence. Head and tail refer to stable records. Forward
links own successors. The collection owner holds any additional tail reference needed for constant-time append. Append
and splice consume already admitted records without allocating. Reporting and destruction traverse iteratively so dynamic
incident count does not become native recursion depth. Detach the successor before releasing the current record so its
forward-owning field cannot recursively destroy the tail. Provider dependencies remain valid through the last release
callback.

Appending a secondary report consumes the action's admitted record for that report's inline primary data. Its existing
suppressed sequence is spliced after that entry, preserving primary-before-suppressed order. The primary's chain ownership
is moved separately rather than recursively embedding another complete report inside each record. An Error attaches its
already owned error record directly. Neither operation allocates a wrapper report or copies a panic message.

A run or host sink detaches owned records while holding its collection lock, then reports or destroys them outside the
lock. Callbacks may reenter and record newly owned incidents into the active destination. They do not mutate the detached
sequence being traversed. Preserve encounter order, existing panic precedence, and the specified ordering of transferred
suppressed chains. Ownership transfer changes neither producer identity nor source context.

Reuse the existing `OwnedCleanupIncident` payload owner and incident domain. Native, bootstrap and product adapters expose
ownership operations over the same records. They do not maintain parallel vectors that copy records at each boundary.
The product host owns reporting policy. It does not supply native ABI conversion helpers back to the runtime.

## Native ownership boundary

Use an explicit shared native ABI layout for the report header and its message ownership descriptor. All participating
providers validate this contract through ordinary product compatibility. Backing storage retains provider-owned release
callbacks and lifetime dependencies. Matching layouts do not authorize releasing another provider's allocation directly.

Call outcomes use a tag and caller-provided report destination. A panicking callee moves a report into that destination
before publishing the panic tag. Cancellation initializes no report. A normal return leaves the report destination
uninitialized. Forwarding moves from a live source destination before releasing its storage. The old machine-word
sentinel encoding cannot carry a pointer to a callee's temporary header.

Synchronous calls use caller-owned destinations. Catch owns its result destination. An independently started run retains
terminal report storage through observation. Foreign and thread-entry boundaries likewise provide destinations whose
lifetimes cover their recovery and transfer contracts. Admission secures persistent destinations before those boundaries
accept obligations.

Provider retirement references preserve code, immutable bytes and release callbacks independently of live-static
dependencies. A live entry, attachment, external owner or cleanup invocation can retain its provider without allocation.
Existing references clone without allocation, including during closure. They do not delay static cleanup. After cleanup,
the host remains retiring until the last reference releases and runtime teardown returns. Only then may it publish the
closed state that permits unloading. Backing release callbacks return before their provider reference is released.

The runtime and bootstrap share report construction, movement, suppression and destruction semantics. Cleanup Error
attachment consumes admitted records directly instead of constructing another allocated `PanicReportData`. Replace the
address-returning construction and suppression contracts together with the compiler's representation and outcome lowering.
Regenerate disposable artifacts under the version-1 policy. Do not preserve the old sentinel path as a compatibility mode.

This representation enlarges values that actually contain a report, including catch results. Successful execution does not
copy a header. Sequential operations can reuse report destinations when their lifetimes do not overlap. Proven nonpanicking
paths need no otherwise-unused propagation destination. Existing result-place and liveness analysis own these storage
choices. Do not allocate one permanent report header for every syntactic call site or add a separate optimization framework.
