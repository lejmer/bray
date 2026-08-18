# Axiom 4: Mutation requires unique authority

Mutation requires unique authority over the storage being changed.

Unique mutation authority is a scoped capability. While it is active, the compiler can identify the access path that
holds write authority and the storage region covered by that authority.

Shared observation permits multiple read-capable access paths to the same storage. Mutation requires exclusive write
authority over the reached storage, unless the mutation is performed through an explicit shared-mutation construct.

A shared-mutation construct defines how mutation is mediated. Its contract must state the mutation discipline it
provides, such as synchronization, atomicity, interior mutability, runtime borrow checking, single-assignment
initialization, or another language-defined mechanism.

A shared-mutation construct grants mutation through a specific capability, operation, or guard. The mutation authority
exists only for the storage covered by that construct and only for the duration permitted by its contract.

Aliasing and mutation are checked over access paths and the storage they reach. Two access paths conflict when they may
reach overlapping storage and at least one path holds write authority in a way the other path's capability does not
permit.

Mutation authority is flow-sensitive. The compiler tracks where write authority begins, where it is transferred, where
it is suspended, and where it ends. After write authority ends, compatible observation or mutation authority may resume
according to the aliasing rules.

Reborrowing preserves the authority chain. A mutable borrower may create a narrower temporary mutation authority when
the rules permit it. During that narrower authority, the original mutable access path is temporarily suspended for the
covered storage.

Mutation of substructure follows the same rule as mutation of whole values. A program may mutate a part of a value when
it has unique authority over the storage reached by that part. Disjoint storage may be mutated or observed independently
when the compiler can prove the paths do not overlap.

Interior mutation is mutation. Logical observation with internal effects uses an explicit shared-mutation construct and
a public capability/effect contract.

Atomic mutation is mutation. Atomic operations are permitted through atomic storage whose contract defines the available
operations and memory-ordering semantics.

Synchronized mutation is mutation. Lock-like constructs grant scoped mutation authority through their guard or
equivalent capability.

Runtime-checked mutation is mutation. A runtime borrow-checking construct may grant mutation dynamically, and failed
capability acquisition must be represented as a defined runtime outcome.

Mutation authority cannot be inferred from ownership alone. Ownership may provide access to request mutation authority,
but the right to mutate is governed by the active access path, aliases, initialization state, and capability rules.

When the compiler cannot prove that mutation authority is unique or properly mediated, the program is rejected.
