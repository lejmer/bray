# Internal re-exports

Re-exporting an internal declaration requires internal-use acknowledgement.

```bray
module api;

using internal impl.Buffer;

export impl.Buffer;
```

The re-export of an internal declaration is internal.

Acknowledgement does not make an internal declaration public.

Acknowledgement does not propagate through re-exports.

A public API exposes internal declarations only through an explicit public wrapper that removes the internal declaration
from the public signature.

```bray
module api;

using internal impl.Buffer;

public struct Buffer
{
    internal inner: impl.Buffer;
}
```

The public wrapper is a new public declaration with its own public contract.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Re-exports](re-exports.md)
- Next: [Summary](summary.md)
