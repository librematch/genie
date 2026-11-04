//! A hit's victim: what the hit reads of it, the objects it may reach, the hit written back,
//! the kill and its counts.

use crate::combat::damage::{Kill, Razed, Victim};
use crate::map::Distance;
use crate::map::Rect;
use crate::map::Tile;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::Notify;
use crate::unit_ai::task_finder::world::{SideWorld, TaskWorld};
use crate::world::World;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

pub(crate) use crate::combat::victim_reads::{victim_in, victim_of};
use crate::construction::MainBuildingId;
use crate::construction::build::NOT_HELD;
use crate::map::spatial::TileSight;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::at_most_or_unordered;
use crate::objects::layers::Action;
use crate::objects::masters::cost_entries;
use crate::unit_ai::{NotifyKind, Seen};
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use std::mem::take;

impl World {
    /// What a hit of an attacker of player `attacker` reads of the object `id` ([`Victim`]): an
    /// object the slice holds, live or asleep (the world's table holds both, and a blast walks
    /// the tiles' lists of both), with its relations to the attacker's player and whether its
    /// place is a guess.
    pub(crate) fn victim(
        &self,
        id: impl ObjectKey,
        attacker: PlayerId,
    ) -> GenieResult<Option<Victim>> {
        let Some((player_id, unit, _)) = self.locate(id) else {
            return Ok(None);
        };
        let master = unit.master(self.master_tables(), player_id)?;
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.model.drop_sites,
        };
        Ok(Some(victim_in(
            (
                unit,
                self.parts.unit_ai.contains(unit.id),
                self.parts.building_of(unit.id),
                self.parts.combat_of(unit.id),
                self.parts.actions.of(unit.id),
                self.parts.unread.get(unit.id),
            ),
            (
                player_id,
                master,
                &self.players.player(player_id)?.attributes,
            ),
            (attacker, &world),
            &self.spatial,
            self.mode_680,
            &self.model.roles,
        )))
    }

    /// The object `id` as the unit AI of a unit of player `viewer` finds it in the world's object
    /// table (`FUN_1415417a0`, [`crate::unit_ai::Seen`]), `None` when no list holds it. An object
    /// the slice holds as a place alone cannot be read. Without the players' sight its tile
    /// counts as unseen.
    pub(crate) fn seen(&self, id: impl ObjectKey, viewer: PlayerId) -> GenieResult<Option<Seen>> {
        if let Some((player_id, unit, _)) = self.locate(id) {
            return self.seen_of(unit, player_id, viewer).map(Some);
        }
        match self.spatial.get(id) {
            Some(_) => Err(GenieError::unsupported(
                "a unit AI's target the slice cannot read",
            )),
            None => Ok(None),
        }
    }

    /// Player `player_id`'s object `unit` as the unit AI of a unit of player `viewer` reads it
    /// ([`World::seen`]), wherever it is held (a unit out of its slot for its step too).
    pub(crate) fn seen_of(
        &self,
        unit: &Unit,
        player_id: PlayerId,
        viewer: PlayerId,
    ) -> GenieResult<Seen> {
        let master = unit.master(self.master_tables(), player_id)?;
        let (x, y) = (unit.position().x(), unit.position().y());
        Ok(Seen {
            state: unit.life.state,
            kind: master.kind,
            owner: player_id,
            class: master.class,
            position: unit.position(),
            radius: master.radius,
            visible: self.spatial.visible(viewer, Tile::new(x as i32, y as i32)),
            flags: master.object_flags,
            sight: self
                .spatial
                .tile_sight(viewer, Tile::new(cvttss2si(x), cvttss2si(y)))
                .unwrap_or(TileSight::Unknown),
        })
    }

    /// The objects besides `unit` (player `player_id`) and its `target` that its hit may reach: for
    /// a master with an area effect, the objects on the tiles of the blast's rectangle about the
    /// unit, a tile wider each way (the unit may step in its update), each with its place in its
    /// tile's list as the unit's update starts (an object the slice cannot read stands in with its
    /// place alone, as a guess); then the main buildings (`+0xbb8`) of the target and of the
    /// annexes among them, which the hits on those annexes go to.
    pub(crate) fn blasted(
        &self,
        unit: &Unit,
        player_id: PlayerId,
        target: Option<ObjectId>,
    ) -> GenieResult<Vec<Victim>> {
        let reach = Some(
            unit.master(self.master_tables(), player_id)?
                .combat_part()
                .area_effect,
        )
        .filter(|&r| Distance::ZERO < r)
        .map(|distance| distance + Distance::new(6.0) + Distance::new(2.0));
        let mut near: Vec<Victim> = Vec::new();
        if let Some(reach) = reach {
            let (x, y) = (unit.position().x(), unit.position().y());
            for (id, placed) in self
                .spatial
                .objects_within(Rect::new(
                    x - reach.get(),
                    y - reach.get(),
                    x + reach.get(),
                    y + reach.get(),
                ))
                .filter(|&(id, _)| id != unit.id && Some(id) != target)
            {
                let seen = self.victim(id, player_id)?;
                near.push(seen.unwrap_or_else(|| Victim {
                    id,
                    position: placed.position,
                    uncertain: true,
                    tile_place: self.spatial.tile_place(id),
                    ..Victim::default()
                }));
            }
        }
        let mains: Vec<ObjectId> = target
            .into_iter()
            .chain(near.iter().map(|v| v.id))
            .filter_map(|id| self.objects().annex_of(id).map(MainBuildingId::id))
            .collect();
        for object_id in mains {
            if object_id != unit.id
                && Some(object_id) != target
                && !near.iter().any(|v| v.id == object_id)
                && let Some(victim) = self.victim(object_id, player_id)?
            {
                near.push(victim);
            }
        }
        Ok(near)
    }
}

/// What a hit's write-back ([`write_back`]) leaves for the world ([`World::kill_left`], at the
/// hit through [`file_kill`]), in the order the hits made it: the object's kill and its razings.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KillLeft {
    /// The object.
    pub(crate) id: ObjectId,
    /// A hit killed it ([`World::killed`]).
    pub(crate) kill: Option<Kill>,
    /// What each hit that left it, a building, below 1 hit point counted.
    pub(crate) razed: Vec<Razed>,
}

impl KillLeft {
    /// The updating unit `id`, killed by its target's retaliation (`kill`).
    pub(crate) const fn own(id: ObjectId, kill: Kill) -> Self {
        Self {
            id,
            kill: Some(kill),
            razed: Vec::new(),
        }
    }
}

/// The object `id` a hit of the updating unit can read ([`victim`]) with its owner and master:
/// one of any list, live or asleep, of a master its owner has.
pub(crate) fn readable<'o>(
    step: &'o Step<'_>,
    id: impl ObjectKey,
) -> GenieResult<Option<(PlayerId, &'o Unit, &'o Master)>> {
    let Some((player_id, unit, _)) = step.w.locate(id) else {
        return Ok(None);
    };
    let master = unit.master(step.w.master_tables(), player_id)?;
    Ok(Some((player_id, unit, master)))
}

/// What a hit of the updating unit reads of object `id` ([`World::victim`] from the update):
/// an object [`readable`] by the hit.
pub(crate) fn victim(
    step: &Step<'_>,
    id: impl ObjectKey,
    roles: &Roles,
) -> GenieResult<Option<Victim>> {
    let Some((player_id, unit, master)) = readable(step, id)? else {
        return Ok(None);
    };
    let view = OthersRef::of(&step.w.players, &step.w.index, step.player)?;
    let attributes = if player_id == step.player {
        &step.w.players.player(step.player)?.attributes[..]
    } else {
        let Some(owner) = view.players().player(player_id) else {
            return Ok(None);
        };
        &owner.attributes[..]
    };
    let sides = SideWorld {
        player: step.player,
        relations: &step.w.players.player(step.player)?.relations,
        foreign: view.players(),
        drop_sites: &step.w.model.drop_sites,
    };
    Ok(Some(victim_in(
        (
            unit,
            step.w.parts.unit_ai.contains(unit.id),
            step.w.parts.building_of(unit.id),
            step.w.parts.combat_of(unit.id),
            step.w.parts.actions.of(unit.id),
            step.w.parts.unread.get(unit.id),
        ),
        (player_id, master, attributes),
        (step.player, &sides),
        &step.w.spatial,
        step.w.mode_680,
        roles,
    )))
}

/// The hit's changes on its victim `victim`, written back to the world where the hit ran. An object
/// asleep (a tree a blast reached) takes its hit points and what it holds; below 1 hit point
/// (COMISS/JBE: a NaN sleeps on) the hit wakes it (`FUN_14119deb0` with 0, [`World::wake_sleeper`])
/// to take the rest. A live one takes its hit points, what it holds, its sleep flag, its decay, its
/// powerups and charge, a struck building's `damaged_lately_timer`, the damage notifies (500) to
/// its unit AI, and a struck building's annexes take its hit points (`+0xbc0`..). Returns its kill
/// and razings for the caller ([`KillLeft`]); `victim` keeps neither them nor its notifies, so
/// another write-back repeats none.
///
/// # Errors
///
/// A notify its unit AI's queue does not take.
pub(crate) fn write_back(
    step: &mut Step<'_>,
    victim: &mut Victim,
) -> GenieResult<Option<KillLeft>> {
    let damaged = take(&mut victim.damaged);
    let left = KillLeft {
        id: victim.id,
        kill: victim.killed.take(),
        razed: victim
            .building
            .as_mut()
            .map(|b| take(&mut b.razed))
            .unwrap_or_default(),
    };
    let Some((_, _, sleeps)) = step.w.locate(victim.id) else {
        return Ok(None);
    };
    if sleeps {
        let (_, u) = step.w.object_mut(victim.id).ok_or(NOT_LOCATED)?;
        u.life.hp = victim.hp;
        u.life.amount = victim.amount;
        if at_most_or_unordered(1.0, victim.hp.get()) {
            return Ok(None);
        }
        let _ = step.w.wake_sleeper(victim.id)?;
    }
    let (_, unit) = step.w.object_mut(victim.id).ok_or(NOT_LOCATED)?;
    unit.life.hp = victim.hp;
    unit.life.amount = victim.amount;
    unit.life.goto_sleep = victim.goto_sleep;
    // a hunt that stopped its decay gave it a master of its own (FUN_140ac44b0)
    if let Some(master_data) = unit.life.master.as_mut() {
        master_data.decay = victim.decay;
    }
    let struck = victim.building.as_ref().filter(|b| b.struck);
    if let Some(b) = struck {
        unit.damaged_lately = b.damaged_lately;
    }
    let state = unit.life.state;
    let combat = step.w.parts.combat_mut(victim.id)?;
    if let Some(armored) = victim.armored.as_ref() {
        combat.hp_powerup = armored.hp_powerup;
    }
    // a shield spends its charge
    combat.charge = victim.source.charge;
    // the damage notify (500) to its unit AI
    if let Some(ai) = step.w.parts.unit_ai.get_mut(victim.id) {
        for &(attacker, p) in &damaged {
            let notify = Notify {
                caller: attacker,
                recipient: victim.id,
                kind: NotifyKind::DAMAGED,
                p,
            };
            ai.post(notify, state)?;
        }
    }
    for &object_id in struck.map_or(&[][..], |building| &building.annexes) {
        if let Some((_, _, false)) = step.w.locate(object_id) {
            let (_, x) = step.w.object_mut(object_id).ok_or(NOT_LOCATED)?;
            x.life.hp = victim.hp;
        }
    }
    Ok((left.kill.is_some() || !left.razed.is_empty()).then_some(left))
}

/// A hit's kill and razings where the hit runs (the damage slot `FUN_1412108e0` kills and counts
/// at once: `FUN_141082e80`, slot 0x8b0): another object's reach the world
/// ([`World::kill_left`]); the updating object's own (`me`, out of its slot) counts as its
/// player's ([`World::count_killed`]), and a livestock's change to Gaia (slot 0x70) is the
/// unit's own at once ([`Unit::ai_change_owner`]: its list's move follows the update, as a
/// takeover's).
///
/// # Errors
///
/// What the kill does not model, or an attribute a player lacks.
pub(crate) fn file_kill(
    step: &mut Step<'_>,
    (me, actions): (&mut Unit, &mut Action),
    kill_left: KillLeft,
    roles: &Roles,
) -> GenieResult<()> {
    if kill_left.id != step.me {
        return step.w.kill_left(&kill_left);
    }
    if let Some(kill) = kill_left.kill {
        let master = me.master(step.w.master_tables(), step.player)?.clone();
        step.w.count_killed(step.player, &master, kill)?;
        if kill.to_gaia {
            me.ai_change_owner(actions, PlayerId::GAIA, step, roles)?;
        }
    }
    step.w.count_razed(&kill_left.razed)
}

/// A strike's write-back where it ran ([`write_back`]), each object's kill and razings right
/// after it ([`file_kill`], `me` the updating object): the blast's objects (`blasted`, which go
/// back to the update's) in their order, then its target `v`.
///
/// # Errors
///
/// A notify a unit AI's queue does not take, or what a kill does not model.
pub(crate) fn write_struck(
    step: &mut Step<'_>,
    me: &mut Unit,
    actions: &mut Action,
    mut blasted: Vec<Victim>,
    v: &mut Victim,
    roles: &Roles,
) -> GenieResult<()> {
    for victim in &mut blasted {
        if let Some(k) = write_back(step, victim)? {
            file_kill(step, (me, actions), k, roles)?;
        }
    }
    step.blasted = blasted;
    if let Some(kill_left) = write_back(step, v)? {
        file_kill(step, (me, actions), kill_left, roles)?;
    }
    Ok(())
}

/// A strike's write-back where it ran ([`write_struck`] on the update's `blasted`, then its
/// target `victim`; `me` the updating object), each kill filed after its object's.
///
/// # Errors
///
/// A notify a unit AI's queue does not take, or what a kill does not model.
pub(crate) fn write_strike_back(
    step: &mut Step<'_>,
    me: &mut Unit,
    actions: &mut Action,
    victim: &mut Victim,
    roles: &Roles,
) -> GenieResult<()> {
    let blasted = take(&mut step.blasted);
    write_struck(step, me, actions, blasted, victim, roles)
}

impl World {
    /// What a hit's write-back left ([`KillLeft`]) on an object the world holds live: its kill
    /// ([`World::killed`]), then its razings ([`World::count_razed`]).
    ///
    /// # Errors
    ///
    /// What the kill does not model, or an attribute a player lacks.
    pub(crate) fn kill_left(&mut self, left: &KillLeft) -> GenieResult<()> {
        let Some(Place::Live(player_id, list_index)) = self.place(left.id) else {
            return Ok(());
        };
        if let Some(kill) = left.kill {
            self.killed(player_id, list_index, kill)?;
        }
        self.count_razed(&left.razed)
    }

    /// Each razing a hit left ([`crate::combat::damage::Razed`]): the history event of the
    /// attacker's player, the attribute adds in their order.
    ///
    /// # Errors
    ///
    /// An attribute a player lacks.
    pub(crate) fn count_razed(&mut self, razed: &[Razed]) -> GenieResult<()> {
        for r in razed {
            if let Some(kind) = r.event {
                let now = self.world_time;
                self.players
                    .player_mut(r.attacker_player)?
                    .history
                    .add_event(kind, r.owner.index() as f32, 0.0, 0, now);
            }
            for &(player_id, attribute, amount) in &r.adds {
                self.players
                    .player_mut(player_id)?
                    .add_attribute(attribute, amount)?;
            }
        }
        Ok(())
    }

    /// A combat object's death by a hit (`FUN_141082e80`, `FUN_1412108e0`): its owner's computer
    /// AI is told (slot 0x2f0, not modelled); the killed handling (slot 0x8b0, `FUN_141205880` ->
    /// `FUN_1412005b0`): unless its class is one of the non-military ones the attacker's player
    /// counts a kill (attribute 20) and its owner a loss (154); the attacker's player counts it
    /// against the owner (300 + the owner) and the owner against the attacker (325 + the
    /// attacker's player); both count its paid cost (170 and 152, and 400 + the owner). A killed
    /// villager first counts for its owner (`FUN_141082e80`): an attacker of class 9 or 10 (an
    /// animal) in attribute 227, and by the attacker's player type a human's in 229, Gaia's in
    /// 226, a computer's in 228. Gaia takes a player's livestock (slot 0x70): its place and sight, its Artifact's
    /// wait, its unit AI's notifies (cleared, then 0x20b), its storage (the old owner loses flags 2,
    /// 16 and 32, Gaia gains 1, 2 and 32) and its list (the old owner's last object takes its
    /// place). Not held: the killer (`+0x690`, `+0x694`), `+0x954`, the statistics of the local
    /// player and the particles (`FUN_141158bb0`).
    pub(crate) fn killed(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        kill: Kill,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player_id, list_index)?;
        let master = unit.master(self.master_tables(), player_id)?.clone();
        self.count_killed(player_id, &master, kill)?;
        if kill.to_gaia {
            self.give_gaia(player_id, list_index)?;
        }
        Ok(())
    }

    /// [`World::killed`]'s counts of an object of player `player_id` and `master`: its owner's
    /// computer AI (not modelled), a villager's counts for its owner, then the kill's
    /// ([`World::count_kill_of`]).
    ///
    /// # Errors
    ///
    /// A computer player's object, and an attribute a player lacks.
    pub(crate) fn count_killed(
        &mut self,
        player_id: PlayerId,
        master: &Master,
        kill: Kill,
    ) -> GenieResult<()> {
        if self.players.player(player_id)?.kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's killed object (its AI's slot 0x2f0)",
            ));
        }
        let ap = kill.attacker_player;
        if self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::VILLAGER)
        {
            // the attacker's class as the hit saw it: the attacker may be out for its update
            if self
                .model
                .roles
                .of_class(kill.attacker_class)
                .intersects(ClassRoles::PREY | ClassRoles::PREDATOR)
            {
                self.players.player_mut(player_id)?.add_attribute(
                    self.model.slots.attribute.villager_killed_by_animal,
                    Amount::new(1.0),
                )?;
            }
            let by_kind = match self.players.player(ap)?.kind {
                PlayerKind::Human => Some(self.model.slots.attribute.villager_killed_by_kind_1),
                PlayerKind::Gaia => Some(self.model.slots.attribute.villager_killed_by_kind_2),
                PlayerKind::Computer => Some(self.model.slots.attribute.villager_killed_by_kind_3),
                PlayerKind::Phantom => None,
            };
            if let Some(kind) = by_kind {
                self.players
                    .player_mut(player_id)?
                    .add_attribute(kind, Amount::new(1.0))?;
            }
        }
        self.count_kill_of(player_id, master, ap)
    }

    /// `FUN_1412005b0` on the object `id` killed for player `killer` (a movement damage's,
    /// [`crate::combat::abilities`]): the counts of [`World::killed`] by its master (its own one, else
    /// its player's).
    ///
    /// # Errors
    ///
    /// An object the slice does not hold, and an attribute whose change the slice does not
    /// model.
    pub(crate) fn count_kill(&mut self, id: ObjectId, killer: PlayerId) -> GenieResult<()> {
        let (player_id, unit) = self.object(id).ok_or(NOT_HELD)?;
        let master = unit.master(self.master_tables(), player_id)?.clone();
        self.count_kill_of(player_id, &master, killer)
    }

    /// `FUN_1412005b0`'s counts of a kill of an object of player `player_id` and `master` for
    /// player `ap`: unless its class is one of the non-military ones (`FUN_141188b20`) `ap` counts
    /// a kill (attribute 20) and `player_id` a loss (154); `ap` counts it against `player_id`
    /// (300 + `player_id`) and `player_id` against `ap` (325 + `ap`); both count its paid cost (170
    /// and 152, and 400 + `player_id`).
    pub(crate) fn count_kill_of(
        &mut self,
        player_id: PlayerId,
        master: &Master,
        ap: PlayerId,
    ) -> GenieResult<()> {
        if !self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::PEACEFUL)
        {
            self.players
                .player_mut(ap)?
                .add_attribute(self.model.slots.attribute.kills, Amount::new(1.0))?;
            self.players
                .player_mut(player_id)?
                .add_attribute(self.model.slots.attribute.kills_other, Amount::new(1.0))?;
        }
        self.players.player_mut(ap)?.add_attribute(
            self.model
                .slots
                .attribute
                .kills_of
                .offset(player_id.index())?,
            Amount::new(1.0),
        )?;
        self.players.player_mut(player_id)?.add_attribute(
            self.model.slots.attribute.killed_by.offset(ap.index())?,
            Amount::new(1.0),
        )?;
        let cost = paid_cost(master);
        self.players
            .player_mut(ap)?
            .add_attribute(self.model.slots.attribute.kills_value, cost)?;
        self.players
            .player_mut(player_id)?
            .add_attribute(self.model.slots.attribute.kills_value_other, cost)?;
        self.players.player_mut(ap)?.add_attribute(
            self.model
                .slots
                .attribute
                .kills_value_of
                .offset(player_id.index())?,
            cost,
        )?;
        Ok(())
    }
}

/// `FUN_140fde380`: `master` is of type 70 and up, of class 51 or 54, with a weapon (`+0x810`
/// above 0): a trebuchet, packed or not, whose kill counts as a unit's.
pub(crate) fn is_trebuchet(master: &Master, roles: &Roles) -> bool {
    MasterKind::COMBAT <= master.kind
        && roles
            .of_class(master.class)
            .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
        && !master.combat_part().weapons.is_empty()
}

/// `master`'s paid cost (`+0x880`: each entry with a resource and the paid flag), summed as
/// floats in its order, as the kill counts read it (`FUN_1412005b0`, `FUN_1411cea90`).
pub(crate) fn paid_cost(master: &Master) -> Amount {
    let mut cost = Amount::ZERO;
    for (_, amount, paid) in cost_entries(&master.creatable_part().cost) {
        if paid {
            cost += Amount::new(amount.as_f32());
        }
    }
    cost
}

/// A victim the world located without its object: `World::locate` and `World::object_mut` read
/// the same index, and nothing between them removes an object.
const NOT_LOCATED: GenieError =
    GenieError::unreachable("a victim the world located without its object");
