# Axiom 1: Immutable by default

The default local capability of a binding is read-only observation. Mutation is an explicit capability, opt-in.

- An immutable binding owns a value but does not grant local mutation authority.
- A mutable binding owns a value and grants local mutation authority.
- An immutable borrow grants observation.
- A mutable borrow grants temporary exclusive mutation authority.
- Interior mutability grants mutation through a special audited abstraction.
- An immutable owner may move or destroy a value without being allowed to mutate its fields.
- A mutable owner may mutate the value while it owns it.
- A borrowed observer may inspect but not mutate.
- A borrowed mutator may mutate but only under exclusive authority.

An immutable binding cannot be rebound and does not grant mutation authority over the value it reaches.

An owned value and a mutable value are not the same thing. Ownership answers "who is responsible for this value?" while
mutability answers "who may modify this value?". Default immutability is access-path immutability. Deep immutability is
a separate stronger guarantee.

Function parameters are immutable by default as well. There are three types of parameters:
- Owned parameters, and immutable local binding: The function owns the value but cannot mutate it unless it requested
  mutable local authority.
- Borrowed parameter: The function observes only.
- Mutable borrowed parameter: The function has temporary exclusive mutation authority over the caller's value.

This is important because ownership transfer should not implicitly imply mutation permission. We want to avoid "move
into function" becoming a way to bypass immutability.

Field mutation requires mutability of the access path. Field declarations alone do not grant mutation authority. If you
own a struct through an immutable binding, you are not able to mutate its fields directly. If you have mutable authority
over the struct, you can mutate mutable parts of it. If a field is intentionally internally mutable, that must be
visible in the field's type or capability.

Mutation authority applies to an access path and the storage it reaches. Two access paths conflict only if they may
overlap in storage and their capabilities are incompatible.

The compiler's rules can be understood as the following capability-based mental model:
- Read capability: may observe.
- Write capability: may mutate.
- Move capability: may transfer ownership.
- Drop capability: may destroy.
- Share capability: may create observers.

The language adopts reference/access-path mutability. That says "this particular way of reaching the value permits
mutation". Binding mutability is just the local source of a mutable access path. Type-level immutability exists too, but
as a stronger concept, specifically for frozen/read-only data, constants, compile-time values, shared static data, and
safe cross-thread sharing.

Immutable access is shareable only when the reached type is safe to share. Interior-mutable state is not
thread-shareable unless its mutation capability provides synchronization or atomicity.

Read-only access means this access path cannot mutate the reached storage. Stronger optimization assumptions require
frozen/deep-immutable or no-interior-effect guarantees.

To avoid "immutable by default" becoming anti-performance ceremony, initialization is not considered ordinary mutation.
A not yet fully initialized value is not a normal value. It has no stable observable identity yet. Therefore, mutating
fields during construction does not violate the spirit of immutable-by-default. Once initialized and bound immutably,
ordinary mutation requires explicit authority. During initialization, a value may not be observed, borrowed, moved,
dropped as a complete value, or leaked through aliases until it is fully initialized. Partial initialization is tracked
by the compiler, and cleanup only applies to fields that were actually initialized.

Mutation is always local, intentional, and scoped. That means:

- While immutable observers exist, mutation authority cannot be active.
- While mutation authority is active, no hidden aliases can observe or mutate inconsistently.
- Once mutation authority ends, immutable observation can resume.

Interior mutability must be explicit both in implementation types and in public capability/effect contracts.

Functions advertise mutation at their boundary. A function that only observes is callable with immutable access. A
function that mutates requires explicit mutable authority. A function that consumes requires ownership transfer. These
are four different API contracts:

- Observe: No visible mutation and no internal mutation.
- Observe with internal effects: Logical observation, but may mutate internal state such as caches, metrics, lazy cells,
  locks, or refcounts.
- Mutate: The abstract value may change.
- Consume: Ownership is transferred or destroyed.

Capability/effect contracts are part of function, method, trait, and generic signatures. A generic function may not call
an operation with stronger mutation/effect requirements than its bounds declare.

Note: Dropping is a consume operation, not ordinary mutation. Destructors may perform effects, but those effects are
part of the type's destruction contract and must not permit mutation through invalid aliases.

For traits/protocols, immutable-by-default means methods also default to observation. A method should not be allowed to
mutate receiver state unless the method's receiver capability says so. This avoids the classic OOP problem where calling
a harmless-looking method may mutate arbitrary internal state.
