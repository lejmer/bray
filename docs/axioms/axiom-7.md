# Axiom 7: Polymorphism is explicit

Polymorphic behavior is declared intentionally.

A declaration that introduces polymorphic behavior states the kind of polymorphism it provides. This applies to overload sets, generic parameters, generic constraints,
trait implementations, dynamic dispatch, operator behavior, conversions, and other type-directed behavior.

Inference may select among declared meanings. It may not create new meanings. A generic call, overloaded call, trait operation, or a unary or binary expression may infer types or
select an implementation only when the selected behavior already exists as a declared part of the program.

Overloading is opt-in at the declaration site. Functions, operators, or multiple trait applications for the same subject type participate in overload resolution only when
they are explicitly grouped into an overload set or implementation overload family. The overload declaration is part of the public contract.

Trait satisfaction is nominal and declared. A type satisfies a trait through an explicit implementation that is visible to the compiler as a declared relationship between
the type and the trait.

Generic constraints are part of the generic declaration. A generic body may use only the operations guaranteed by its declared constraints.

Dynamic dispatch is explicit at the type or call boundary. A value whose behavior is selected dynamically carries that dispatch model in its type or contract.

Conversions are explicit operations, except for literals. A literal may adapt to a target type when the target type is known and the literal value is valid for that type.
Non-literal values do not implicitly cast, widen, narrow, reinterpret, allocate, borrow, clone, move, or dispatch through conversion-like behavior.

Imported declarations do not silently extend overload sets, implementation overload families, operators, conversions, or trait behavior. Extension of polymorphic behavior
must be explicit at the declaration site or import site according to deterministic language rules.

Type-directed behavior has a visible declaration site and deterministic lookup rules. The compiler may use type information to select declared behavior, but it may not
discover new behavior through accidental naming, ambient imports, hidden conversions, or undeclared structural coincidence.

When multiple declared polymorphic choices could apply, resolution must produce exactly one best choice according to the language rules. Ambiguous polymorphism is rejected.
