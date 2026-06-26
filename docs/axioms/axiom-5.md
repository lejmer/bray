# Axiom 5: Movement transfers ownership

Moving a value transfers ownership from one access path to another. After a move, the source access path no longer owns the moved value.

A moved-from access path may not be observed, borrowed, moved again, mutated, or destroyed as a complete value unless it is reinitialized according to the
initialization rules.

Copying creates a separate value with a separate ownership story. Copying is available only for types whose contract explicitly supports copy semantics.

The compiler determines statically whether an operation moves or copies. The source and destination ownership states after the operation follow from that determination.

A move may target a whole value or an owned substructure when the type's ownership contract permits it. Moving substructure updates the ownership and initialization state of
the containing value accordingly.

Moving is subject to active borrows, aliases, mutation authority, initialization state, and destruction rules. A value may be moved only when the move does not violate
those rules.

Destruction follows ownership. The old owner does not destroy a value after ownership has moved. The new owner becomes responsible for the value's later destruction unless
ownership moves again.
