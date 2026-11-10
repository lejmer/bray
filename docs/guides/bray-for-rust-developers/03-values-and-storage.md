# Bray for Rust developers, part 3: Work with values and storage

The donkey has spent two guides watching us count oats. Let's give it something to carry. We'll prepare a delivery, starting with a packing target and finishing with a function that takes ownership of the packed items. Along the way, we'll borrow the stock to inspect and update it, and copy a quantity record so we can compare the plan with what we started with.

We're assuming you know your way around ownership and borrowing in Rust, and have worked through the Bray functions, structs, and control flow in [part 1](01-build-and-organize.md) and [part 2](02-data-and-control-flow.md). With a Bray toolchain installed for your native target, start a fresh executable:

```sh
bray init pantry_delivery --package pantry_delivery
cd pantry_delivery
```

## Set a packing target

In Rust, a `const` names a compile-time value, a `static` provides storage, and a `const fn` can participate in constant evaluation. Those distinctions carry over to Bray, with `const func` in place of `const fn`.

We'll use a function that ensures our target is at least three. Replace `src/main.bray` with:

```bray
module app;

using std.io;

const func packing_target(pos requested: i32) -> i32
{
    if requested < 3
    {
        return 3;
    }

    return requested;
}

const TARGET: i32 = packing_target(2);

struct Policy
{
    target: i32;
}

static POLICY: Policy =
{
    target = TARGET,
};

func main()
{
    let policy: &Policy = &POLICY;

    if policy.target == 3
    {
        std.io.print_line("Packing target: three");
    }
}
```

Run it:

```sh
bray run
```

The application prints:

```text
Packing target: three
```

`TARGET` names the value calculated at compile time and has no runtime storage identity. `POLICY` gives us an address-bearing storage instance owned by the executable product, which we can borrow with `&POLICY`. That distinction matters when choosing between them: use a constant when you need a value, and a static when you need persistent storage with an identity. Both initializers must be valid constant expressions. The static exists at runtime, but importing its module doesn't execute the initializer. If our policy eventually comes from a configuration file, we'll need an explicit runtime operation to read it.

Like a Rust `const fn`, `packing_target` can also be called with runtime arguments. `const func` makes a function eligible for constant evaluation without forcing every call to happen at compile time. Bray requires the function to be pure, deterministic, terminating, and total for valid inputs, which our comparison and return satisfy.

We'll only read `POLICY`. Bray-owned statics cannot be moved out of or directly mutably borrowed, and there is no ordinary mutable-global declaration for them. Updating their contents requires a type and operation with the appropriate interior-mutation contract, which we'll cover with concurrency.

## Hand over an item

In Rust, assigning a non-`Copy` struct to another binding transfers ownership. Bray does the same. A struct doesn't become copyable merely because its fields happen to be copyable.

Add these declarations to `src/main.bray`:

```bray
struct Item
{
    name: string;
    mut quantity: i32;
}

func dispatch(pos item: Item)
{
    std.io.print_line(item.name);
}
```

Add this at the end of `main`:

```bray
let received: Item =
{
    name = "oats",
    quantity = 2,
};

let mut packed: Item = received;

std.io.print_line(packed.name);
```

Run `bray run`. The output gains:

```text
oats
```

The assignment transfers ownership from `received` to `packed`, including responsibility for cleanup. Trying to read `received.name` afterward would be rejected because that binding no longer owns an initialized `Item`. A call to `dispatch` will make the same kind of transfer through its plain `Item` parameter. We haven't called it yet because we still need to update the quantity before handing the item over.

There are two separate permissions in `let mut packed` and `mut quantity`. In Rust, a mutable binding generally permits mutation of its fields. Bray also requires the field declaration to allow it. We can therefore update the quantity but cannot assign to the name field, even through a mutable binding. Moving the whole item doesn't require either permission.

Printing the name leaves it usable: Bray's built-in `string` is copyable, unlike Rust's owned `String`. That doesn't make the containing `Item` copyable.

## Copy the packing record

A Rust type can opt into copying with `#[derive(Clone, Copy)]`. Bray uses `@copy` on the struct or union declaration. Let's use that for a quantity record we can keep while the actual stock changes.

Add this declaration:

```bray
@copy
struct Count
{
    mut quantity: i32;
}
```

Add these statements to `main`, after printing `packed.name`:

```bray
let before: Count =
{
    quantity = packed.quantity,
};

let mut planned: Count = before;

planned.quantity = policy.target;

if before.quantity == 2 && planned.quantity == 3
{
    std.io.print_line("Original count: two, planned count: three");
}
```

Run `bray run`. The new line is:

```text
Original count: two, planned count: three
```

Both bindings remain usable because the assignment copies `before`. Changing `planned.quantity` updates the new record and leaves the old one at two. Our `Item` also contains copyable fields, but we left it without `@copy` because we want passing an item to transfer ownership of the delivery. A `Count` is only a record of the quantity. Keeping two records is useful (counting both as stock would be rather less useful), and opting into copying lets the type express that choice.

`@copy` derives the operation from the represented fields. Every field must support copying, and the type cannot have incompatible ownership or lifecycle obligations. You don't implement `Copyable` yourself or provide a custom copy body. Use an explicit function such as `clone` for duplication that allocates resources or runs custom logic.

## Borrow the item while packing

In Rust, a function taking `&Item` observes an item, while one taking `&mut Item` can update it. Bray uses the same borrow forms. Add these functions:

```bray
func inspect(pos item: &Item)
{
    std.io.print_line(item.name);
}

func pack(pos item: &mut Item, target: i32)
{
    if item.quantity < target
    {
        item.quantity = target;
    }
}
```

Add this at the end of `main`:

```bray
inspect(&packed);
pack(&mut packed, target = planned.quantity);

if packed.quantity == 3
{
    std.io.print_line("Packed three");
}
```

Run `bray run`. The new lines are:

```text
oats
Packed three
```

`packed` still owns the item throughout both calls. `inspect` receives shared access to read it, then `pack` receives temporary exclusive access to change its quantity. Once `pack` returns, we can read the updated quantity through `packed` again. The familiar Rust restrictions apply here: a live shared borrow prevents conflicting mutation, and a live mutable borrow prevents conflicting access. Bray checks which storage a path reaches, so another path to the same item remains subject to those restrictions.

A borrow also cannot keep its owner alive. Let's see what that means when a function returns one.

## Return a borrow without lifetime parameters

Rust often infers lifetimes, but a function selecting between two borrowed inputs needs to express how the result relates to them:

```rust
fn smaller<'a>(left: &'a Item, right: &'a Item) -> &'a Item {
    if left.quantity <= right.quantity {
        left
    } else {
        right
    }
}
```

Bray infers dependency contracts instead of asking you to write source-level lifetime annotations. Add this function:

```bray
func smaller(pos left: &Item, pos right: &Item) -> &Item
{
    if left.quantity <= right.quantity
    {
        return left;
    }

    return right;
}
```

Now add this at the end of `main`:

```bray
let reserve: Item =
{
    name = "rice",
    quantity = 1,
};

let selected: &Item = smaller(&packed, &reserve);

std.io.print_line(selected.name);
```

Run `bray run`. The application adds:

```text
rice
```

Here, `selected` refers to the rice because its quantity is smaller. The function's result may depend on either input, though, and Bray carries those dependencies through the result even across function and package boundaries. The caller must preserve the required storage and access while the result needs them. This is why we can return a borrow supplied by the caller but couldn't return a borrow of an ordinary local `Item` that dies when the function returns. Lifetime parameters are absent from the signature, but the lifetime constraints still apply.

## Inspect, update, and consume a delivery list

Rust usually expresses the three iteration modes through `&items`, `&mut items`, and an owned `items`. Bray puts the choice directly after `in`: ordinary iteration borrows the source, `in mut` borrows it mutably, and `in move` consumes it.

We'll use `std.collection.List<Item>` to own both items. Add these imports after `using std.io`:

```bray
using std.collection;
using std.memory;
using std.collection.ListReadIterable;
using std.collection.ReadCursorIterator;
using std.collection.ListWriteIterable;
using std.collection.WriteCursorIterator;
using std.collection.ListMoveIterable;
using std.collection.MoveCursorIterator;
using std.collection.ListElementIndex;
```

These named implementations provide the list's iteration and indexing operations. We'll explain how they are selected in part 4.

Creating and growing a list can fail. To propagate those errors as we did in part 2, change the signature of `main` to `func main() -> Result<unit, std.memory.MemoryLayoutError>`. Add `return Ok(unit);` at the end of its body. Insert all remaining statements before that return.

Add the list after the last use of `selected`:

```bray
let mut delivery: std.collection.List<Item> = try std.collection.List<Item>(capacity = 2);

try std.collection.push<Item>(&mut delivery, packed);
try std.collection.push<Item>(&mut delivery, reserve);

for item in delivery
{
    inspect(item);
}
```

Run `bray run`. The new lines are:

```text
oats
rice
```

The two `push` calls move the items into the list. The observing loop borrows that list and gives `inspect` shared access to each item. The loop doesn't consume `delivery`, even though there is no `&` in `for item in delivery`. That's an important difference from Rust's `for item in delivery` over an owned collection.

Now pack every item to the target. Add this after the loop:

```bray
for item in mut delivery
{
    pack(item, target = policy.target);
}

const FIRST: usize = 0;
const SECOND: usize = 1;

if delivery[FIRST].quantity == 3 && delivery[SECOND].quantity == 3
{
    std.io.print_line("Both items packed");
}
```

Run `bray run`. The application adds:

```text
Both items packed
```

Each iteration supplies a mutable borrow of an element, which we reborrow when passing it to `pack`. The list remains owned by `delivery` after the loop. To inspect the resulting quantities, we use index constants of type `usize`, the selector type accepted by this list implementation.

The donkey can finally leave with the delivery. Add the consuming loop to hand each item to `dispatch`:

```bray
for item in move delivery
{
    dispatch(item);
}
```

Run `bray run`. The application adds:

```text
oats
rice
```

This loop consumes `delivery` and gives each owned item to `dispatch`. We can no longer use the list, just as we can no longer use `packed` and `reserve` after moving them into it.

## Widen the count without changing its value

Before dispatch, we need a count for a report that uses `i64`. Rust's `as` can widen an integer for that purpose, but the same operator can also truncate one: `300_u16 as u8` produces `44`. Bray restricts plain `as` to total, value-preserving conversions.

Add this before the consuming dispatch loop, while `delivery` still owns the items:

```bray
let first_count: i32 = delivery[FIRST].quantity;
let report_count: i64 = first_count as i64;

if report_count == 3
{
    std.io.print_line("Report count: three");
}
```

Run `bray run`. `Report count: three` appears before the final `oats` and `rice` lines.

Every `i32` value fits in `i64`, so this conversion preserves the count. We still have to write it explicitly. Assigning `first_count` directly to an `i64` binding doesn't insert a conversion, even though a literal could adapt to that expected type. Once the value has a type, widening it requires `as`.

For a conversion that can fail, use `std.convert<Target>`, which returns a `Result`. For deliberate rounding, saturation, or wrapping, use the corresponding named operation in `std.numeric`. Plain `as` doesn't silently choose one of those policies. See the [conversion rules](../../language/expressions/conversion-expressions.md) for the available forms.

## Check the finished delivery

Run the completed application:

```sh
bray run
```

Apart from build progress, the complete output is:

```text
Packing target: three
oats
Original count: two, planned count: three
oats
Packed three
rice
oats
rice
Both items packed
Report count: three
oats
rice
```

For an optional exercise, move the conversion and reporting block after the consuming loop, then run `bray check`. The program must be rejected because it tries to read `delivery` after the loop has consumed it.

Move the block back before the loop and run `bray run` to restore the output above.

## Next up

Part 4 will use traits, implementations, and generics to build reusable APIs. We'll compare those with Rust's trait model, including Bray's named implementations and explicit overload sets.

The language reference covers [constants](../../language/declarations/constant-declarations.md), [static storage](../../language/declarations/static-storage-declarations.md), [const functions](../../language/callables/const-functions.md), [copy contracts](../../language/types/copy-contracts.md), [borrowing](../../language/ownership-and-borrowing/borrow-rules.md), [inferred dependencies](../../language/ownership-and-borrowing/dependency-contracts.md), and [iteration modes](../../language/expressions/iteration-source-resolution.md).
