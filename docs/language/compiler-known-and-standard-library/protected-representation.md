# Protected representation

Some compiler-known declarations have protected representation.

Protected representation means user code can use the declaration according to its public contract, but cannot construct,
inspect, or mutate representation details that the compiler reserves for language invariants.

A protected-representation declaration can still be:

- named in type expressions when its contract permits it,
- moved according to its ownership contract,
- borrowed according to its borrow contract,
- matched through public pattern rules when the declaration exposes any,
- passed through callable surfaces that mention it,
- stored in products, unions, tuples, arrays, boxes, or nullable forms when the containing type rules permit it.

Protected representation prevents user code from depending on representation details that the compiler must control.

`Range<T>`, `Future<T>`, `Task<T>`, `PanicReport`, raw pointers, and selected compiler-known support types can have
protected representation when their owning rules require it.

Protected async frame representation can carry compiler-private layout and operation metadata across compiled package
interfaces. That metadata does not create a source-visible type argument, field, path, reflection result, or layout
guarantee.

Copy behavior for a protected-representation type exists only when the owning language rule defines a copy contract for
that type.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Compiler-provided declarations](compiler-provided-declarations.md)
- Next: [Target profiles and properties](target-profiles-and-properties.md)
