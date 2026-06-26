# Axiom 9: The core language is small

The core language is built from a small set of fundamental concepts.

A fundamental concept belongs in the core only when it changes the language's semantic model, type system, ownership model, capability model, control-flow model, module model,
or compilation model.

The core favors deep orthogonal primitives. A primitive is preferred when it composes with existing concepts, has clear static semantics, and reduces the need for special
cases elsewhere in the language.

Convenience belongs outside the core when it can be expressed through libraries, standard abstractions, tooling, formatting, derive-like generation, or ordinary composition
without weakening the language model.

Surface syntax may be convenient, but it must lower to a small number of explicit semantic forms. A feature that looks small in syntax but creates broad semantic exceptions
is a large feature.

New core features must justify their interaction with ownership, borrowing, aliasing, mutation authority, effects, generics, traits, modules, diagnostics, and lowering.
A feature that requires many unrelated concepts to special-case it is not orthogonal.

The language grows by strengthening the few concepts it already has before adding new concepts beside them.
