# Bray for Rust developers, part 3: Work with values and storage

Our pantry can report missing oats and recover from a panic. In this part, we'll prepare a delivery: set a packing target, inspect and update the stock, and hand the packed items to a dispatch function. Copying the paperwork is allowed. Copying the delivery itself would require considerably more oats.

This guide assumes you know Rust's ownership and borrowing rules and the Bray functions, structs, and control flow from [part 1](01-build-and-organize.md) and [part 2](02-data-and-control-flow.md). We'll start a fresh executable, so you don't need either earlier project. Use an installed Bray toolchain with support for your native target.

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

`TARGET` is a value calculated at compile time. Its declaration has no runtime storage identity. `POLICY` is an address-bearing storage instance owned by the executable product. Borrowing `&POLICY` gives us access to that storage without taking ownership.

Both initializers must be valid constant expressions. A static's storage exists at runtime, but its initializer doesn't run when a module is imported. We can't put a file read there and call it configuration (tempting though that particular hiding place may be).

Like Rust's `const fn`, a Bray `const func` can also be called with runtime arguments. The modifier makes the function eligible for constant evaluation. It doesn't force every call to happen at compile time. Bray requires const functions to be pure, deterministic, terminating, and total for valid inputs. `packing_target` only compares and returns values, so it fits.

For this application, shared access to `POLICY` is enough. Bray-owned statics cannot be moved out of, directly mutably borrowed, or declared as ordinary mutable globals. Updating state inside a static requires a type and operation that provide the appropriate interior-mutation contract. We'll leave shared mutable state for the concurrency guide.

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

The assignment moves `received` into `packed`. Using `received.name` after that assignment would be rejected because `received` no longer owns an initialized `Item`. The `packed` binding now owns the item and is responsible for its cleanup.

We haven't called `dispatch` yet. Its plain `Item` parameter will take ownership when we do. For now, we still have some packing to finish.

Notice both `let mut packed` and `mut quantity`. In Rust, a mutable binding generally allows mutation of its fields. Bray also asks the field declaration to permit mutation. Here, we can change the quantity while the item's name remains immutable. Moving the whole item doesn't require a mutable binding.

Bray's built-in `string` is copyable, so passing `packed.name` to `print_line` leaves the field usable. Don't read `string` as Rust's owned `String` and assume the same copy behavior. The containing `Item` still has no copy contract.

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

The assignment copies `before`, so both bindings remain usable. Updating `planned.quantity` doesn't change `before.quantity`.

`@copy` derives copying from the represented fields. Every field must support it, and the type cannot have incompatible ownership or lifecycle obligations. You don't implement `Copyable` yourself or supply a custom copy body. An operation that allocates resources or runs custom duplication logic needs an explicit function, such as `clone`.

Our `Item` contains copyable fields too, but we've deliberately left it without `@copy`. Passing it around should transfer the delivery, whereas passing a `Count` can duplicate the record. That's an API decision, not something the compiler should infer from how cheap a struct looks.

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

Neither function owns `packed`. The shared borrow lets `inspect` read it. The mutable borrow gives `pack` temporary exclusive access to update it. After the call, we can read the updated quantity through `packed` again.

As in Rust, a live shared borrow prevents conflicting mutation, and a live mutable borrow prevents conflicting access. Bray checks the reached storage, so changing the spelling of a path doesn't manufacture another item. Borrowing also doesn't keep an owner alive beyond its valid lifetime.

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

The returned borrow may depend on either input. Bray carries those dependencies through the result, including across function and package boundaries. The caller must keep the required storage and access valid while the result needs them.

Leaving out lifetime parameters hasn't made dangling references acceptable. Returning a borrow of an ordinary local `Item` would still be rejected. We have removed the lifetime spelling from the function signature, not the obligation to have something left to borrow.

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

The named implementation imports make list iteration and indexing available. Part 4 explains implementation selection. For now, use these imports as written.

Creating and growing a list can fail, so change the signature of `main` to `func main() -> Result<unit, std.memory.MemoryLayoutError>`. End its body with `return Ok(unit);`, and insert all remaining statements before that return. This is the error propagation pattern from part 2.

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

The index constants have type `usize`, which is the selector type accepted by this list implementation. Mutable list iteration gives us mutable borrows of the elements. Passing `item` to `pack` reborrows it for the call. The list remains owned by `delivery` after the loop.

Finally, add the dispatch loop:

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

The consuming loop takes the list and supplies owned items to `dispatch`. Neither `delivery` nor the earlier `packed` and `reserve` bindings can be used afterward. The names are still in the file, but the inventory has left the building.

## Widen the count without changing its value

In Rust, `as` can widen an integer, but it can also truncate one. For example, `300_u16 as u8` produces `44`. That would be an unfortunate way to reconcile a delivery.

Bray's plain `as` conversion is total and value-preserving. Add this before the consuming dispatch loop, while `delivery` still owns the items:

```bray
let first_count: i32 = delivery[FIRST].quantity;
let report_count: i64 = first_count as i64;

if report_count == 3
{
    std.io.print_line("Report count: three");
}
```

Run `bray run`. `Report count: three` appears before the final `oats` and `rice` lines.

Every `i32` value fits in `i64`, so the conversion preserves the count. Assigning `first_count` directly to an `i64` binding would not insert an implicit conversion. A literal can adapt to an expected type, but a value that already has a type needs the explicit conversion.

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

We now have a compile-time target, shared policy storage, independently copied records, and owned items that are borrowed during preparation and consumed at dispatch.

For an optional exercise, put the conversion and reporting block after the consuming loop. Run `bray check`. The program must be rejected because the block tries to read `delivery` after it has been consumed. Move the block back before the loop and run `bray run` again to restore the output above. A dispatch system that lets us send the same delivery twice would be optimistic, even by software standards.

## Next up

Part 4 will use traits, implementations, and generics to build reusable APIs. We'll compare those with Rust's trait model, including Bray's named implementations and explicit overload sets.

The language reference covers [constants](../../language/declarations/constant-declarations.md), [static storage](../../language/declarations/static-storage-declarations.md), [const functions](../../language/callables/const-functions.md), [copy contracts](../../language/types/copy-contracts.md), [borrowing](../../language/ownership-and-borrowing/borrow-rules.md), [inferred dependencies](../../language/ownership-and-borrowing/dependency-contracts.md), and [iteration modes](../../language/expressions/iteration-source-resolution.md).
