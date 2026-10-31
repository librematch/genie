//! An ownership change (slot 0x70, `FUN_14120f800`): to Gaia, a building's with its
//! foundation, annexes and garrison, and the attributes and storage it moves.

use crate::GenieError;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lifecycle::death_parts::count_class;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::ids::PASTURE_1889;
use crate::objects::masters::ids::PASTURE_MANGROVE;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::{Master, Masters, player_master};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::unit_ai::NotifyKind;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlots;

use crate::world::World;
use crate::world::player::Player;

impl World {
    /// Slot 0x70 ([`change_hands`]) to Gaia of the object at `j` of player `q`, from outside its
    /// update: once Gaia has its master it leaves `q`'s list (`FUN_141630930`, the last object
    /// in its place) for the end of Gaia's (`FUN_14167f670` -> `FUN_141616200`) with its
    /// attributes ([`World::hand_over_attributes`]). Its only callers give livestock, which
    /// Gaia's add does not take into its list of class 10 of combat level 4 (`+0x14a0`, a draw
    /// on the Player stream).
    pub(crate) fn give_gaia(&mut self, q: PlayerId, j: ListIndex) -> Result<(), GenieError> {
        let mut unit = self.players.player_mut(q)?.objects.take_out(j)?;
        let changed = change_hands(
            &mut unit,
            (q.index(), PlayerId::GAIA),
            &self.masters,
            &mut self.spatial,
            self.world_time,
        );
        let j = self.put_back_own(q, unit)?;
        if !changed? {
            return Ok(());
        }
        let unit = self.players.player_mut(q)?.objects.swap_remove(j)?;
        let handed = self.hand_over_attributes(q.index(), 0, &unit);
        // the old owner's remove and Gaia's add (slots 0x2d8, 0x2d0) move it between the
        // trackers; a building whose build points reach its build time completes in Gaia's
        // (`FUN_1411cd860`, `COMISS`/`JC`: a NaN does not)
        let (id, master_id) = (unit.id, unit.life.master_id);
        let built = self
            .masters
            .first()
            .and_then(|m| m.get(master_id))
            .is_some_and(|m| m.kind == BUILDING)
            && unit
                .construction
                .is_some_and(|c| Seconds::of_master(c.build_time) <= c.progress);
        self.players[0].objects.push(unit);
        self.track_remove(q, id, master_id)?;
        self.track_add(PlayerId::GAIA, id, master_id)?;
        if built {
            self.track_complete(PlayerId::GAIA, id, master_id)?;
        }
        handed
    }

    /// A building's slot 0x70 (`FUN_1411cd860`) from player `old` to `new`, of the building `id`
    /// in `old`'s live list (from its own update, after it: [`Outcome::Captured`]'s `building`). In
    /// order:
    ///
    /// - its annexes (`+0xbc0`..), each a building: one its old owner watches leaves the watched
    ///   list (`FUN_141091310`, `+0xca4` 0), then the annex's own slot 0x70, depth first;
    /// - below state 3 its record and mesh shape go (`FUN_141055b40`);
    /// - in state 2 the old owner loses its master's storage of flag 4 and a held attribute 4
    ///   (slot 0xf8, the amount's sign flipped, `DAT_142e88e80`);
    /// - a foundation's (state 0) footprint leaves the old owner's map (slot 0x1d8);
    /// - its action list and production list go (`FUN_140a98620`, [`World::clear_actions`],
    ///   [`World::end_production_action`]), then its whole queue ([`World::cancel_queue`]);
    /// - a watched one leaves the old owner's watched list;
    /// - the combat object's slot 0x70 (`FUN_14120f800`, [`change_hands`]; the lists' moves and the
    ///   attributes as [`World::give_gaia`] does them), unless the new owner lacks its master;
    /// - built (its build time at most its build points: `COMISS`, a NaN is not), it completes in
    ///   the new owner's tracker (`FUN_14104eac0`);
    /// - a watched one joins the new owner's watched list at its tile (`FUN_141090430`);
    /// - a foundation's footprint comes on the new owner's map (slot 0x1d0);
    /// - in state 2 its master's stack unit (`+0x986`; 1889's on rice terrain 1893) comes at its
    ///   place (the master's slot 0x40, [`World::remains`]) and its master's technology is
    ///   researched ([`World::research_built`]); then its owner gains the storage of flags 4 and 8
    ///   and a held attribute 4;
    /// - the units inside leave it toward no point, from the last ([`World::let_out`] with −1, −1,
    ///   `DAT_14330ec20`); one that finds no spot leaves and is destroyed (slots 0x1a8, 0xd0);
    /// - below state 3 its record and mesh shape come again (`FUN_141056460`);
    /// - its gather point goes (`+0xc30`..`+0xc44`).
    ///
    /// Not held: the head unit's sprite (`+0xc90`, the drawing's), the semi-asleep lists
    /// (`+0xc98`, `FUN_14107db90`, `FUN_14107bc80`) and a computer player's AI memory of it
    /// (`+0x7a8`, `FUN_14160b900`; a playback runs no AI).
    ///
    /// # Errors
    ///
    /// A building asleep ([`crate::ownership::capture::HANDS_LEFT`]), and what the parts do not model.
    pub(crate) fn building_changes_hands(
        &mut self,
        old: PlayerId,
        id: ObjectId,
        new: PlayerId,
        watched: bool,
    ) -> Result<(), GenieError> {
        self.annexes_change_hands(old, id, new)?;
        let Some(crate::orders::Place::Live(_, idx)) = self.place(id) else {
            return Err(crate::ownership::capture::HANDS_LEFT);
        };
        let u = &self.players.player(old)?.objects[idx];
        let master = match u.own_master.as_deref() {
            Some(m) => m.clone(),
            None => u.master(&self.masters, old)?.clone(),
        };
        let (state, held, amount) = (u.life.state, u.life.attribute_type, u.life.amount);
        let built = u
            .construction
            .is_some_and(|c| Seconds::of_master(c.build_time) <= c.progress);
        let rice = u.rice;
        let placed = self.spatial.get(id).copied();
        if state < ObjectState::Dying {
            self.unrecord(id);
        }
        if state == ObjectState::Alive {
            lose_built_storage(self.players.player_mut(old)?, &master, held, amount.get())?;
        }
        if state == ObjectState::Foundation {
            self.foundation_sight_off(id, placed)?;
        }
        self.clear_for_hand_over(old, (idx, id), watched)?;
        let owner = self.hand_building_over(old, idx, new, id)?;
        if built {
            self.track_complete(new, id, master.id)?;
        }
        if watched {
            self.watch_at_place(new, id, placed)?;
        }
        if state == ObjectState::Foundation {
            self.foundation_sight_on(id, placed)?;
        }
        if state == ObjectState::Alive {
            self.built_with_new_owner(owner, id, &master, rice)?;
            gain_built_storage(self.players.player_mut(owner)?, &master, held, amount.get())?;
        }
        self.garrison_leaves(owner, id)?;
        if state < ObjectState::Dying {
            self.record_again(id, &master);
        }
        if let Some((_, u)) = self.object_mut(id) {
            u.gather_point = None;
        }
        Ok(())
    }

    /// A building that changes hands leaves the spatial records and the planners' tiles.
    fn unrecord(&mut self, id: ObjectId) {
        self.spatial.drop_record(id);
        self.pathing.removed(id);
        self.pathing.tiles_changed(self.spatial.take_changes());
    }

    /// The new owner watches the building `id`'s tile (the building's place, when it has one).
    fn watch_at_place(
        &mut self,
        new: PlayerId,
        id: ObjectId,
        placed: Option<crate::map::spatial::Placed>,
    ) -> Result<(), GenieError> {
        if let Some(p) = placed {
            let _ = self.watch(new, id, p.position.tile())?;
        }
        Ok(())
    }

    /// [`World::building_changes_hands`]'s end of the building `id`'s work at `idx` of its old
    /// owner `old`: its actions, its production action and its queue, and its watched entry.
    fn clear_for_hand_over(
        &mut self,
        old: PlayerId,
        (idx, id): (ListIndex, ObjectId),
        watched: bool,
    ) -> Result<(), GenieError> {
        self.clear_actions(old, idx)?;
        self.end_production_action(old, idx)?;
        self.cancel_queue(old, idx)?;
        if watched {
            self.unwatch(old, id)?;
        }
        Ok(())
    }

    /// [`World::building_changes_hands`]'s foundation (state 0) `placed` leaving its old owner's
    /// map (slot 0x1d8).
    pub(crate) fn foundation_sight_off(
        &mut self,
        id: ObjectId,
        placed: Option<crate::map::spatial::Placed>,
    ) -> Result<(), GenieError> {
        if let (Some(p), Some(v)) = (placed, self.spatial.sight.as_mut()) {
            v.sight_off(id, p.position, p.radius, true, false)?;
        }
        Ok(())
    }

    /// [`World::building_changes_hands`]'s foundation (state 0) `placed` coming on its new
    /// owner's map (slot 0x1d0).
    pub(crate) fn foundation_sight_on(
        &mut self,
        id: ObjectId,
        placed: Option<crate::map::spatial::Placed>,
    ) -> Result<(), GenieError> {
        if let (Some(p), Some(v)) = (placed, self.spatial.sight.as_mut()) {
            let _ = v.sight_on(id, p.position, p.radius, true, false)?;
        }
        Ok(())
    }

    /// [`World::building_changes_hands`] below state 3: the building's record and mesh shape of
    /// `master`'s obstruction come again (`FUN_141056460`).
    pub(crate) fn record_again(&mut self, id: ObjectId, master: &Master) {
        let record = crate::map::spatial::Obstruction {
            kind: master.obstruction_type,
            class: master.obstruction_class,
        };
        if record.recorded() {
            self.spatial.add_record(id, record);
            if let Some(p) = self.spatial.get(id).copied() {
                self.pathing.place_shape(id, &p);
            }
            self.pathing.tiles_changed(self.spatial.take_changes());
        }
    }

    /// [`World::building_changes_hands`]'s annexes of `id` of a building type first, each leaving
    /// the old owner's watched list and changing hands (depth first).
    ///
    /// # Errors
    ///
    /// An annex of a master the slice does not have, and what its change does not model.
    pub(crate) fn annexes_change_hands(
        &mut self,
        old: PlayerId,
        id: ObjectId,
        new: PlayerId,
    ) -> Result<(), GenieError> {
        for annex in self.spatial.annexes_of(id) {
            let Some((q, a)) = self.object(annex) else {
                continue;
            };
            if a.master(&self.masters, q)?.kind != BUILDING {
                continue;
            }
            let annex_watched = self
                .players
                .player(old)?
                .watched
                .iter()
                .any(|w| w.id == annex);
            if annex_watched {
                self.unwatch(old, annex)?;
            }
            self.building_changes_hands(q, annex, new, false)?;
        }
        Ok(())
    }

    /// [`World::building_changes_hands`]'s combat object's slot 0x70 (`FUN_14120f800`,
    /// [`change_hands`]) on building `id` (player `old`'s `idx`): when it changes, it moves to
    /// `new`'s objects with its attributes ([`World::hand_over_attributes`]) and trackers; the
    /// owner it then has.
    ///
    /// # Errors
    ///
    /// What the change and the attributes do not model.
    pub(crate) fn hand_building_over(
        &mut self,
        old: PlayerId,
        idx: ListIndex,
        new: PlayerId,
        id: ObjectId,
    ) -> Result<PlayerId, GenieError> {
        let mut unit = self.players.player_mut(old)?.objects.take_out(idx)?;
        let changed = change_hands(
            &mut unit,
            (old.index(), new),
            &self.masters,
            &mut self.spatial,
            self.world_time,
        );
        let idx = self.put_back_own(old, unit)?;
        Ok(if changed? {
            let unit = self.players.player_mut(old)?.objects.swap_remove(idx)?;
            let handed = self.hand_over_attributes(old.index(), new.index(), &unit);
            let master_id = unit.life.master_id;
            self.players.player_mut(new)?.objects.push(unit);
            self.track_remove(old, id, master_id)?;
            self.track_add(new, id, master_id)?;
            handed?;
            new
        } else {
            old
        })
    }

    /// [`World::building_changes_hands`] of a building in state 2 with its `owner`: its master's
    /// stack unit (`+0x986`; 1889's on rice terrain 1893) comes at its place (the master's slot
    /// 0x40, [`World::remains`]) and its master's technology is researched
    /// ([`World::research_built`]).
    ///
    /// # Errors
    ///
    /// What the stack unit and the research do not model.
    pub(crate) fn built_with_new_owner(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        master: &Master,
        rice: bool,
    ) -> Result<(), GenieError> {
        let building = master.building.as_ref();
        if let Some(stack) = building.and_then(|b| b.stack_unit) {
            let stack = if master.id == PASTURE_1889 && rice {
                PASTURE_MANGROVE
            } else {
                stack
            };
            let at = self
                .object(id)
                .map_or(Position::ORIGIN, |(_, u)| u.position());
            self.remains(owner, stack, at, None)?;
        }
        if let Some(tech) = building.and_then(|b| b.tech) {
            self.research_built(owner, tech)?;
        }
        Ok(())
    }

    /// [`World::building_changes_hands`]'s units inside building `id` leave it toward no point,
    /// from the last ([`World::let_out`] with −1, −1, `DAT_14330ec20`); one that finds no spot
    /// leaves and is destroyed (slots 0x1a8, 0xd0).
    ///
    /// # Errors
    ///
    /// What the leaving and the destruction do not model.
    pub(crate) fn garrison_leaves(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
    ) -> Result<(), GenieError> {
        let inside = self
            .object(id)
            .map_or_else(Vec::new, |(_, u)| u.garrison.clone());
        let mut stayed = Vec::new();
        for &g in inside.iter().rev() {
            let Some(crate::orders::Place::Live(_, bi)) = self.place(id) else {
                break;
            };
            if !self.let_out(
                owner,
                bi,
                g,
                Point::from([HANDOVER_NO_POINT; 2]),
                (None, true),
            )? {
                stayed.push(g);
            }
        }
        for g in stayed {
            if let Some((q, _)) = self.object(g) {
                self.destroy(q, g)?;
            }
        }
        Ok(())
    }

    /// The attributes an ownership change ([`change_hands`]) moves with object `u` from player
    /// `old` to `new`, by its master of its own, in the order of `FUN_14120f800`: the storage
    /// the old owner loses (`FUN_1411a56c0` with 1: flags 2, 16 and 32, the amount's sign
    /// flipped), its class counters ([`count_class`], −1 for the old owner, then 1 for the new)
    /// and the storage the new owner gains (`FUN_141193860`: flags 1, 2 and 32). The lists'
    /// moves come between the first two, which nothing here reads.
    pub(crate) fn hand_over_attributes(
        &mut self,
        old: usize,
        new: usize,
        u: &Unit,
    ) -> Result<(), GenieError> {
        let Some(m) = u.own_master.as_deref() else {
            return Ok(());
        };
        for &(kind, amount, flag) in &m.storage {
            if let Some(kind) = kind
                && matches!(flag, 2 | 16 | 32)
            {
                self.players[old].add_attribute(kind, -amount)?;
            }
        }
        count_class(&mut self.players[old].attributes, m, -1.0);
        count_class(&mut self.players[new].attributes, m, 1.0);
        for &(kind, amount, flag) in &m.storage {
            if let Some(kind) = kind
                && matches!(flag, 1 | 2 | 32)
            {
                self.players[new].add_attribute(kind, amount)?;
            }
        }
        Ok(())
    }
}

/// The master types whose death-spawn master an ownership change copies (`FUN_14120f800`'s
/// switch on the master's `+0x8`): static to moving objects, combat units and buildings.
pub(crate) const DEAD_COPY_TYPES: [MasterKind; 6] = [
    master_type::STATIC,
    master_type::ANIMATED,
    master_type::MOVING,
    master_type::ACTION,
    master_type::COMBAT,
    BUILDING,
];

/// `FUN_14120f800`, slot 0x70 of a combat object (`TRIBE_Combat_Object`'s): unit `u` of player
/// `old` goes to player `new` (`masters`, every player's). First its records take the new owner
/// ([`Spatial::records_owner`]); without its master in the new owner's table (`player+0x28`)
/// nothing else changes and the answer is false. Then, in order: its sight goes from the old
/// owner's map (slot 0x1d8) when it sees (state 2, not inside another); its master's death
/// spawn (`+0x5e4`) in the old owner's table becomes its dead master ([`Unit::dead_master`], a
/// copy of type 10 to 40, 70 or 80) unless it has one; its owner
/// changes; it takes a master of its own (`FUN_141186d10`: a copy of the master it reads unless
/// it has one, [`Unit::own_master`]) with the new owner's master's `fog_visibility` (`+0x605`)
/// and, for livestock (class 58), line of sight (`+0x5d0`); its sight comes to the new owner's
/// map with that line of sight (slot 0x1d0); its unit AI's notifies go with `world_time`
/// (`FUN_140c5dc10`) and it tells its unit AI it changed hands (slot 0x328, 0x20b with the new
/// owner). The caller's: the storage (`FUN_1411a56c0` with 1 for the old owner,
/// `FUN_141193860` for the new), the lists (the players' slots 0x2d8 and 0x2d0) and the class
/// counters ([`count_class`], −1 then 1), from the master of its own, and the units a siege
/// weapon (class 13) carries. Not simulation: the names (`FUN_141189810`, `+0x6a8`, `+0x758`,
/// `+0x770`), the old owner's selection (`FUN_141677380` for `+0x30c` bit 0), the log line and
/// the message of game mode 12 for master 1377; nor are the slot 0xa8 calls on the copies (the
/// masters' graphics).
pub(crate) fn change_hands(
    u: &mut Unit,
    (old, new): (usize, PlayerId),
    masters: &[Masters],
    spatial: &mut Spatial,
    world_time: WorldTime,
) -> Result<bool, GenieError> {
    spatial.records_owner(u.id, new)?;
    let Some(to) = player_master(masters, new, u.life.master_id) else {
        return Ok(false);
    };
    let old_table = masters.get(old);
    let mut own = match u.own_master.take() {
        Some(own) => own,
        None => Box::new(
            old_table
                .and_then(|m| m.get(u.life.master_id))
                .ok_or(GenieError::unsupported(
                    "an ownership change of an object whose master the slice does not have",
                ))?
                .clone(),
        ),
    };
    // the dead master comes before the copy (FUN_141186d10), from the master it reads
    if u.dead_master.is_none() && own.death_spawns[0].is_some() {
        u.dead_master = old_table
            .and_then(|m| own.death_spawns[0].and_then(|id| m.get(id)))
            .filter(|d| DEAD_COPY_TYPES.contains(&d.kind))
            .map(|d| Box::new(d.clone()));
    }
    own.fog_visibility = to.fog_visibility;
    if own.class == class::LIVESTOCK {
        own.line_of_sight = to.line_of_sight;
    }
    let los = own.line_of_sight;
    u.own_master = Some(own);
    let sees = u.life.state == ObjectState::Alive && u.life.inside.is_none();
    spatial.change_owner(u.id, new, los, sees)?;
    if let Some(a) = u.actions.artifact_mut() {
        a.gaia = new == PlayerId::GAIA;
    }
    if let Some(ai) = u.ai.as_mut() {
        ai.clear_notifies(world_time.since_start())?;
    }
    u.post_own(NotifyKind::OWNER_CHANGED, [new.index() as i32, 0, 0])?;
    Ok(true)
}

/// [`World::building_changes_hands`] of a building in state 2: its old owner `p` loses its
/// master's storage of flag 4 and a `held` attribute 4 (slot 0xf8, the `amount`'s sign
/// flipped, `DAT_142e88e80`).
///
/// # Errors
///
/// An attribute the player does not hold.
pub(crate) fn lose_built_storage(
    p: &mut Player,
    master: &Master,
    held: Option<AttributeId>,
    amount: f32,
) -> Result<(), GenieError> {
    for &(kind, amount, flag) in &master.storage {
        if let Some(kind) = kind
            && flag == STORAGE_WHILE_BUILT
        {
            p.add_attribute(kind, -amount)?;
        }
    }
    // The population headroom attribute, which a building's held amount gives (`+0x308` 4).
    if held == Some(attribute::HEADROOM) {
        p.add_attribute(attribute::HEADROOM, Amount::new(-amount))?;
    }
    Ok(())
}

/// [`World::building_changes_hands`] of a building in state 2: its new owner `p` gains its
/// master's storage of flags 4 and 8 and a `held` attribute 4 (its `amount`).
///
/// # Errors
///
/// An attribute the player does not hold.
pub(crate) fn gain_built_storage(
    p: &mut Player,
    master: &Master,
    held: Option<AttributeId>,
    amount: f32,
) -> Result<(), GenieError> {
    for &(kind, amount, flag) in &master.storage {
        if let Some(kind) = kind
            && matches!(flag, STORAGE_WHILE_BUILT | STORAGE_8)
        {
            p.add_attribute(kind, amount)?;
        }
    }
    if held == Some(attribute::HEADROOM) {
        p.add_attribute(attribute::HEADROOM, Amount::new(amount))?;
    }
    Ok(())
}

/// A building's storage flag that its owner holds while it stands built (slot 0x70's).
pub(crate) const STORAGE_WHILE_BUILT: u8 = 4;
/// The other storage flag a building's new owner gains on an ownership change (slot 0x70's).
pub(crate) const STORAGE_8: u8 = 8;
/// The point a building's units leave toward on an ownership change: none (`DAT_14330ec20`).
pub(crate) const HANDOVER_NO_POINT: f32 = crate::math::image::turn::HANDOVER_NO_POINT;
