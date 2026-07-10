# Declaration names, lookup, and identity

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

## Ordinary lookup namespace

Bray has one general identifier lookup namespace: the **ordinary lookup namespace**.

A lookup namespace determines whether the same spelling can identify more than one entity in the same lookup scope. A declaration's
kind does not create a separate namespace. Types, values, traits, predicates, callable contracts, named implementations, members,
generic parameters, callable parameters, local bindings, pattern bindings, and other named declarations all introduce ordinary
names.

Consequently:

- a type and a constant cannot share a name in one module,
- a generic type parameter and a generic const parameter cannot share a name on one declaration,
- a field and a callable member cannot share a name on one type,
- different trait member categories cannot share a name in one trait,
- a local binding cannot shadow a visible declaration, parameter, or binding.

An explicit overload family introduces its ordinary name once. The overload arms keep separate declaration identities but do not
introduce the family name again.

A context does not create a new namespace. Type lookup, value lookup, trait lookup, predicate lookup, and other context-specific
operations first resolve an ordinary name and then require the resolved entity to have a valid semantic category. If the name resolves
to an entity of the wrong category, that is different from the name not being found.

Packages and modules use specialized path indexes and act as lookup providers, but they do not create separate name-collision
partitions. Visible package identities, module path components, and declarations that can occupy the same path position participate
in the same ordinary name surface.

The following are selected through dedicated semantic relationships rather than ordinary name lookup:

- unnamed implementations, which are selected by coherence key,
- overload arms, which are selected through their overload family,
- implementation fulfillments, which correspond to trait member identities,
- lifecycle members selected through their declaration form,
- tuple elements selected by ordinal,
- contextual `Self`, `self`, and `result` bindings.

Directives and `using` declarations do not introduce ordinary names.

An export does not create a declaration identity or introduce an unqualified name inside the exporting module. It exposes the target
declaration's ordinary name through the module's exported lookup surface, where that name must not conflict with another declaration
or export.

An additional lookup namespace exists only if this specification explicitly permits a new name category to reuse an ordinary name
in the same scope. Bray currently defines no such category.

## Declaration identity

Declaration identity is the stable semantic identity introduced by the declaration.

For module-level declarations, identity includes the package identity, logical module path, declaration name, and declaration kind.

For type members, identity includes the declaring type and member name.

For trait members, identity includes the declaring trait application surface and member name.

For implementation members, identity is tied to the implementation declaration and the member or fulfillment it defines.

For named trait implementations, the implementation name is the implementation identity.

For unnamed trait implementations, identity is the exact implementing subject and exact trait application in its coherence domain.

For overload declarations, the overload name is the shared call or implementation surface and occupies one ordinary name.

Overload arms keep their own declaration identities.

A declaration name cannot shadow another visible ordinary name in the same unqualified name scope.

Qualified paths distinguish declarations through their resolved left-hand entity.

This means `some.thing` and `thing` can both be visible when the path makes the reference unambiguous.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Block-level declarations](block-level-declarations.md)
- Next: [Visibility and reachability](visibility-and-reachability.md)
