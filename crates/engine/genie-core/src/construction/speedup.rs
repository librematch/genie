//! A speed-up building's work on the objects about it (`FUN_1411c39b0`, `FUN_1411c4a00`): a
//! building that its player's attribute 255 names gives each allied object of its player's
//! object target on the tiles of its reach a moving speed-up, a work-rate speed-up or a heal.

use crate::combat::ClassOrMaster;
use crate::combat::damage::ArmorPair;
use crate::combat::damage::TransformTest;
use crate::combat::damage::gain_hp;
use crate::construction::build::NOT_HELD;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::Regeneration;
use crate::objects::hit_points::REGENERATION_MINUTE;
use crate::objects::layers::Action;
use crate::objects::layers::Speedup;
use crate::objects::layers::SpeedupValue;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::unit::Unit;
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::allied;
use crate::world::NO_ATTRIBUTE;
use crate::world::Player;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::AttributeSlots;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

/// What a speed-up building's effect type names (attributes 259 and 260, `CVTTSS2SI`;
/// `FUN_1411c4a00`'s second argument, an attribute of the game's list), with its amount
/// (attributes 257 and 261, its fourth).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SpeedupEffect {
    /// 5, "Movement Speed": a moving object's speed-up (`FUN_14114cb70`).
    MovementSpeed(SpeedupValue),
    /// 13, "Work Rate": an action object's speed-up (`FUN_14106a510`).
    WorkRate(SpeedupValue),
    /// 109, "Regeneration Rate": a heal by slot 0xb8, the amount added to the building's own
    /// regeneration.
    Regeneration(Regeneration),
    /// Any other value: nothing.
    Other,
}

impl SpeedupEffect {
    /// The effect an attribute's `kind` names (`CVTTSS2SI`: a NaN names none), by `amount`.
    fn of(kind: Amount, amount: Amount) -> Self {
        let kind = cvttss2si(kind.get());
        let names = |attribute: MasterAttribute| kind == i32::from(attribute.get());
        if names(MasterAttribute::MOVEMENT_SPEED) {
            Self::MovementSpeed(SpeedupValue::of_data(amount.get()))
        } else if names(MasterAttribute::WORK_RATE) {
            Self::WorkRate(SpeedupValue::of_data(amount.get()))
        } else if names(MasterAttribute::REGENERATION_RATE) {
            Self::Regeneration(Regeneration::new(amount.get()))
        } else {
            Self::Other
        }
    }
}

/// A building that its player's attribute 255 names, as `FUN_1411c39b0` reads it and its
/// player's attributes 256 to 261 (`player+0x100`'s `+0x800` to `+0x828`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpeedupBuilding {
    /// Its id (`+8`), which each speed-up it gives holds.
    id: ObjectId,
    /// Its player (`+0x138`), whose relations the ally test reads.
    player: PlayerId,
    /// How far its footprint grows (attribute 256).
    reach: Distance,
    /// The objects it works on (attribute 258, `CVTTSS2SI`).
    target: ClassOrMaster,
    /// Its effects in their order: attribute 259 by 257, then 260 by 261.
    effects: [SpeedupEffect; 2],
}

impl SpeedupBuilding {
    /// Building `id` of master `master` and of player `owner`, holding `attributes` (read through
    /// the build's slots `slots`): its speed-up while attribute 255 names the master (`UCOMISS`
    /// with the master's id, `CVTDQ2PS`; `JP`: a NaN names none), else none. A player without
    /// attribute 255 names none.
    ///
    /// # Errors
    ///
    /// Named, a player without one of the other attributes.
    fn of(
        id: ObjectId,
        owner: PlayerId,
        attributes: &Player,
        slots: &AttributeSlots,
        master: MasterId,
    ) -> GenieResult<Option<Self>> {
        if attributes.attribute(slots.speedup_building)
            != Some(Amount::new(f32::from(master.get())))
        {
            return Ok(None);
        }
        let read = |slot| attributes.attribute(slot).ok_or(NO_ATTRIBUTE);
        Ok(Some(Self {
            id,
            player: owner,
            reach: Distance::new(read(slots.speedup_range)?.get()),
            target: ClassOrMaster::of_data(cvttss2si(read(slots.speedup_object)?.get())),
            effects: [
                SpeedupEffect::of(read(slots.speedup_effect)?, read(slots.speedup_percentage)?),
                SpeedupEffect::of(
                    read(slots.speedup_secondary_effect)?,
                    read(slots.speedup_secondary_percentage)?,
                ),
            ],
        }))
    }
}

/// What `FUN_1411c4a00` reads of an object on a tile: a ghost as an object of its viewer and the
/// ghost master.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TileObject {
    /// Its id.
    id: ObjectId,
    /// Its player (`+0x138`): a ghost's viewer.
    player: PlayerId,
    /// Its type (`+0xf`): a ghost's 25.
    kind: MasterKind,
    /// Its master's id (`+0xa0`'s `+0x58`).
    master: MasterId,
    /// Its master's class (`+0x410`).
    class: ClassId,
    /// Its slot in its player's lists; none for a ghost, which its viewer's ghosts hold.
    place: Option<Place>,
}

/// The tiles a speed-up building's reach covers (`FUN_1411c39b0`): its footprint at `at` of radii
/// `radius` grown by `reach`, from the place less both (`CVTTSS2SI`; a negative tile 0, `CMOVNS`)
/// to both plus the place less `edge` (the build's `DAT_142eb1e9c`), at most the map's last
/// column and row (`CMOVL`). Returns the low and the high corner: a high one below the low (a
/// NaN's) covers nothing.
#[must_use]
pub(crate) fn reach_tiles(
    at: Position,
    radius: Footprint,
    reach: Distance,
    map: GridSize,
    edge: f32,
) -> (Tile, Tile) {
    let low = |place: f32, radius: f32| cvttss2si(place - (reach.get() + radius)).max(0);
    let high = |place: f32, radius: f32, size: i32| {
        let tile = cvttss2si(((reach.get() + radius) + place) - edge);
        if tile < size { tile } else { size - 1 }
    };
    (
        Tile::new(low(at.x(), radius.x()), low(at.y(), radius.y())),
        Tile::new(
            high(at.x(), radius.x(), map.width()),
            high(at.y(), radius.y(), map.height()),
        ),
    )
}

impl Unit {
    /// `FUN_1411c39b0`, the start of a built building's whole seconds (`0x1411c9000`, state 2):
    /// while its player's attribute 255 names its master `master` ([`SpeedupBuilding::of`]),
    /// the tiles of its reach about its footprint of radii `radius` ([`reach_tiles`], the build's
    /// `TurnTuning::speedup_edge`), row by row and each row's columns in order, take its two
    /// effects in their order ([`World::speed_up_tile`]). The walk meets the building on its
    /// own tiles: it runs with the building lent back to the world ([`Step::lend`]).
    ///
    /// # Errors
    ///
    /// A named building's player without its speed-up attributes, the lend's and the effects'.
    pub(crate) fn speed_up_around(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        master: MasterId,
        radius: Footprint,
    ) -> GenieResult<()> {
        let Some(building) = SpeedupBuilding::of(
            self.id,
            step.player,
            step.world.players.player(step.player)?,
            &step.world.model.slots.attribute,
            master,
        )?
        else {
            return Ok(());
        };
        let (low, high) = reach_tiles(
            self.position(),
            radius,
            building.reach,
            step.world.spatial.map,
            step.world.model.tuning.turn.speedup_edge,
        );
        step.lend((self, actions), |world| {
            for y in low.y()..=high.y() {
                for x in low.x()..=high.x() {
                    for effect in building.effects {
                        world.speed_up_tile(&building, effect, Tile::new(x, y))?;
                    }
                }
            }
            Ok(())
        })
    }
}

impl World {
    /// `FUN_1411c4a00`, a speed-up building's `effect` on `tile` (on the map: the reach holds
    /// it there): each object of the tile's list in its order (`+0x28`, its count `+0x1b` read
    /// again after each) that the building's player counts allied and its target names
    /// ([`World::reached`]) takes it ([`World::speed_up`]). The movement speed skips a tile a
    /// player marked ([`World::marked`]); an effect that is none of the three reads no object.
    ///
    /// # Errors
    ///
    /// An object on the tile the world does not hold, and the effect's.
    pub(crate) fn speed_up_tile(
        &mut self,
        building: &SpeedupBuilding,
        effect: SpeedupEffect,
        tile: Tile,
    ) -> GenieResult<()> {
        let walks = match effect {
            SpeedupEffect::MovementSpeed(_) => !self.marked(tile),
            SpeedupEffect::WorkRate(_) | SpeedupEffect::Regeneration(_) => true,
            SpeedupEffect::Other => false,
        };
        if !walks {
            return Ok(());
        }
        let mut place = 0;
        while let Some(&id) = self.spatial.tile_list(tile).get(place) {
            let object = self.on_tile(id)?;
            if self.reached(building, &object)? {
                self.speed_up(building, effect, &object)?;
            }
            place += 1;
        }
        Ok(())
    }

    /// Whether a player marked `tile` (the obstruction manager's word, `DAT_143f0a088 + 0x98`:
    /// the low 32 bits of its bits from 23, `SHR` and `TEST EDX`); a tile off the map, which a
    /// reach never covers, has no marks.
    fn marked(&self, tile: Tile) -> bool {
        (self.spatial.tiles().word(tile).unwrap_or(0) >> 23) as u32 != 0
    }

    /// Object `id` of a tile's list, as `FUN_1411c4a00` reads it: an object of the world's
    /// lists by its own master, else a ghost by its viewer and the ghost master.
    ///
    /// # Errors
    ///
    /// An id that names neither ([`NOT_HELD`]), and a master the tables lack.
    fn on_tile(&self, id: ObjectId) -> GenieResult<TileObject> {
        if let Some(place) = self.place(id) {
            let master = self
                .players
                .at(place)?
                .master(self.master_tables(), place.player())?;
            return Ok(TileObject {
                id,
                player: place.player(),
                kind: master.kind,
                master: master.id,
                class: master.class,
                place: Some(place),
            });
        }
        let (viewer, _) = self.ghost(id).ok_or(NOT_HELD)?;
        let master = self.master(viewer, self.model.slots.master.doppelganger)?;
        Ok(TileObject {
            id,
            player: viewer,
            kind: MasterKind::DOPPELGANGER,
            master: master.id,
            class: master.class,
            place: None,
        })
    }

    /// Whether `building`'s effects reach `object`: its player counts the object's allied
    /// (`FUN_141663d90` with 1) and its target names it ([`ClassOrMaster::targets`]).
    ///
    /// # Errors
    ///
    /// A building's player the world does not hold.
    fn reached(&self, building: &SpeedupBuilding, object: &TileObject) -> GenieResult<bool> {
        let relations = &self.players.player(building.player)?.relations;
        Ok(allied(
            relations,
            building.player,
            object.player,
            AllyTest::WithRelation4,
        ) && building.target.targets(object.class, object.master))
    }

    /// `effect` on `object`, which `building` reaches: a moving object's speed-up, an action
    /// object's or a heal.
    ///
    /// # Errors
    ///
    /// The effect's.
    fn speed_up(
        &mut self,
        building: &SpeedupBuilding,
        effect: SpeedupEffect,
        object: &TileObject,
    ) -> GenieResult<()> {
        match effect {
            SpeedupEffect::MovementSpeed(value) => self.speed_up_mover(building, value, object),
            SpeedupEffect::WorkRate(value) => self.speed_up_work(building, value, object),
            SpeedupEffect::Regeneration(amount) => self.regenerate_by(building, amount, object),
            SpeedupEffect::Other => Ok(()),
        }
    }

    /// The movement speed (5): a moving object (type 30 and up, `CMP 0x1e`, `JC`) takes the
    /// building's speed-up by `value` ([`Speedup::of_building`], `FUN_14114cb70`).
    ///
    /// # Errors
    ///
    /// A moving object without its movement state.
    fn speed_up_mover(
        &mut self,
        building: &SpeedupBuilding,
        value: SpeedupValue,
        object: &TileObject,
    ) -> GenieResult<()> {
        if object.kind < MasterKind::MOVING {
            return Ok(());
        }
        let mover = self.parts.mover_mut(object.id).ok_or_else(|| {
            GenieError::unreachable("a moving object without its movement state")
                .with("object", object.id)
        })?;
        mover.speedup = Speedup::of_building(building.id, value);
        Ok(())
    }

    /// The work rate (13): an action object (type 40 and up, `CMP 0x28`, `JC`) takes the
    /// building's speed-up by `value` on its action layer ([`Speedup::of_building`],
    /// `FUN_14106a510`); a class target spares the build's [`MasterRoles::WORK_SPEEDUP_SPARED`]
    /// (the town centre and the castle).
    ///
    /// # Errors
    ///
    /// The action layer's.
    fn speed_up_work(
        &mut self,
        building: &SpeedupBuilding,
        value: SpeedupValue,
        object: &TileObject,
    ) -> GenieResult<()> {
        let spared = building.target.class().is_some()
            && self
                .model
                .roles
                .of_master(object.master)
                .contains(MasterRoles::WORK_SPEEDUP_SPARED);
        if spared || object.kind < MasterKind::ACTION {
            return Ok(());
        }
        self.parts.actions_mut(object.id)?.speedup = Speedup::of_building(building.id, value);
        Ok(())
    }

    /// The regeneration (109): `object` heals by its slot 0xb8 ([`World::gain_by_slot`]) the
    /// minute's hit points (`DAT_14330ebc8`, [`REGENERATION_MINUTE`]) over the building's own
    /// regeneration stinger (`+0xb3c`, slot 0x750) plus its powerup (`+0xa34`, slot 0x6f0) plus
    /// `amount` (`ADDSS` in that order), times the world's dt (`world+0x164`). Then, of type 70
    /// and up and at least at its master's hit points (the master it now has; `COMISS`, `JC`: a
    /// NaN is not), it gets back the armor its charge's hits took (`FUN_1412066a0`: `+0x9e4`
    /// zeroed), and above them (`JBE`) it comes down to them. A ghost's hit points (its static
    /// part's `+0x278`) are not held: its heal changes nothing the world holds.
    ///
    /// # Errors
    ///
    /// The heal's, an object the heal's transform moved off the lists, and a master the tables
    /// lack.
    fn regenerate_by(
        &mut self,
        building: &SpeedupBuilding,
        amount: Regeneration,
        object: &TileObject,
    ) -> GenieResult<()> {
        let Some(place) = object.place else {
            return Ok(());
        };
        let own = self.parts.combat_of(building.id);
        let rate = own.regen_stinger.get() + (own.regen_powerup.get() + amount.get());
        let heal = HitPoints::new(REGENERATION_MINUTE / rate * self.dt_seconds.get());
        self.gain_by_slot(object, place, heal)?;
        let place = self.place(object.id).ok_or_else(|| {
            GenieError::unreachable("a healed object left the lists").with("object", object.id)
        })?;
        let unit = self.players.at(place)?;
        let most = HitPoints::of_master(
            unit.master(self.master_tables(), place.player())?
                .hit_points,
        );
        if MasterKind::COMBAT <= object.kind && most <= unit.life.hp {
            self.parts.combat_mut(object.id)?.armor_removed = ArmorPair::ZERO;
        }
        let unit = self.players.at_mut(place)?;
        if most < unit.life.hp {
            unit.life.hp = most;
        }
        Ok(())
    }

    /// Slot 0xb8 of `object` at `place` by `heal`: below type 70 its hit points add it
    /// (`FUN_141188ca0`); a combat object's ([`gain_hp`], `FUN_141203be0`) then runs its hit
    /// point transforms' test at once ([`World::transform_by_hit_points`]).
    ///
    /// # Errors
    ///
    /// A combat object asleep (only static objects sleep), a master the tables lack, and the
    /// transforms'.
    fn gain_by_slot(
        &mut self,
        object: &TileObject,
        place: Place,
        heal: HitPoints,
    ) -> GenieResult<()> {
        if object.kind < MasterKind::COMBAT {
            self.players.at_mut(place)?.life.hp += heal;
            return Ok(());
        }
        let Place::Live(player, list_index) = place else {
            // RGE_Static_Object::update flags only static objects to sleep
            return Err(GenieError::unreachable("a combat object asleep").with("object", object.id));
        };
        let master_hp = self
            .players
            .live(player, list_index)?
            .master(self.master_tables(), player)?
            .hit_points;
        let unit = self.players.live_mut(player, list_index)?;
        let combat = self.parts.combat_mut(object.id)?;
        let test = gain_hp(
            &mut unit.life.hp,
            &mut combat.hp_powerup,
            master_hp,
            combat.hp_stinger,
            heal,
        );
        if test == TransformTest::Runs {
            self.transform_by_hit_points(player, list_index)?;
        }
        Ok(())
    }
}
