# Reborrowing and borrow values

A borrow value does not own the reached storage. It is valid only while that storage remains valid and its capability
contract remains satisfied. Its lifetime can be shortened to its last required use.

A shared borrow value is copyable. Copying it preserves the same reached storage, lifetime, and capability requirements.
A mutable borrow value is not copyable.

A reborrow is created from an existing borrow and has no greater authority than that borrow. While it is active, it
suspends use of the original borrow that would conflict under [borrow rules](borrow-rules.md) for the reached storage.

Nested borrow types are allowed.

```bray
&&T
&&mut T
&mut &T
&mut &mut T
```

Each borrow layer has its own capability. An outer shared borrow provides shared access to the next layer, while an
outer mutable borrow provides mutation authority over that layer. The reachable operation depends on the whole access
path, including every borrow layer.

A borrow value can be stored only when the containing value's type and contract carry the borrow's lifetime and
capability requirements. Storing it does not extend the lifetime of the reached storage.

A callable can return a borrow only when the callable result contract preserves the lifetime and capability dependency
on a parameter, receiver, or other input storage that can outlive the returned borrow.

A callable cannot return a borrow of local storage that ends before the returned borrow.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Borrow rules](borrow-rules.md)
- Next: [Dependency contracts](dependency-contracts.md)
