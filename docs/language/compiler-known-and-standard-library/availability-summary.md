# Availability summary

| Entity kind | Compiler can reason about it | Available without import | Uses ordinary import rules |
| --- | --- | --- | --- |
| Compiler-known scalar types | yes | yes, when target-available | no |
| `string` | yes | yes | no |
| `Range<T>` | yes | yes for target-available integer `T` | no |
| `RawPointer<T>` | yes | yes | no |
| Compiler-known type forms | yes | yes, when target-available | no |
| `Result<T, E>`, `RunResult<T>` | yes | yes | no |
| `Future<T>`, `Task<T>` | yes | yes | no |
| `PanicReport`, `ConversionError` | yes | yes | no |
| `blocking_execution()`, `compute_execution()`, `main_thread_execution()` | yes | yes | no |
| `core.memory` raw memory declarations | yes | yes, when target-available | no |
| `target` properties                                                      | yes                          | yes                                        | no                                |
| Compiler-known traits | yes | yes, when target-available | no |
| User implementations of compiler-known traits | yes | only when declared in the coherence domain | yes, for external implementations |
| Recognized standard-library functions | yes | no | yes |
| Recognized standard-library types | yes | no | yes |
| Ordinary user declarations | according to their contracts | only in their declaration scope | yes |

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Recognized standard-library operations](recognized-standard-library-operations.md)
- Next: [Conformance catalog](conformance-catalog.md)
