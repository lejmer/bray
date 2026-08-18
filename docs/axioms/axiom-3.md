# Axiom 3: Every value has a statically known ownership story

Every value has a statically known ownership story. At every point in the program, the compiler can determine which
entity owns a value, whether the value is initialized, whether the value has been moved, whether the value may be
borrowed, whether the value must be destroyed, and which operation is responsible for its lifetime.

Ownership is part of the static semantics of the language.

A value's ordinary owner is unique. The owner is responsible for the value's lifetime and has the authority to move,
consume, or destroy the value, subject to active borrows, aliases, initialization state, and capability rules.

Ownership may be structural. When a value contains owned subvalues, ownership of the containing value determines
ownership of those subvalues according to the type's ownership contract. The exact compound forms are language-defined,
and their ownership behavior must be explicit and statically knowable.

Ownership may move. Moving a value transfers ownership from the old owner to the new owner. After a move, the old access
path no longer owns that value.

Copying is distinct from moving. A copy creates a separate value with its own ownership story, and is available only for
types with explicit copy semantics.

Borrowing creates a temporary non-owning access path. While a borrow exists, the owner remains responsible for the
value, and some owner capabilities may be temporarily suspended according to the aliasing and mutability rules.

Ownership is flow-sensitive. The compiler tracks ownership state through control flow. When control-flow paths merge,
the ownership state must merge coherently. A program point has exactly one valid ownership interpretation.

Initialization is part of the ownership story. A value may be uninitialized, partially initialized, fully initialized,
moved, or destroyed. Only initialized values may be observed, borrowed, moved, or destroyed as complete values.
Partially initialized values are tracked by the compiler, and cleanup applies only to initialized parts.

Destruction is exactly-once for owned resources. A fully initialized owned value is destroyed exactly once unless
ownership is moved elsewhere or the value enters an explicit ownership construct whose contract changes its destruction
behavior.

Ownership is visible at API boundaries. Function, method, trait, and generic signatures declare whether values are
observed, mutably borrowed, consumed, produced, or returned. A function that consumes a value receives ownership. A
function that returns a value creates a new ownership obligation for the caller.

Mediated ownership is explicit. Ownership-extending abstractions define their ownership and destruction rules as part of
their type and public contract.

Ownership is checked over access paths and the storage they reach. Substructure, projections, temporaries, captures, and
views participate in ownership tracking when they denote owned storage or owned substructure.

The compiler may use conservative ownership reasoning when precision would make the model too complex. Conservative
uncertainty rejects programs.
