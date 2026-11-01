# `objects/masters`: the definitions of the objects

A master is the definition of one kind of object for one player: its hit points, speed, armor,
attacks, costs, sprites and tasks. Each player has its own copy of every master. An effect
changes the copies of the players that it reaches.

The values are game data. They come from the player's masters in the save, and from the `.dat`
file for what the save does not hold.

- `mod.rs` holds the `Master` type and its tasks.
- `lookup.rs` finds a player's master by id, and the master and task that a work command takes.
- An effect changes a master through the attribute setters. The game has one setter per master
  type, and `dispatch.rs` picks it:
  - `static_setters.rs`: the static, animated, moving and action masters.
  - `combat_setters.rs`: the combat and missile masters (`RGE_`).
  - `tribe_setters.rs`: the combat and building masters of Age of Empires II's layer
    (`TRIBE_`).
- `derived.rs` updates the values that follow from a changed master.
- `attribute.rs` names the attributes that an effect changes.

## In the game

- The master classes (`MasterStaticObject.cpp` and the classes above it), with their attribute
  setters and their copy for an upgrade.

## Files

<!-- files -->
| File | What it holds |
| --- | --- |
| [`action_type.rs`](action_type.rs) | The action types (`TRIBE_Action_*`'s `+0x8`, a task's `action_type`): the action a task makes a unit do, and the type of an action a unit runs. |
| [`adjacent_mode.rs`](adjacent_mode.rs) | A building master's adjacent mode (`+0x980`): whether walls and gates join their neighbours. |
| [`attack_priority.rs`](attack_priority.rs) | A master's attack priority: which targets its unit AI's checks let it choose. |
| [`attribute.rs`](attribute.rs) | The attributes an effect changes on a master and on its objects (the game's attribute list: string 12200 plus the number names each). |
| [`blast_level.rs`](blast_level.rs) | A master's blast level and blast defence (`BlastLevel`, `BlastDefense`). |
| [`carry_capacity.rs`](carry_capacity.rs) | A master's carry capacity (`+0x61c`): how much an object of it carries. |
| [`charge_event.rs`](charge_event.rs) | A master's charge event (`+0x920`): the range a ranged charge adds. |
| [`charge_target.rs`](charge_target.rs) | A master's charge targets (`+0x924`): the groups of classes its charge works on. |
| [`charge_type.rs`](charge_type.rs) | A master's charge type (`+0x922`): what its charge does. |
| [`class.rs`](class.rs) | The master classes (`+0x18`, an `i16`): the one definition of the class values. |
| [`class_id.rs`](class_id.rs) | A master's class as one type: the group of masters the game treats alike (villagers, siege weapons, gates, farms), the value the `.dat` names `unit_class`. |
| [`class_mask.rs`](class_mask.rs) | A set of classes as the game tests one: a bit mask over the classes below a bound. |
| [`combat_ability.rs`](combat_ability.rs) | A master's combat abilities (`CombatAbility`, `combat_ability`). |
| [`combat_level.rs`](combat_level.rs) | A master's combat level (`+0x62c`): which of the unit AI's kinds of fighter it is. |
| [`combat_setters.rs`](combat_setters.rs) | The RGE combat and missile masters' setters: the armor and attack amounts. |
| [`conversion_adjust.rs`](conversion_adjust.rs) | A master's adjustments of a conversion (`ConversionAdjust`). |
| [`cost_slot.rs`](cost_slot.rs) | A master's cost (`+0x880`): three slots in their places, an empty one among them. |
| [`damage_amount.rs`](damage_amount.rs) | An attack's or an armor's amount. |
| [`damage_class.rs`](damage_class.rs) | An armor or attack entry's class as one type, and the entry: the `.dat`'s armor and attack classes. |
| [`damage_share.rs`](damage_share.rs) | A master's share of a damage: resisted, reflected, dealt to its own or by its blast. |
| [`derived.rs`](derived.rs) | The values the slice derives from a master after a change (`Master::follow`): the sprites' speed and facets, a moving unit's values. |
| [`dispatch.rs`](dispatch.rs) | The attribute setters by master type (slots 0x18, 0x20 and 0x28), the master copy (slot 8), the loader's defaults and the selected entries. |
| [`fly_mode.rs`](fly_mode.rs) | A master's fly mode (`+0x608`): where its objects' height goes on the ground. |
| [`fog_visibility.rs`](fog_visibility.rs) | A master's fog visibility (`+0x605`): whether its objects show in the fog of war. |
| [`formation_class.rs`](formation_class.rs) | A master's formation category (`+0x8bc`): the sub-formation its units take in a group. |
| [`garrison_capacity.rs`](garrison_capacity.rs) | A master's garrison capacity (`+0x5d4`): how many units an object of it holds. |
| [`garrison_kinds.rs`](garrison_kinds.rs) | The kinds of units a building takes in (`GarrisonKinds`, `garrison_type`). |
| [`gather_group.rs`](gather_group.rs) | A master's gather group as one type: the kind of resource the unit AI's resource searches look for, the value the `.dat` names `resource_gather_group`. |
| [`ghost_mode.rs`](ghost_mode.rs) | A master's `create_doppelganger_on_death` (`+0x682`): whether its objects leave ghosts. |
| [`hero_flags.rs`](hero_flags.rs) | A master's hero flags (`HeroFlags`, `hero_flag`). |
| [`hill_mode.rs`](hill_mode.rs) | A master's hill mode (`+0x604`): how much its site's tiles may differ in height. |
| [`hit_chance.rs`](hit_chance.rs) | A master's hit chance (`base_hit_chance`): the percent of its missiles and strikes that hit. |
| [`id_field.rs`](id_field.rs) | A master's id fields as its data holds them: shorts the effects set, add to and scale, every value kept, which name a master (`MasterField`) or a terrain restriction (`RestrictionField`) when 0 or more. |
| [`ids.rs`](ids.rs) | The master ids (`i16`, `crate::objects::masters::Master::id`): the one definition of the masters the engine names, by the `.dat`'s unit id. |
| [`interaction_mode.rs`](interaction_mode.rs) | A master's interaction mode (`+0x62d`): how a player may select and order its objects. |
| [`interface_kind.rs`](interface_kind.rs) | A master's interface kind (`+0x62f`): which command panel its objects show. |
| [`invulnerability.rs`](invulnerability.rs) | A master's invulnerability level: the hit points a hit cannot take. |
| [`lookup.rs`](lookup.rs) | A player's masters by id (`Masters`) and the work command's choice of master and task. |
| [`master_id.rs`](master_id.rs) | A master's id as one type: the master a command builds, trains or turns an object into, and a target's master. |
| [`master_kind.rs`](master_kind.rs) | A master's type as one type: the byte at `+0x8` that tells a static object from a combat unit or a building, which the `.dat` names `unit_type`. |
| [`master_type.rs`](master_type.rs) | The master types (`+0x8`, the object's kind at `+0xf`): the one definition of the type bytes. |
| [`minimap_mode.rs`](minimap_mode.rs) | A master's minimap mode (`+0x62e`): how its objects show on the minimap. |
| [`mod.rs`](mod.rs) | What the engine reads of a player's masters: the tasks a unit can do and the masters a unit changes into to do them, and how an effect changes a master: the attribute setters (slots 0x18, 0x20 and 0x28 of every master type, `Master::set_attribute`), the master copy of an upgrade (slot 8, `Master::copy_from`) and the values the slice derives from a master (`Master::follow`). |
| [`object_flags.rs`](object_flags.rs) | A master's object flags (`ObjectFlags`): its traits and the master its upper half names. |
| [`obstruction_class.rs`](obstruction_class.rs) | A master's obstruction class as one type: what its obstruction record blocks, the value the `.dat` names `obstruction_class`. |
| [`obstruction_type.rs`](obstruction_type.rs) | A master's obstruction type as one type: the shape of the record its objects file in the obstruction manager, the value the `.dat` names `obstruction_type`. |
| [`occlusion.rs`](occlusion.rs) | A master's occlusion mode (`+0x650`): how its objects show behind others. |
| [`placement_terrains.rs`](placement_terrains.rs) | A master's placement terrains (`PlacementTerrains`). |
| [`run_pattern.rs`](run_pattern.rs) | A master's run pattern (`+0x7b9`). |
| [`selection_effect.rs`](selection_effect.rs) | A master's selection effect (`+0x684`): how a selected object of it shows. |
| [`sound_events.rs`](sound_events.rs) | A task's two sound events (`SoundEvents`) and a sound event's id (`SoundEventId`). |
| [`special_attack.rs`](special_attack.rs) | A master's special attack (`+0x8d8`): what a hit on its objects or their charge does. |
| [`static_setters.rs`](static_setters.rs) | The static, animated, moving and action masters' setters. |
| [`target_owner.rs`](target_owner.rs) | A task's owner type (`+0x18`): whose targets the task is for. |
| [`task_flags.rs`](task_flags.rs) | A task's flags (`TaskFlags`, `combat_level_flag`). |
| [`task_group.rs`](task_group.rs) | A master's task swap group (`+0x7b8`): the masters a unit may switch between for a task. |
| [`task_id.rs`](task_id.rs) | A task's id as one type: the place of a task in its master's task list, which an action keeps to name the task it works on. |
| [`task_index.rs`](task_index.rs) | A place in a master's task list: its default task (`+0x790`). |
| [`task_sprites.rs`](task_sprites.rs) | A task's four sprites (`TaskSprites`). |
| [`task_terrain.rs`](task_terrain.rs) | A task's terrain (`+0x16`): the terrain a task finder's task is for, or a terrain look task's flag mask. |
| [`tests.rs`](tests.rs) | Tests. |
| [`tribe_setters.rs`](tribe_setters.rs) | The TRIBE combat and building masters' setters: the costs, the charge, the sprites and the train locations. |
| [`values.rs`](values.rs) | What the attribute setters share: the caps, the truncations, the sprite and class amounts, the damage sprites' and train locations' entries. |
| [`volley_spread.rs`](volley_spread.rs) | How far a volley's missiles scatter (`VolleySpread`). |
| [`work_value.rs`](work_value.rs) | A task's work values. |
<!-- /files -->

## How it connects

<!-- uses -->
- Uses: [`combat`](../../combat/), [`construction`](../../construction/), [`gathering`](../../gathering/), [`map`](../../map/), [`math`](../../math/), [`movement`](../../movement/), [`players`](../../players/), [`production`](../../production/), [`technology`](../../technology/), [`unit_ai`](../../unit_ai/), [`world`](../../world/).
<!-- /uses -->
