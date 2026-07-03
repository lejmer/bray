# Expression ownership

Expressions interact with ownership.

An expression can:

- create a value,
- move a value,
- copy a value,
- borrow a value,
- mutably borrow a value,
- consume a value,
- partially move a value,
- initialize storage,
- reinitialize storage,
- destroy storage.

Construction expressions create fully initialized values when all required parts are initialized.

Assignment expressions reinitialize storage when the destination and type contract permit it.

Consuming expressions make the consumed value unavailable through its old access path unless it is reinitialized.

Partial moves leave the subject partially initialized.

Destruction of partially initialized values destroys only initialized parts.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression effects and capabilities](expression-effects-and-capabilities.md)
- Next: [Expression initialization behavior](expression-initialization-behavior.md)
