# genie's coding style

This guide sets the rules for the code in this repository. It applies to every crate. A rule
that applies only to the engine crates says so.

It rests on six sources:

- The **Rust API Guidelines**. Their ids (`C-CASE`, `C-NEWTYPE`, …) follow each rule they set.
- **Rust Design Patterns**: its idioms, patterns and anti-patterns.
- **Data-oriented design**: Richard Fabian's book *Data-Oriented Design* and Mike Acton's talk
  "Data-Oriented Design and C++" (CppCon 2014).
- The **game-engine patterns** of Bevy's ECS and of Robert Nystrom's *Game Programming
  Patterns*.
- **Hexagonal architecture** (ports and adapters) for the systems that meet the outside world.
- The **sans-io** pattern for the crates that do I/O (sans-io.readthedocs.io). Rust examples
  are `quinn-proto` beside `quinn`, and `rustls`.

Three things rank above any guideline, in this order:

1. **The game's behaviour.** genie-core reproduces the game bit for bit: its order of
   operations, its random draws, its float arithmetic. No rule here changes an update's result.
2. **This guide.** Where it departs from a general guideline, it says why.
3. **The general guidelines.**

The gates (section 16) check what a machine can check. A review checks the rest against this
guide.

## 1. Principles

- **Faithful before small.** Choose the design that the game's behaviour needs, however many
  call sites it changes. A smaller change that only approximates the game is wrong.
- **Composition over inheritance.** An object is the layers and parts that it holds, not a class
  hierarchy.
- **Data-oriented design.** State is plain data in columns. A system is a function that takes
  exactly the data that it reads and writes (section 8).
- **Hexagonal architecture.** The renderer, audio, input, network and storage are adapters behind
  ports of the core. The simulation core stays pure (section 10).

Apply each principle where it applies, not by rote. If a design choice is open, make it as a
data-oriented architect makes it.

## 2. Crates and dependencies

| Folder | Holds | Licence |
| --- | --- | --- |
| `crates/base/` | `genie-error`, `genie-diff`: below every other crate | GPL-3.0 |
| `crates/formats/` | the readers and writers of the game's files | GPL-3.0 |
| `crates/engine/` | the engine: `genie-core`, `genie-command-codec` | AGPL-3.0 |
| `crates/tools/` | command-line tools and the oracle library | AGPL-3.0 |

- Dependencies point inward. Engine and tool crates can depend on format crates. A format crate
  never depends on an engine or tool crate: it stays usable without the engine, and CI fails on
  such a dependency. An adapter depends on the core, never the reverse.
- Every crate's `Cargo.toml` carries `[lints] workspace = true`.
- Add a new engine crate to the wasm32 build line of `.github/workflows/ci.yml`.
- Split a crate only at a stable seam: an adapter, or a foundation crate without dependencies
  back into the engine.
- Add no module without a caller that exists now.

## 3. Naming

- Follow RFC 430's casing to the letter (C-CASE), in every crate, the tests and the generated
  code too:
  - `UpperCamelCase` for types, traits and enum variants; `snake_case` for crates, modules,
    functions, methods, fields and locals; `SCREAMING_SNAKE_CASE` for constants and statics;
    a short `UpperCamelCase` type parameter (`T`, `K`) and a short lowercase lifetime (`'a`).
  - An enum's values are its variants, in `UpperCamelCase`: `BuildState::Move { move_to:
    None }`, never an alias constant on the enum (`BuildState::MOVE`). A newtype over a value
    that the game holds as any integer (`GameMode`, `NotifyKind`) names its known values as
    associated constants, and those are `SCREAMING_SNAKE_CASE`.
  - An acronym or a contraction is one word: `ScxVersion`, `AiModule`, `UnitTypeId`,
    `DrsReader`, `AocToWk`, not `SCXVersion`, `AIModule`, `UnitTypeID`, `AoCToWK`.
  - In `snake_case` and `SCREAMING_SNAKE_CASE`, a single letter is a word only at the end:
    `btree_map`, `PI_2`, `kernel_sin`, `target_master`, not `b_tree_map`, `k_sin`,
    `t_master`. This holds for test names too: no article `a` and no possessive `s`
    (`build_move_to_takes_its_task_moving_sprite`, not
    `a_build_s_move_to_takes_its_task_s_moving_sprite`). A digit is no letter
    (`state_6_goes_on`).
  - A generator converts the game's names by these rules: `tPos` becomes `tpos` (a lone
    letter joins the next word), `mWaypoints` and `kDefaultWaitTime` become `waypoints` and
    `default_wait_time` (no member or constant prefix), `(int32_t*)mRules` becomes `rules` (no
    cast), `TribeInformationAIModule` becomes `TribeInformationAiModule`.
- Name a file, a folder and an item in plain English for what it does or holds:
  `pathing/short_path.rs`, not `mshortpather.rs` or `p2.rs`. Two names a letter apart
  (`reach.rs` beside `reached.rs`) are not allowed.
- Put no game address, Ghidra label or class abbreviation into a path or an identifier. Write
  `image::sprite::EPSILON`, not `DAT_142eb0c60`. Addresses go into the documentation as
  provenance (section 13). The addresses of one build are data of that build, not code.
- Give a type the name that says what it is where it is used: `HuntAction`, not `hunt::Action`.
  This is why the workspace allows `clippy::module_name_repetitions`.
- Use `as_` for a free borrowed view, `to_` for a conversion that costs, `into_` for one that
  consumes (C-CONV).
- Name a getter `foo()`, not `get_foo()`. `get` is only for a lookup by key (C-GETTER).
- Name constructors `new`, `with_…` and `from_…` (C-CTOR).
- Give a collection `iter`, `iter_mut` and `into_iter`, and name an iterator type after its
  method (C-ITER, C-ITER-TY).
- Keep one word order in a crate: `ParseError` and `ReadError`, not `ErrorParse` (C-WORD-ORDER).
- Name a Cargo feature after the thing: `serde`, not `with-serde` (C-FEATURE).
- In a ported formula, short names that match the decompile (`dx`, `x0`, `y1`) are correct. The
  reader compares them with the routine.

## 4. Modules and files

- Import from the crate root: `use crate::movement::move_to::MoveWait;`. Do not write
  `use super::…`. Inline paths and doc links follow the same rule. A test module's
  `use super::*;` is the one exception.
- Give each engine system a folder with a `README.md`. Write its prose by hand.
  `scripts/readmes.py SRC` fills the generated tables. If you add a file or a folder, run it.
  If you change the first sentence of a `//!` doc, run it too.
- Give an action a folder of its own in its system (`gathering/hunt/`). `mod.rs` holds its state
  and constants. The other files follow its slots and states: its start, its update, its
  `set_state`. `unit.rs` binds the action to the world. `unit_ai_side.rs` holds what the unit AI
  asks of it. `tests.rs` holds its tests.
- Keep a hand-written `.rs` file at 700 production lines or less. Test code does not count. A
  file on the debt list (`.config/file-size-debt.json`) must not grow.
- Define each constant once, in the module of its meaning. A closed set of values is an enum.
  A value defined in two files is a defect.
- Do not edit a generated file. Change its generator. A generated file starts with
  `// Generated by` and carries its generator's file-level allow list.

## 5. Types

### Newtypes (C-NEWTYPE)

A parameter or a field accepts exactly the values that are meant for it. So give every value
with a meaning its own type, not a raw primitive:

- **Ids and indices:** an object (`target: i32` becomes `ObjectId`), a player (`bp: usize`
  becomes `PlayerId`), a master, a master class, a task.
- **Quantities with a unit:** a duration in seconds (`reload: f32`), a distance or a range, a
  speed, an angle.
- **Places and extents:** a point on the map (`at: [f32; 3]` becomes `Vec3`), a footprint's
  radii (`radius: [f32; 2]`), a tile, a fixed-point position.

Then a swapped argument does not compile, and a reader traces a value through the code by its
type. A raw number stays only inside a type's own arithmetic and at the boundaries: the save
reader, the `.dat` reader, the recording's commands and the oracle's import.

If two parameters hold the same kind of value in different roles, one type is correct for
both. `me: &Master` and `target: &Master` are both masters, and either one is a valid master
in either place. The names keep these roles apart. A reload time and a range only share the
primitive `f32`: they are two kinds of value and need two types.

- Keep the value private: a tuple struct whose field is not `pub`.
- Set the value only through constructors that check it: `TryFrom`, or `of_field` for a raw
  field whose −1 means none.
- Read the value through a getter that `getset` generates. Give the type no setter.
- Let the type hold only what it names. No `ObjectId` is −1. No `MasterId` is negative.
- Derive what the value allows: `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `PartialOrd`,
  `Ord`, `Hash` (C-COMMON-TRAITS). Give a mask type `Binary` and `LowerHex` (C-NUM-FMT).
- Make a set of bits a `bitflags` type (C-BITFLAG). Name the bits that have a meaning, and
  declare `const _ = !0;` so that no operation drops a bit. Build a word from the save with
  `from_bits_retain`, never `from_bits_truncate`. Give a positional bit (a player's) a
  constructor, and a mask of several bits a plain associated constant
  (`vision::VisionBits`).
- Give a quantity type the operations that its meaning has, and no others: a duration plus a
  duration is a duration, a point minus a point is an offset.
- Keep the game's arithmetic in a quantity type. Each operator does the one `f32` operation
  that the game does, in the game's order (C-OVERLOAD). A newtype changes no bit.

```rust
/// An object's id (`+0x8`). Every field that names an object takes −1 for none, so no id
/// is −1: such a field holds `Option<ObjectId>` ([`ObjectId::of_field`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CopyGetters)]
pub struct ObjectId(
    /// Its value, as the object and the fields that name it hold it.
    #[getset(get_copy = "pub")]
    i32,
);
```

### Parse, don't validate

A validation checks a raw value and passes the same raw value on. Every later reader must then
trust the check, or do it again, and nothing stops a caller that skipped it. A parse checks the
raw value once and returns a type that can hold only valid values. Later code takes that type,
so it cannot receive an unchecked value, and the compiler holds the check in place. (Alexis
King, "Parse, don't validate", 2019.)

- Parse as early as possible: at the system boundary, where the value first enters genie. The
  boundaries are the save reader, the `.dat` reader, the command codec and the oracle's import.
  The parse returns a `Result`. An invalid value is an error there, with its context
  (section 7).
- Do not keep a raw value to convert it later, where a function needs the type. A struct field
  that a reader fills holds the parsed type from the start.
- Keep the value in its type from the boundary on, through every field and every call. Pass
  the parsed type, never the raw value with a check beside it.
- Convert back to a raw number only at an outgoing boundary (a writer, a format that the game
  reads) or inside the type's own arithmetic. Two genie functions never pass a value as a raw
  number and parse it again.
- Migrate a type in one sweep, from the boundary inward: first the reader or the codec that
  makes the value, then every field and every function down the call chain. Leave no raw
  value behind for a later step to convert.
- `ObjectId::field` gives the game's integer back, −1 for none. Call it only at an outgoing
  boundary. These are the codec's encoder, a notify's parameters, a report line in the game's
  terms, and a sort key or sum of the game's integers.
- Write no `is_valid` function that returns a `bool` for later code to trust. Write a
  constructor that returns the type or an error (`TryFrom`, `of_field`).
- Put the check into the shape of the type. A required value is `T`, not `Option<T>`. A closed
  set is an enum. A list that is never empty holds its first element on its own.
- Check a value once. A function that receives the parsed type does not check it again.

```rust
// Validate: the raw id travels on, and every reader must remember the check.
fn is_object(id: i32) -> bool {
    id != -1
}

// Parse: the check makes the type, and the type travels on. `attack` cannot receive −1.
fn attack(target: ObjectId) {}

fn command(raw: i32) -> Result<(), GenieError> {
    let target = ObjectId::try_from(raw)?;
    attack(target);
    Ok(())
}
```

### `Option`, enums and flags

- Use `Option<T>` only where the function works without the value: a target or a point, an
  entry of a unit list. If the function needs the value, take `T`. A missing value becomes an
  error at the boundary that reads it.
- Find the cutoff: the first function that cannot do its work without the value. That
  function takes `T`. Its caller unwraps the `Option` once and decides what none does: skip
  the entry, return, or fail.
- Do not pass an `Option<T>` down a call chain whose functions all need the value. A `None`
  that arrives deep in the chain hides where the value went missing.
- Fail early. If a later step needs a value that is none, return the error where the value
  is read. A raw field that must name an object converts with `ObjectId::try_from(raw)?`,
  not with `of_field`.
- Keep `Option<T>` in a list only where the game's null entries count: their positions, the
  list's length, a sort. If a routine acts only on the entries that exist, flatten the list
  at its start (`ids.iter().flatten()`).
- Some game code turns none into −1 and uses the number: a sort key, a saved field, a
  comparison, a table index. Write that conversion once, where the game does it, and name
  the game's routine in its comment. Check each such site against the game's code: a lookup
  of −1 finds no object, because no object's id is −1 (the world's reusable ids start at
  −2).
- A lookup by id (`World::object`, `World::place`, `World::world_object`) takes
  `impl ObjectKey`: an id, an `Option<ObjectId>`, a `RecordId` or a `GroupTarget`. A key that
  names none finds nothing, as the game's table does.
- A recyclable master's object (a missile, a ghost) has a negative id. Test it with
  `ObjectId::is_recyclable`, not with `id < 0`.
- If the game holds two meanings in one integer, make an enum whose variants hold their data:
  `Issuer` (a player or an object), `GroupTarget` (an object or another kind of target),
  `RecordId` (an object, a tile or a map edge). Where the game compares or sorts the integer,
  the enum does too: its `PartialEq` and `Ord` go by the game's value. Its parse from the save
  and its value for a writer are the only places that see the integer.
- A list of obstruction records holds `RecordId`, a list of objects holds `ObjectId`. Convert
  an object list once, where it enters a record query (`RecordId::from`). A function whose
  callers all pass objects takes `&[ObjectId]` and converts inside.
- Make a closed set of values an enum and match it whole.
- Give a public enum of a format crate `#[non_exhaustive]` or an `Unknown(…)` variant. A newer
  game can write a value that the enum does not know yet.
- Make a two-way choice of genie's own an enum, not a `bool` parameter (C-CUSTOM-TYPE). A flag
  that the game itself holds stays a `bool` field, with its offset in the doc.
- Prefer existence to a flag. A state shows by the list or table that holds the object, not by
  a `bool` that every system tests.

### Traits on types

- Implement `Debug` on every public type. A hand-written `Debug` is never empty (C-DEBUG,
  C-DEBUG-NONEMPTY).
- Implement `Default` where a natural empty value exists (C-COMMON-TRAITS).
- Convert with `From`, `TryFrom` and `AsRef`, on the more specific type (C-CONV-TRAITS,
  C-CONV-SPECIFIC).
- Keep `World`, the readers and `GenieError` `Send` and `Sync`. The world holds no interior
  mutability (C-SEND-SYNC).
- Implement `Deref` only for a smart pointer. `Deref` never imitates inheritance (C-DEREF).

### Parameter structs and builders (C-BUILDER)

- Give a function at most seven arguments. If it needs more, or if it takes a tuple parameter
  such as `(p, i): (usize, usize)`, give it a plain parameter struct named for the call.
- Use `bon`'s builder only on a struct with many optional fields. Put no `#[builder]` on a
  function.
- Do not add `allow(clippy::too_many_arguments)`.

## 6. Functions

- Take borrowed types: `&str`, `&[T]`, `&T`, not `&String` or `&Vec<T>`.
- Take ownership only of a value that the function keeps (C-CALLER-CONTROL).
- Return values, not out-parameters (C-NO-OUT).
- Make a function whose first parameter is its main object a method of that object (C-METHOD).
- Return the intermediate results that a caller needs (C-INTERMEDIATE).
- Keep a value mutable only while you build it: `let v = { let mut v = …; v.sort(); v };`.
- Move a value out of a changed enum with `mem::take` or `mem::replace`, not with a clone.
- Do not clone to satisfy the borrow checker. Split the borrow, or pass the id. A clone of a
  large value (a master, a unit list) in production code needs a written reason.
- Keep each function under the CRAP gate's threshold of 30. If a function's complexity is more
  than 30, split it into named functions. A closure does not hide a branch: the gate counts it
  nowhere, so a closure never takes a branch out of a function to pass the gate.

## 7. Errors

Every crate uses one error type, `genie_error::GenieError`. It holds a kind, a reason, the
raising `file:line` (`#[track_caller]`) and key-value context.

- Make the error with its kind: `invalid_input`, `missing_data`, `io`, `unreachable` or
  `unsupported`.
- Write the reason as a static, lowercase phrase without a full stop. It names what is wrong.
  The census groups the stops by it, and a reader finds the site by a search for it. If two
  sites share a reason, define it once as a `const`.
- Attach what the site knows: `.with("object", id)`.
- Propagate the error with `?` where it happens. No function takes a `Result` as a parameter.
  No accumulator carries an error past later effects. If a caller must branch on a result, it
  matches the result. Then it passes the `Ok` value or the error on.
- Never panic on what a recording, a save or a `.dat` can cause. On those paths, write no
  `unwrap`, `expect`, `unreachable!` or panicking index. For a state that the code holds
  impossible, return `GenieError::unreachable("…")`. Panics belong in tests only.
- Do not add a `GenieError::unsupported`. Port every branch of a routine, the branches that no
  recording reaches included, and test them with unit tests. The existing sites are the gaps
  that are left. Their number only decreases.
- Document the errors of every public function that returns a `Result` in `# Errors`
  (C-FAILURE).
- An error type implements `std::error::Error`, `Send`, `Sync` and `'static` (C-GOOD-ERR).

### Indexing

`v[i]` checks the bound, and an `i` out of range panics: in production code an index is a
hidden `unwrap`. It also hides what the code means. `t.players[1].objects[0]` names a slot in
two nested lists, and that slot holds another object after the next swap-remove.

- Reach an object by its id through the world's lookup (`World::object`), not by its position
  in nested lists. A position stays valid only until the list changes. An id stays valid for
  the object's life.
- Walk a collection with an iterator (`iter`, `iter_mut`, `zip`, `enumerate`, `chunks_exact`,
  `windows`), not with an index from `0..len`. An iterator needs no bound check per element
  and says what the loop does.
- If an index comes from input (a recording, a save, the `.dat`), read with `get(i)` and turn
  `None` into an error with the index as context (section 7). Better, parse the index into a
  type at the boundary that holds only valid values (section 5).
- In a hot loop, take the slice once before the loop and iterate it (`cells.get(start..end)`,
  with an error for a range that the cells do not hold), or zip the columns that the loop
  reads. Do not index several columns per element.
- Do not use `get_unchecked`: the workspace forbids `unsafe`. Measure before you rewrite a
  loop for speed. A bound check that never fails costs little. The panic is the defect.
- Tests can index. There a panic is a failing test, and a short index keeps the test readable.

## 8. Data-oriented design

- Hold state as plain data in columns: a field per object, a table per kind, an index by id. A
  field that a system reaches only through a chain of accessors is a defect.
- Give a system exactly the data that it reads and writes. A `&mut World` or `&mut Unit`
  parameter whose callee uses two fields is a defect. Order-sensitive code that calls other
  objects is the exception: it takes the world (section 9).
- Compute a derived value where it is read, or keep it as a column with one writer: the
  transform that changes its inputs. A second writer, or a writer that does not read the
  inputs, is a defect.
- Do not copy state to sync it afterwards. A mirror of a value that the game reads live is
  wrong unless every write keeps it current by construction.
- Put no getter, setter or wrapper between a system and its data without a reason. Two
  reasons count: a checked value (a newtype) and an invariant (a setter that keeps an index
  current).
- Split hot and cold data, and make dense tables by id, only after a profile. A table iterates
  in id order.
- Make no layout change that alters the order of the updates or of the random draws.
- Read game data from the game's files: masters, graphics, tasks and costs come from the save or
  the `.dat`, never from Rust constants. The only constants in code are the values that the
  game's own code compares. They come from the generated `math/image.rs`, each with its routine
  in the doc.

## 9. Game-engine patterns

genie takes Bevy's ideas, not its crate: `bevy_ecs` iterates by archetype, and its parallel
scheduler breaks a bit-for-bit replay. No pattern changes the game's order of updates.

- **Ids, not references.** An object reaches another by its `ObjectId` through the world. No
  long-lived struct holds a reference to an object.
- **Calls at once.** A call on another object runs at once, as the game's call does. Borrow each
  object briefly, never across a call. Only the part that runs leaves its object while it runs,
  as Bevy's `resource_scope` takes out a resource. So the world still finds the running object
  by its id.
- **Queues only where the game queues.** The unit AI's notify queue, the removal lists and the
  command queue are the queues. Add no general effect or event queue. Defer only a structural
  change. `docs/EFFECTS.md` lists every deferred channel.
- **A declared schedule.** The tick is a table of named steps in `world/schedule.rs`. Add a new
  step there.
- **Exclusive world access.** No parallel scheduler and no thread runs inside an update.
- **Commands as values.** A command is a value with a codec (`genie-command-codec`). Its run
  is one `match`.
- **Components.** An object holds its layers and parts. A slot override is a `match`, never a
  trait hierarchy that imitates the game's classes.
- **A fixed step.** The world steps by the recording's or the caller's dt. The engine reads no
  clock.
- **Type objects.** Unit types are data (the masters), not Rust types.
- **A spatial partition.** A new spatial query uses the tile lists and the obstruction grid, not
  a scan of every object.
- **A dirty flag only where the game has one.** Elsewhere a derived value has one writer
  (section 8).
- **Ids as the game hands them out.** Reuse no id that the game does not reuse.
- **No global state.** Engine crates hold no `static mut`, `thread_local!`, `OnceLock`,
  `LazyLock` or `lazy_static`. A shared table is a field of the world. Many parallel instances
  and WebAssembly need this.
- **Observers outside the update.** A frontend reads the state and the events after an update.
  No callback runs inside it.

## 10. Hexagonal architecture

- Do no I/O in the core: no `std::fs`, `std::net`, `std::env`, `Instant` or `SystemTime` in
  genie-core.
- Put each outside system into an adapter crate, behind a port that the core defines: a trait,
  or the data that the core exposes.
- Shape a port by the core's need. No type of an adapter's library is in its signature.
- Give a trait a real second implementation (a test double counts), and keep it small. A trait
  with many methods and one production implementation is a defect: pass the data instead.
- If a trait can serve as `dyn`, keep it object-safe (C-OBJECT). Seal a trait that others must
  not implement (C-SEALED).

## 11. Sans-io in the leaf crates

The core does no I/O (section 10). The leaf crates do: the format crates read and write the
game's files, and the later network, storage and RelicLink crates talk to servers. Write their
logic sans-io: the logic does no I/O itself. Then a blocking caller, an async runtime and
WebAssembly use the same code without assumptions. This section does not apply to genie-core.

- Keep the format or protocol logic free of I/O. It takes bytes, events and the time in, and it
  gives bytes, events and values out. It never opens a file, a socket or a timer, and it never
  blocks, sleeps or awaits.
- Model a protocol as a state machine that the caller drives: it takes the bytes that arrived
  and the current time, and it gives the bytes to send, the events and the time of its next
  timeout. Name the methods for the protocol, not for a runtime.
- Pass the time in. The logic reads no clock, so a test or a replay can give any time.
- Parse from a buffer that the caller filled (`&[u8]`). If the bytes do not hold a whole
  record yet, return how many more the parser needs instead of reading them. Write by returning
  the bytes, or into a buffer that the caller owns.
- Put the I/O into thin drivers beside the logic. A blocking driver works over `std::io::Read`
  and `Write`. An async driver sits behind a Cargo feature named after its runtime (`tokio`,
  not `with-tokio`, C-FEATURE). A driver holds no format or protocol logic: it moves bytes and
  time between the I/O and the state machine.
- Keep the logic free of an async runtime: no `async fn`, no runtime type and no runtime
  dependency outside the drivers.
- Test the logic with byte slices and a time that the test gives. A unit test opens no file and
  no socket.
- A reader over the whole file in memory (`&[u8]`) is sans-io. A reader over `R: Read` is a
  blocking driver. When you change such a reader, give it a sans-io core.

## 12. Determinism

genie gives the same bits as the game on every host, WebAssembly included.

- Keep the game's numeric type field by field: `f32` or `f64`, the integer's width and its
  signedness (the save's layout in genie-save names them).
- Keep the game's order of operations. Round each sum and each product on its own: no fused
  multiply-add, and no rewrite of `a * b / c` as `a * (b / c)`. Compare floats exactly where
  the game does. genie-core turns off clippy's float lints for this reason.
- Write `wrapping_add` and its kin where the game's integer arithmetic wraps. A plain `+` is
  only for values that cannot overflow: an overflow panics in a debug build (CI's tests) and
  wraps in a release build (the census), and the two runs then differ.
- Divide and take remainders with `/` and `%`: they truncate toward zero, as the game's C++
  does. `div_euclid` and `rem_euclid` do not.
- Convert a float to an integer with `math::conversions` (`cvttss2si`), as the x86 instruction
  does. Rust's `as` saturates, the instruction does not. An `as` cast that stands for `movsx`
  or `movzx` is correct.
- Use the ported math functions of `math/fmath/`. The standard library's float functions can
  differ between hosts.
- Draw from the same random stream as the game, in the same order (`math/rng.rs`).
- Keep the game's list order. A removal swaps the last element into the free place, as the
  game's lists do, so a generic "order-preserving removal" is wrong here. A sort uses MSVC's
  introsort (`math/msvc.rs`), which is not stable: its ties fall as the game's do. Never iterate
  a `HashMap` or a `HashSet`: clippy's `iter_over_hash_type` fails the build. A hash map is for
  lookups. A map that is walked is a `BTreeMap` or a list.
- Run parallel work outside a world: many worlds at once (the census's recordings), never
  threads inside an update.
- Check each change to the simulation against the recordings' checkpoints. No recording that
  holds can regress.

## 13. Documentation

- Document every item (`missing_docs`). A crate root opens with a thorough `//!`. A published
  crate's root has an example (C-CRATE-DOC).
- Start a port's doc with the game's routine: its class and method where the debug symbols give
  one (`RGE_Action_Attack::work`), else its address in build 158041 (`0x140a82ae0`). Name its
  virtual slot and the fields that it reads and writes, with their offsets (`+0x18`). A reader
  finds a behaviour by these names.
- Describe what the code does in the game's terms. Use plain English and short sentences. Spell
  as the rest of the repository does (British: "behaviour", "licence").
- Link the items that the prose names: [`MoveWait`] (C-LINK). The docs build without a broken
  link.
- Write `# Errors` and `# Panics` sections (C-FAILURE). A doc example uses `?`, not `unwrap`,
  and hides its set-up behind `#` (C-QUESTION-MARK).
- Write a comment for a decision or for the game's reason, not for what the code says.
- Record a known difference from the game at the field: `#[diff(gap = "why")]`. Mark a field
  that the save does not hold `#[diff(unsaved)]`, and a field compared as one value
  `#[diff(eq)]`.

## 14. Tests

- Work test-driven. Write the tests with the port, from the decompile's behaviour. Watch them
  fail, then make them pass. A port without its tests is not done.
- Give a branch that no recording reaches a unit test.
- Put a module's tests into `tests.rs` beside it, included by `#[cfg(test)] mod tests;`.
- Name a test with a sentence that states the behaviour:
  `a_step_that_fails_stops_what_follows_it`.
- Stand in for an unmodelled type, class or action with a value that the game never uses (99),
  not with a real one that is only unported yet.
- Build a test's newtypes through `test_ids`: `object_id(n)` (a `const fn`, so a constant can
  name an object), `object_id_list` and `object_ids` for lists, `RecordId::from(object_id(n))`
  for an object's record. Write none for a field that holds none, never `object_id(-1)`.
- Compare a ported routine bit for bit with the game's results where they exist: the
  `tests/fixtures/fuzz_*_158041.txt` files and `tests/routines.rs`.
- Mark a test that takes minutes or gigabytes `#[ignore = "expensive: …"]`.
- Run the tests with `cargo nextest`. If you pipe test output, set `-o pipefail` first.

## 15. Lints

- The workspace's lint set applies to every crate: clippy's `pedantic` and `nursery`,
  `missing_docs`, and `unsafe_code = "forbid"`.
- CI and the pre-commit hook run clippy with `-D warnings`. The source never holds
  `#![deny(warnings)]`.
- Fix a warning. Do not filter it away.
- Give an allowance a reason tied to the game's behaviour, written beside it, on the narrowest
  item. `scripts/lint_allows.py --check` fails on a new allowance.
- genie-core turns off the cast and float lints for the game's arithmetic. No other crate does,
  except for a field width that its file format fixes.

## 16. The gates

Enable the pre-commit hook once per clone: `git config core.hooksPath .githooks`. Then a commit
cannot fail CI's checks. `.github/workflows/ci.yml` runs the same checks and more.

| Gate | Command | Where |
| --- | --- | --- |
| Format | `cargo fmt --all --check` | hook, CI |
| Clippy | `cargo clippy --locked --workspace --all-targets -- -D warnings` | hook, CI |
| File length | `scripts/file_size.py` | hook, CI |
| Allowances | `scripts/lint_allows.py` | hook, CI |
| Folder READMEs | `scripts/readmes.py crates/engine/genie-core/src --check` | hook, CI |
| Effects inventory | `scripts/effects.py crates/engine/genie-core/src --check` | hook, CI |
| Schedule | `scripts/schedule.py crates/engine/genie-core/src --check` | hook, CI |
| CRAP | `scripts/crap.sh` (the tests under coverage) | hook |
| Tests | `cargo test --locked --workspace`, on Linux, Windows and macOS | CI |
| WebAssembly | the engine crates built for `wasm32-unknown-unknown` | CI |
| Dependency direction | no format crate depends on an engine or tool crate | CI |
| Docs | `cargo doc --locked --workspace --no-deps` | CI |

## 17. Commits

- Make one change per commit.
- Write the subject as `crate: the change`, for example
  `genie-core: the Attack's start ends at once where the start does`.
- A commit that changes the simulation passes the checkpoint check with no regression.
