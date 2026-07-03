# Declaration names and identity

A declaration name is the identifier introduced by a declaration.

For declarations with ordinary names, the declaration name follows the declaration keyword:

```bray
func parse(pos text: string) -> Result<Value, ParseError>
{
    ...
}

struct Buffer
{
    ...
}

predicate non_empty(length: usize) =
    length > 0;
```

Declaration identity is the stable semantic identity introduced by the declaration.

For module-level declarations, identity includes the package identity, logical module path, declaration name, and declaration kind.

For type members, identity includes the declaring type and member name.

For trait members, identity includes the declaring trait application surface and member name.

For implementation members, identity is tied to the implementation declaration and the member or fulfillment it defines.

For named trait implementations, the implementation name is the implementation identity.

For unnamed trait implementations, identity is the exact implementing subject and exact trait application in its coherence domain.

For overload declarations, the overload name is the shared call or implementation surface.

Overload arms keep their own declaration identities.

A declaration name cannot shadow another visible declaration or binding in the same unqualified name scope.

Qualified paths distinguish declarations through their resolved left-hand entity.

This means `some.thing` and `thing` can both be visible when the path makes the reference unambiguous.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Block-level declarations](block-level-declarations.md)
- Next: [Visibility and reachability](visibility-and-reachability.md)
