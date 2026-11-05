//! An object's auras: its master's tasks of type 155 ([`ActionKind::AURA`]) in the groups that
//! slot 277 builds of them (`TRIBE_Combat_Object::slot277`, `0x141206d50`), which the powerup
//! update (`FUN_141204a10`, [`World::powerup_update`]) reads.
//!
//! The game builds the groups when the object's master changes (its constructor, a master change
//! `FUN_14107cb70`, the hit point transform `FUN_141204450`, slot 262 for an effect's master),
//! clears and refills them there, and keeps them in `+0xa58` and `+0xaa8`. genie derives them
//! from the object's master where they are read ([`aura_groups`]), in the order the game's maps
//! list them ([`insertion_order`]).
//!
//! [`World::powerup_update`]: crate::world::World::powerup_update

mod update;

use crate::combat::ClassOrMaster;
use crate::combat::powerup::{PowerupAmount, PowerupAttribute, PowerupShare};
use crate::map::Distance;
use crate::math::msvc::{HashKey, MapHash, float_hash, fnv1a, hash_combine, insertion_order};
use crate::objects::Graphics;
use crate::objects::SpriteId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::Masters;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::Task;
use crate::objects::masters::TaskFlags;
use crate::players::RelationMask;
use crate::players::UnitRelation;
use crate::players::attribute::AttributeId;
use crate::vision::registry::QueryShape;

/// What an aura's entry does with the objects it names (the entry's `+0x16`, the task's
/// [`Task::search_flag`]; the value, 1 or 2, of its part's map of targets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuraRole {
    /// 1: the objects take the powerup.
    Powered,
    /// 2: the objects count toward the amount.
    Counted,
}

/// An aura task as its group keeps it (slot 277's entry of `+0xaa8`, 40 bytes): its target, what
/// it does with the objects it names, its flags and the attribute that enables it. The entry's
/// other fields copy its group's keys; its sound fields (`+0x60`, `+0x78`, `+0x34`, into
/// `+0xac4`..`+0xacc`) are the audio's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AuraEntry {
    /// The object it names.
    pub(crate) target: ClassOrMaster,
    /// What it does with them.
    pub(crate) role: AuraRole,
    /// The task's flags (`+0x15`).
    pub(crate) flags: TaskFlags,
    /// The player's attribute that enables it (`+0x38`), none for always.
    pub(crate) enablement: Option<AttributeId>,
}

/// Where an aura group searches (the key of slot 277's map `+0xa58`): its radius (the task's
/// `work_range`), its shape (a rounded rectangle, mode 0, with [`TaskFlags::AURA_ROUNDED`], else
/// a rectangle, mode 1) and the relations it keeps ([`aura_relations`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AuraArea {
    /// How far it searches.
    pub(crate) radius: Distance,
    /// The query's shape.
    pub(crate) shape: QueryShape,
    /// The relations it keeps.
    pub(crate) relations: RelationMask,
}

impl HashKey for AuraArea {
    /// `FUN_1411e86d0`: the radius's, the mode's and the relations' hashes combined in turn.
    fn map_hash(self) -> MapHash {
        let seed = hash_combine(0, float_hash(self.radius.get()));
        let seed = hash_combine(seed, fnv1a(&self.shape.get().to_le_bytes()));
        MapHash::Data(hash_combine(seed, fnv1a(&[self.relations.get()])))
    }
}

/// The most objects an aura part counts (its task's `work_val2`, the part key's `+0xc`): its
/// amount is its own at that many, a share of it at fewer ([`aura_amount`]).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub(crate) struct MostObjects(f32);

impl MostObjects {
    /// The float `value`.
    #[must_use]
    pub(crate) const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub(crate) const fn get(self) -> f32 {
        self.0
    }
}

/// The objects an aura part found (`FUN_141204a10`): its powered list's length plus its count
/// (`+0x98`), a 32-bit integer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AuraCount(i32);

impl AuraCount {
    /// None.
    pub(crate) const ZERO: Self = Self(0);

    /// The count with one counted object more.
    #[must_use]
    pub(crate) const fn counted(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The part's objects: `powered` listed ones (the list's length, `SAR 2` as 32 bits) plus
    /// the counted ones (`ADD`).
    #[must_use]
    pub(crate) const fn with_powered(self, powered: usize) -> Self {
        Self((powered as i32).wrapping_add(self.0))
    }

    /// Whether it found none.
    #[must_use]
    pub(crate) const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// The count as the amount's float (`CVTDQ2PS`).
    #[must_use]
    pub(crate) const fn as_float(self) -> f32 {
        self.0 as f32
    }
}

/// The amount an aura part of `powerup` gives for `objects` objects (`FUN_141204a10`): the count
/// (`CVTDQ2PS`), the most where the count is above it (`COMISS`/`JBE`: a NaN most keeps the
/// count), times the amount (`MULSS`), over the most (`DIVSS`).
#[must_use]
pub(crate) fn aura_amount(objects: AuraCount, powerup: AuraPowerup) -> PowerupAmount {
    let (count, most) = (objects.as_float(), powerup.most.get());
    let counted = if most < count { most } else { count };
    PowerupAmount::new((counted * powerup.amount.get()) / most)
}

/// The powerup an aura part gives (the key of a group's map at its node's `+0x20`): whether a
/// share ([`TaskFlags::AURA_SHARE`]), the attribute (`search_wait_time` as a whole number), the
/// amount (`work_val1`), the most objects that count (`work_val2`) and the sprite the powered
/// objects show (`+0x48`, the task's proceeding sprite, the renderer's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AuraPowerup {
    /// What the amount is.
    pub(crate) share: PowerupShare,
    /// The attribute it raises.
    pub(crate) attribute: PowerupAttribute,
    /// The amount at the most objects.
    pub(crate) amount: PowerupAmount,
    /// The most objects that count.
    pub(crate) most: MostObjects,
    /// The sprite, none where the task has none the graphics hold (the game's null pointer).
    pub(crate) sprite: Option<SpriteId>,
}

impl HashKey for AuraPowerup {
    /// `FUN_141200200`: the share's byte, the attribute's, the amount's, the most's and the
    /// sprite pointer's eight bytes combined in turn. A part without a sprite hashes the null
    /// pointer, as the game does. A part with one hashes the sprite's address, which differs in
    /// every process: two of the game's clients can list such parts differently, and with them
    /// the Object stream's draws of the objects they power up (158041's HZAWISZA 1078, ZHANGFEI
    /// 2038, HYUJI 2047 and CAOCAO 2052 have two parts that differ by the sprite alone). genie
    /// lists such a part as the game does most often ([`MapHash::Address`]).
    fn map_hash(self) -> MapHash {
        if self.sprite.is_some() {
            return MapHash::Address;
        }
        let share = u8::from(self.share == PowerupShare::MasterShare);
        let seed = hash_combine(0, fnv1a(&[share]));
        let seed = hash_combine(seed, fnv1a(&self.attribute.get().to_le_bytes()));
        let seed = hash_combine(seed, float_hash(self.amount.get()));
        let seed = hash_combine(seed, float_hash(self.most.get()));
        MapHash::Data(hash_combine(seed, fnv1a(&0_u64.to_le_bytes())))
    }
}

/// One powerup of an aura group and the entries that give it (a node of the group's map: the
/// entries' places in `+0xaa8` at its `+0x28`, in the tasks' order).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AuraPart {
    /// The powerup.
    pub(crate) powerup: AuraPowerup,
    /// Its entries.
    pub(crate) entries: Vec<AuraEntry>,
}

/// An aura group: what one query finds and the powerups it gives (a node of slot 277's map
/// `+0xa58`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AuraGroup {
    /// Where it searches.
    pub(crate) area: AuraArea,
    /// The classes its query reads (the node's `+0x60`): each entry's, once, in the tasks' order.
    pub(crate) classes: Vec<ClassId>,
    /// Its powerups, in its map's order.
    pub(crate) parts: Vec<AuraPart>,
}

/// The relations an aura task keeps (slot 277's switch on its `owner_type`, `+0x18`, one bit
/// each, [`UnitRelation::bit`]): its own (1: 2), the neutrals' and the enemies' (2: `0x18`),
/// Gaia's (3: 1), Gaia's, its own and its allies' (4: 7), Gaia's, the neutrals' and the enemies'
/// (5: `0x19`), all but its own (6: `0x1d`), any other every one (`0x1f`).
pub(crate) fn aura_relations(owner: TargetOwner) -> RelationMask {
    use UnitRelation::{Ally, Enemy, Gaia, Neutral, Own};
    let relations: &[UnitRelation] = match owner {
        TargetOwner::OWN => &[Own],
        TargetOwner::ENEMY => &[Neutral, Enemy],
        TargetOwner::GAIA => &[Gaia],
        TargetOwner::FRIENDLY => &[Gaia, Own, Ally],
        TargetOwner::ENEMY_OR_GAIA => &[Gaia, Neutral, Enemy],
        TargetOwner::OTHER => &[Gaia, Ally, Neutral, Enemy],
        _ => &UnitRelation::ALL,
    };
    RelationMask::of(relations)
}

/// Slot 277's aura groups of `master`'s tasks (`0x141206d50`): each task of type 155 whose
/// target resolves ([`ClassOrMaster::query_class`] in `masters`, the object's player's table) and
/// whose most objects (`work_val2`) are above 0 joins the group of its area, its class the
/// group's, and its entry the part of its powerup; groups and parts in the order the game's maps
/// list them, entries in the tasks' order. `graphics` resolves the sprite as the game's pointer.
/// Not held: the UI's flag `+0x78a` (a task of type 3 on class 3 or 13, `FUN_141189bc0`), the
/// sounds `+0xac4`..`+0xacc` and the hit point transforms `+0xb20` (type 158); the unit AI's
/// leash `+0xb74` (type 6) is [`crate::unit_ai::NewObject::of`]'s.
#[must_use]
pub(crate) fn aura_groups(
    master: &Master,
    masters: Option<&Masters>,
    graphics: &Graphics,
) -> Vec<AuraGroup> {
    let mut groups: Vec<AuraGroup> = Vec::new();
    let auras = master
        .action_part()
        .tasks
        .iter()
        .filter(|task| task.action_kind == ActionKind::AURA);
    for task in auras {
        let target = ClassOrMaster::of_task(task);
        let Some(class) = target.query_class(masters) else {
            continue;
        };
        if !task.work_val2.positive() {
            continue;
        }
        let group = group_of(&mut groups, area_of(task));
        if !group.classes.contains(&class) {
            group.classes.push(class);
        }
        let entry = AuraEntry {
            target,
            role: if task.search_flag == 0 {
                AuraRole::Powered
            } else {
                AuraRole::Counted
            },
            flags: task.combat_level_flag,
            enablement: task.enablement,
        };
        part_of(&mut group.parts, powerup_of(task, graphics))
            .entries
            .push(entry);
    }
    let groups: Vec<AuraGroup> = groups
        .into_iter()
        .map(|group| AuraGroup {
            parts: in_map_order(group.parts, |part| part.powerup),
            ..group
        })
        .collect();
    in_map_order(groups, |group| group.area)
}

/// The area of an aura task (slot 277's outer key).
fn area_of(task: &Task) -> AuraArea {
    AuraArea {
        radius: task.work_range,
        shape: if task.combat_level_flag.contains(TaskFlags::AURA_ROUNDED) {
            QueryShape::RoundedRectangle
        } else {
            QueryShape::Rectangle
        },
        relations: aura_relations(task.owner_kind),
    }
}

/// The powerup of an aura task (slot 277's inner key): its sprite where `graphics` holds it.
fn powerup_of(task: &Task, graphics: &Graphics) -> AuraPowerup {
    AuraPowerup {
        share: if task.combat_level_flag.contains(TaskFlags::AURA_SHARE) {
            PowerupShare::MasterShare
        } else {
            PowerupShare::Plain
        },
        attribute: PowerupAttribute::of_task(task.wait_time),
        amount: PowerupAmount::new(task.work_val1.get()),
        most: MostObjects::new(task.work_val2.get()),
        sprite: task
            .sprites
            .proceeding
            .filter(|&sprite_id| graphics.get(sprite_id).is_some()),
    }
}

/// The group of `area` in `groups` (`FUN_1411e9710`'s find), a new one at the end where none is
/// equal to it.
fn group_of(groups: &mut Vec<AuraGroup>, area: AuraArea) -> &mut AuraGroup {
    let at = groups
        .iter()
        .position(|group| group.area == area)
        .unwrap_or_else(|| {
            groups.push(AuraGroup {
                area,
                classes: Vec::new(),
                parts: Vec::new(),
            });
            groups.len() - 1
        });
    &mut groups[at]
}

/// The part of `powerup` in `parts` (`FUN_1411e9390`'s find), a new one at the end where none is
/// equal to it.
fn part_of(parts: &mut Vec<AuraPart>, powerup: AuraPowerup) -> &mut AuraPart {
    let at = parts
        .iter()
        .position(|part| part.powerup == powerup)
        .unwrap_or_else(|| {
            parts.push(AuraPart {
                powerup,
                entries: Vec::new(),
            });
            parts.len() - 1
        });
    &mut parts[at]
}

/// `items`, each a new key `key` of a map in turn, in the order the map lists them
/// ([`insertion_order`]).
fn in_map_order<T, K: HashKey>(items: Vec<T>, key: impl Fn(&T) -> K) -> Vec<T> {
    let keys: Vec<K> = items.iter().map(key).collect();
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    insertion_order(&keys)
        .into_iter()
        .filter_map(|position| slots.get_mut(position).and_then(Option::take))
        .collect()
}

#[cfg(test)]
mod tests;
