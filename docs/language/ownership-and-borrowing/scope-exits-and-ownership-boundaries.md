# Scope exits and ownership boundaries

A scope exit resolves local ownership state for that scope.

Initialized local owned values whose ownership remains in the scope are destroyed when the scope exits.

Values moved out of the scope are not destroyed as local owned values.

Partially initialized local values resolve only initialized subparts.

Reachable exits from a region must merge to coherent ownership, borrowing, initialization, destruction, finalization, capability, effect, and fact-context state.

A value can cross a boundary only when the destination can preserve every dependency contract carried by that value.

Boundaries include:

- returning from a callable,
- yielding from a value-producing region,
- storing into a field, variant payload, tuple element, array element, or box storage,
- assigning to an existing access path,
- passing an argument to a callable,
- capturing into an async computation, task, or thread,
- forming a trait view,
- using or exporting a declaration surface.

A destination preserves a dependency contract when the destination's lifetime, ownership state, capability state, and semantic contract are proven to keep every required storage, borrow, capability, and obligation valid until the destination no longer uses or owns the value.

Storing a dependency-carrying value never extends the source storage or scoped capability that the dependency requires.

If the destination could outlive required storage or a required scoped capability, the transfer is rejected.

If a dependency contract cannot be represented in the destination's compiler-visible semantic contract, the transfer is rejected.

Function, method, constructor, lifecycle, lambda, async, and implementation bodies are checked against their inferred dependency contracts.

At each normal exit, the result value's dependency contract must be derived from parameters, receiver state, owned input values, async or task state available to that body, or other storage and capabilities that outlive the returned value.

Returning a borrow of local storage that ends at the callable exit is rejected.

Returning a value that owns local state is valid when ownership moves into the result and the value's dependency contract no longer depends on the local binding.

When multiple control-flow exits can produce a value, the merged dependency contract conservatively preserves every dependency that can be required by any reachable exit unless facts available at the use site prove a narrower alternative.

The expected result type, assignment target type, or overload result type does not invent missing dependency contracts.

Dependency contracts are inferred from the producing expression and checked against the destination.

A declaration whose public result, stored value, callable value, trait view, task handle, thread handle, or lifecycle value carries non-local dependencies exposes those dependencies through its compiler-visible declaration contract.

This exposure is semantic metadata, not additional source syntax.

Callers must satisfy the exposed dependency contract when they use the declaration.

This is valid because the returned borrow depends on the `item` parameter:

```bray
func same(pos item: &u8) -> &u8
{
    return item;
}
```

The returned borrow remains valid only while the storage reached through `item` remains valid and while the returned borrow's capability requirements remain satisfied.

This is rejected because the returned borrow depends on local storage that ends at the function exit:

```bray
func bad() -> &u8
{
    let value: u8 = 1;
    return &value;
}
```

This type carries the dependency contract of its `data` field:

```bray
struct Cursor
{
    data: &[u8];
    index: usize;
}
```

A `Cursor` value cannot outlive the storage reached by `data`.

A lambda cannot capture a borrow from an enclosing local binding:

```bray
let writer =
{
    let output = &mut file;

    lambda (pos text: string)
    {
        output.write(text); // invalid
    }
};
```

Pass the state explicitly when a callable value needs caller-provided context:

```bray
let write_line = lambda (pos output: &mut File, pos text: string)
{
    output.write(text);
};
```

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Partial moves](partial-moves.md)
- Next: [Fact and borrow invalidation](fact-and-borrow-invalidation.md)
