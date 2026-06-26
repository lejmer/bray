# Core axiom set

1. **Immutable by default.**

   The default local capability of a binding is read-only observation. Mutation, rebinding, interior mutation, and mutation effects must be explicit. Ownership does not
   imply mutation authority.

2. **Aliasing is explicit, non-owning, and capability-checked.**

   An alias is any additional live access path to the same underlying value, storage, resource, identity, or overlapping region of memory. Owning aliases are forbidden.
   Borrowed aliases, views, projections, iterators, and mediated-ownership handles are allowed only when their capabilities are explicit, non-conflicting, and lifetime-safe.

3. **Every value has a statically known ownership story.**

   At every point in the program, the compiler can determine which entity owns a value, whether the value is initialized, whether it has been moved, whether it may be
   borrowed, whether it must be destroyed, and which operation is responsible for its lifetime. Ownership is part of the static semantics of the language.

4. **Mutation requires unique authority.**

   Shared observation is fine. Mutation is fine. Shared mutation is not fine unless it goes through an explicit construct that advertises synchronization, interior
   mutability, atomicity, or dynamic checking. Aliasing must never hide mutation.

5. **Moves are not copies, but semantic transfers, moving ownership.**

   Moving a value transfers ownership. Copying a value duplicates it. These are distinct concepts in the language.

6. **Resource lifetime is deterministic by default.**

   Owned resources should have predictable destruction points. Memory is the obvious case, but this should apply equally to files, sockets, locks, temporary buffers,
   GPU handles, arenas, etc. Resource safety and memory safety are the same design problem.

7. **No invisible polymorphism.**

   Polymorphic behavior should be declared intentionally. Overload sets, trait implementations, implicit conversions, generic constraints, dynamic dispatch,
   and type-directed behavior should not appear accidentally. Implicit casting is only allowed for literals.

8. **Abstraction is behavioral.**

   The language should use traits/interfaces/protocols as the primary abstraction mechanism. Types do not inherit implementation identity from parent types.
   Behavior is attached through explicit capabilities.

9. **Small core language.**

   The language should have few fundamental concepts, but those concepts may be deep. Prefer deep orthogonal primitives over many shallow conveniences.

10. **A program's dependencies are part of the program.**

    The build graph, source graph, and dependency graph should be visible and reproducible. Vendoring-first. A program's dependencies are part of the program,
    and should not be thought of as ambient environment.

11. **Trusted memory power is explicit and bounded.**

    Unchecked memory operations are available only through a closed trusted capability model. Modules must opt in to trusted declarations, functions must declare the
    exact trusted capabilities they use, and trusted-ness remains local to the declaring function. Public trusted implementations must expose either a safe wrapper or
    an explicitly trusted public contract.
