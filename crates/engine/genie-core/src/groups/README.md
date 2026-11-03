# `groups`: unit groups and formations

The order splitter (`orders/split.rs`) puts soldiers that move together into a unit group. The
group moves as one: it has its own path and motion, keeps its members in a formation, and gives
each member a move of its own.

The folder follows the parts of a group:

- `mod.rs` holds the group's state, field for field as the save writes it. A save of any build
  fills it, and the forced replay compares every field.
- `create.rs` adds members, `stats.rs` keeps the group's centroid, radii, ranges and speed, and
  `place.rs` sets a new group's place and facing.
- `command.rs` holds the group's orders: move, stop and the other commands.
- `formation/` lays out the formation's slots and gives each slot a member.
- `motion/` moves the group along its path.
- `steer/` brings each member to its slot.
- `update/` is the group's update in each tick.

## How the tick reaches it

The world runs the update of each group that a player owns, before the player's objects
(`update/`). A group whose update returns 0 is deleted.

## In the game

- `BUnitGroup` (`group.cpp`), `BFormation` (`formtn.cpp`) and `BMotion` (`motion.cpp`). The names
  are those of the HD edition's debug symbols.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`attack.rs`](attack.rs) | A group's command on an attack task (FD-036): `FUN_14100f7f0`, which `BUnitGroup::command` runs for the tasks 1, 7, 9, `0x68` and `0x7d` on a target, with the member passes it makes (`FUN_14100c6c0`, `FUN_141007ba0`, `FUN_14102bbd0`). |
| [`command.rs`](command.rs) | A group's orders (FD-036): `BUnitGroup::command` (`FUN_14100efd0`), `commandMoveTo` (`FUN_141010a80`) and `stopAllUnits` (`FUN_14103a0d0`), the HD PDB's names. |
| [`command_kind.rs`](command_kind.rs) | A group command's kind (`GroupCommandKind`): a move, a follow, or the action type of the task the group's members carry out. |
| [`create.rs`](create.rs) | A group's members (FD-036): `AddUnit` (`FUN_1410083c0`) with its sub-formation statistics (`FUN_141008d90`, `FUN_1410116c0`) and a unit's radius in a formation (`FUN_141025920`). |
| [`differentiator.rs`](differentiator.rs) | A group's differentiator (`mDifferentiatorID`): the order splitter's command count. |
| [`formation_category.rs`](formation_category.rs) | A unit's formation category in its group (`FormationCategory`): the sub-formation its master's type, hero flag and formation category put it in. |
| [`formation_kind.rs`](formation_kind.rs) | A formation's type as one type: `BFormation`'s `mType`, a group's lock, a unit AI's formation type and the `FORM_FORMATION` command's. |
| [`group_flags.rs`](group_flags.rs) | A group's flags (`GroupFlags`, `mGroupFlags`). |
| [`group_id.rs`](group_id.rs) | A unit group's id as one type: the id the world gives each group it creates, which a unit's group link, a group's differentiator groups and the lookups by id name. |
| [`group_list.rs`](group_list.rs) | The world's unit groups (`UnitGroups`), an object's link to its group and the group every listed unit is in. |
| [`layout.rs`](layout.rs) | The formations' layouts besides createColumnLine (FD-036): a horde (`FUN_141013eb0`), a user defined formation (`FUN_141014f60`), a phalanx (`FUN_1410147b0`) and a box (`FUN_141011a00` with `FUN_14102ab30` and `CreateLines` `FUN_1410143a0`), as `UpdateDesiredPositions` (`FUN_14103fd00`) makes them. |
| [`lock.rs`](lock.rs) | A group's formation type (FD-036): `CalcFormationType` (`FUN_14100b940`), the members' vote after a change of members; `LockFormation` (`FUN_141024e70`), the lock to a type; and the members' stances as the group's state (`FUN_14100d140`). |
| [`mod.rs`](mod.rs) | Unit groups and formations (FD-036): the game's `BUnitGroup` (`group.cpp`) with its motion (`BMotion`) and formations (`BFormation`). |
| [`motion_flags.rs`](motion_flags.rs) | A group motion's flags (`MotionFlags`, `mMotionFlags`). |
| [`move_reason.rs`](move_reason.rs) | Why a group member moves on its own (`mReason`), and the path of that move (`mPathID`). |
| [`objects.rs`](objects.rs) | An object of the world's id table (`WorldObject`) as the groups read it, and a unit's sub-formation. |
| [`order.rs`](order.rs) | A group's command (FD-036): `BUnitGroup::command` (`FUN_14100efd0`), which chooses by the task a member has on the target how the group's order runs. |
| [`place.rs`](place.rs) | A new group's place and facing (FD-036): `SetInitialPosition` (`FUN_141039990`) on the first clear point of a spiral round the centroid (`FUN_141007630`), `SetInitialOrientation` (`FUN_141039680`) and the motion's orientation (`FUN_141049e10`). |
| [`slot_state.rs`](slot_state.rs) | A formation slot's state: its member steers to it, reaches it, stops at it or stands in it. |
| [`stats.rs`](stats.rs) | A group's statistics (FD-036): `FUN_14100bcb0`, the centroid, radii, ranges, spacing and speed of the members, the objects their searches list, and the formation's type after a change of members (`FUN_14100b940`). |
| [`target.rs`](target.rs) | A group's target: an object, none, or a target of another kind. |
| [`task.rs`](task.rs) | The task a group's member takes on a target (FD-036): the unit's slot 0x130 (`RGE_Action_Object::slot38`, `FUN_14106b8b0`), which `BUnitGroup::command` asks of its first member that can act to choose the command a group's order runs. |
| [`tests.rs`](tests.rs) | Tests. |
| [`transform_kind.rs`](transform_kind.rs) | How a formation's transform places its slots (`mTransformType`). |
| [`unit_kind.rs`](unit_kind.rs) | A group's unit type: a master's id as the statistics' and the slots' words hold it. |
| [`formation/`](formation/) | A group's formation (FD-036): `BFormation`, its slots and lines, and the layouts that fill them (createColumnLine `FUN_141012280`, ...). |
| [`motion/`](motion/) | A group's motion (FD-036): `BMotion`'s update (`FUN_14104a090`, motion.cpp): its path step (`FUN_14104a270`) over the path levels a unit's motion step keeps alike, planned for the group's lead from the group's place, the walk toward the walked level's waypoint with its turn (`FUN_141046270`, `FUN_14104c150`), then the velocity along the vectors. |
| [`steer/`](steer/) | The members' moves to their slots (FD-036): the part of `BUnitGroup::update` (`FUN_14103d910`, `0x14103e8e8`..`0x14103efba`) that brings each member of a formation to its slot, with the member's steering (`FUN_141025fb0`) and the setter of a member's own move (`FUN_141039f60`). |
| [`update/`](update/) | A group's update (FD-036): `BUnitGroup::update` (`FUN_14103d910`), which the world runs for the groups a player owns before the player's objects, and the deletion of a group whose update returns 0 (the world list's unlink, then `BUnitGroup`'s destructor, `FUN_140fe1920`). |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../combat/), [`construction`](../construction/), [`gathering`](../gathering/), [`map`](../map/), [`math`](../math/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`pathing`](../pathing/), [`players`](../players/), [`testing`](../testing/), [`unit_ai`](../unit_ai/), [`vision`](../vision/), [`world`](../world/).
- Used by: [`commands`](../commands/), [`garrison`](../garrison/), [`gathering`](../gathering/), [`map`](../map/), [`movement`](../movement/), [`objects`](../objects/), [`orders`](../orders/), [`unit_ai`](../unit_ai/), [`world`](../world/).
<!-- /uses -->
