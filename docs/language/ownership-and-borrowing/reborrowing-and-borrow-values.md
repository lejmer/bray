# Reborrowing and borrow values

A borrow value does not own the reached storage.

A borrow value has a lifetime.

A borrow is valid only while the reached storage remains valid and the borrow's capability contract remains satisfied.

A shared borrow value is copyable.

Copying a shared borrow copies the borrow value and preserves the same reached storage, lifetime, and capability
requirements.

A mutable borrow value is not copyable.

A borrow lifetime can be shortened to the last required use of the borrow.

A reborrow can be created from an existing borrow.

A reborrow has authority no greater than the borrow it comes from.

A reborrow suspends incompatible use of the original borrow for the reached storage while the reborrow is active.

Nested borrow types are allowed.

```bray
&&T
&&mut T
&mut &T
&mut &mut T
```

Each borrow layer has its own capability.

An outer shared borrow provides shared access to the next layer.

An outer mutable borrow provides mutation authority over the next layer.

The reachable operation depends on the whole access path, including every borrow layer.

A borrow value can be stored only when the containing value's type and contract carry the borrow's lifetime and
capability requirements.

Storing a borrow does not extend the lifetime of the reached storage.

A callable can return a borrow only when the callable result contract preserves the lifetime and capability dependency
on a parameter, receiver, or other input storage that can outlive the returned borrow.

A callable cannot return a borrow of local storage that ends before the returned borrow.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Borrow rules](borrow-rules.md)
- Next: [Dependency contracts](dependency-contracts.md)
