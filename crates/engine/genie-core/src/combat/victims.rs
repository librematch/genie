//! A hit's victim: what the hit reads of it, the objects it may reach, the hit written back,
//! the kill and its counts.

use crate::GenieError;
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
use crate::players::PlayerType;
use crate::players::attribute;
use crate::unit_ai::Notify;
use crate::unit_ai::task_finder::world::{SideWorld, TaskWorld};
use crate::world::World;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

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
    ) -> Result<Option<Victim>, GenieError> {
        let Some((p, u, _)) = self.locate(id) else {
            return Ok(None);
        };
        let master = u.master(self.master_tables(), p)?;
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.model.drop_sites,
        };
        Ok(Some(victim_in(
            (
                u,
                self.parts.unit_ai.contains(u.id),
                self.parts.building_of(u.id),
                self.parts.combat_of(u.id),
                self.parts.actions.of(u.id),
                self.parts.unread.get(u.id),
            ),
            (p, master, &self.players.player(p)?.attributes),
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
    pub(crate) fn seen(
        &self,
        id: impl ObjectKey,
        viewer: PlayerId,
    ) -> Result<Option<Seen>, GenieError> {
        if let Some((p, u, _)) = self.locate(id) {
            return self.seen_of(u, p, viewer).map(Some);
        }
        match self.spatial.get(id) {
            Some(_) => Err(GenieError::unsupported(
                "a unit AI's target the slice cannot read",
            )),
            None => Ok(None),
        }
    }

    /// Player `p`'s object `u` as the unit AI of a unit of player `viewer` reads it
    /// ([`World::seen`]), wherever it is held (a unit out of its slot for its step too).
    pub(crate) fn seen_of(
        &self,
        u: &Unit,
        p: PlayerId,
        viewer: PlayerId,
    ) -> Result<Seen, GenieError> {
        let master = u.master(self.master_tables(), p)?;
        let (x, y) = (u.position().x(), u.position().y());
        Ok(Seen {
            state: u.life.state,
            kind: master.kind,
            owner: p,
            class: master.class,
            position: u.position(),
            radius: master.radius,
            visible: self.spatial.visible(viewer, Tile::new(x as i32, y as i32)),
            flags: master.object_flags,
            sight: self
                .spatial
                .tile_sight(viewer, Tile::new(cvttss2si(x), cvttss2si(y)))
                .unwrap_or(TileSight::UNKNOWN),
        })
    }

    /// The objects besides `unit` (player `p`) and its `target` that its hit may reach: for a
    /// master with an area effect, the objects on the tiles of the blast's rectangle about the
    /// unit, a tile wider each way (the unit may step in its update), each with its place in its
    /// tile's list as the unit's update starts (an object the slice cannot read stands in with
    /// its place alone, as a guess); then the main buildings (`+0xbb8`) of the target and of the
    /// annexes among them, which the hits on those annexes go to.
    pub(crate) fn blasted(
        &self,
        unit: &Unit,
        p: PlayerId,
        target: Option<ObjectId>,
    ) -> Result<Vec<Victim>, GenieError> {
        let reach = Some(unit.master(self.master_tables(), p)?.area_effect)
            .filter(|&r| Distance::ZERO < r)
            .map(|r| r + Distance::new(6.0) + Distance::new(2.0));
        let mut near: Vec<Victim> = Vec::new();
        if let Some(reach) = reach {
            let (x, y) = (unit.position().x(), unit.position().y());
            for (id, o) in self
                .spatial
                .objects_within(Rect::new(
                    x - reach.get(),
                    y - reach.get(),
                    x + reach.get(),
                    y + reach.get(),
                ))
                .filter(|&(id, _)| id != unit.id && Some(id) != target)
            {
                let seen = self.victim(id, p)?;
                near.push(seen.unwrap_or_else(|| Victim {
                    id,
                    position: o.position,
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
        for m in mains {
            if m != unit.id
                && Some(m) != target
                && !near.iter().any(|v| v.id == m)
                && let Some(v) = self.victim(m, p)?
            {
                near.push(v);
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
    c: &'o Step<'_>,
    id: impl ObjectKey,
) -> Result<Option<(PlayerId, &'o Unit, &'o Master)>, GenieError> {
    let Some((p, u, _)) = c.w.locate(id) else {
        return Ok(None);
    };
    let master = u.master(c.w.master_tables(), p)?;
    Ok(Some((p, u, master)))
}

/// What a hit of the updating unit reads of object `id` ([`World::victim`] from the update):
/// an object [`readable`] by the hit.
pub(crate) fn victim(
    c: &Step<'_>,
    id: impl ObjectKey,
    roles: &Roles,
) -> Result<Option<Victim>, GenieError> {
    let Some((p, u, master)) = readable(c, id)? else {
        return Ok(None);
    };
    let view = OthersRef::of(&c.w.players, &c.w.index, c.player)?;
    let attributes = if p == c.player {
        &c.w.players.player(c.player)?.attributes[..]
    } else {
        let Some(owner) = view.players().player(p) else {
            return Ok(None);
        };
        &owner.attributes[..]
    };
    let sides = SideWorld {
        player: c.player,
        relations: &c.w.players.player(c.player)?.relations,
        foreign: view.players(),
        drop_sites: &c.w.model.drop_sites,
    };
    Ok(Some(victim_in(
        (
            u,
            c.w.parts.unit_ai.contains(u.id),
            c.w.parts.building_of(u.id),
            c.w.parts.combat_of(u.id),
            c.w.parts.actions.of(u.id),
            c.w.parts.unread.get(u.id),
        ),
        (p, master, attributes),
        (c.player, &sides),
        &c.w.spatial,
        c.w.mode_680,
        roles,
    )))
}

/// The hit's changes on its victim `v`, written back to the world where the hit ran. An object asleep (a tree a blast reached) takes its hit points and what it
/// holds; below 1 hit point (COMISS/JBE: a NaN sleeps on) the hit wakes it (`FUN_14119deb0`
/// with 0, [`World::wake_sleeper`]) to take the rest. A live one takes its hit points, what it holds,
/// its sleep flag, its decay, its powerups and charge, a struck building's
/// `damaged_lately_timer`, the damage notifies (500) to its unit AI, and a struck building's
/// annexes take its hit points (`+0xbc0`..). Returns its kill and razings for the caller
/// ([`KillLeft`]); `v` keeps neither them nor its notifies, so another write-back repeats none.
///
/// # Errors
///
/// A notify its unit AI's queue does not take.
pub(crate) fn write_back(c: &mut Step<'_>, v: &mut Victim) -> Result<Option<KillLeft>, GenieError> {
    let damaged = take(&mut v.damaged);
    let left = KillLeft {
        id: v.id,
        kill: v.killed.take(),
        razed: v
            .building
            .as_mut()
            .map(|b| take(&mut b.razed))
            .unwrap_or_default(),
    };
    let Some((_, _, sleeps)) = c.w.locate(v.id) else {
        return Ok(None);
    };
    if sleeps {
        let (_, u) = c.w.object_mut(v.id).ok_or(NOT_LOCATED)?;
        u.life.hp = v.hp;
        u.life.amount = v.amount;
        if at_most_or_unordered(1.0, v.hp.get()) {
            return Ok(None);
        }
        let _ = c.w.wake_sleeper(v.id)?;
    }
    let (_, u) = c.w.object_mut(v.id).ok_or(NOT_LOCATED)?;
    u.life.hp = v.hp;
    u.life.amount = v.amount;
    u.life.goto_sleep = v.goto_sleep;
    // a hunt that stopped its decay gave it a master of its own (FUN_140ac44b0)
    if let Some(m) = u.life.master.as_mut() {
        m.decay = v.decay;
    }
    let struck = v.building.as_ref().filter(|b| b.struck);
    if let Some(b) = struck {
        u.damaged_lately = b.damaged_lately;
    }
    let state = u.life.state;
    let combat = c.w.parts.combat_mut(v.id)?;
    if let Some(a) = v.armored.as_ref() {
        combat.hp_powerup = a.hp_powerup;
    }
    // a shield spends its charge
    combat.charge = v.source.charge;
    // the damage notify (500) to its unit AI
    if let Some(ai) = c.w.parts.unit_ai.get_mut(v.id) {
        for &(attacker, p) in &damaged {
            let n = Notify {
                caller: attacker,
                recipient: v.id,
                kind: NotifyKind::DAMAGED,
                p,
            };
            ai.post(n, state)?;
        }
    }
    for &a in struck.map_or(&[][..], |b| &b.annexes) {
        if let Some((_, _, false)) = c.w.locate(a) {
            let (_, x) = c.w.object_mut(a).ok_or(NOT_LOCATED)?;
            x.life.hp = v.hp;
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
    c: &mut Step<'_>,
    (me, actions): (&mut Unit, &mut Action),
    k: KillLeft,
    roles: &Roles,
) -> Result<(), GenieError> {
    if k.id != c.me {
        return c.w.kill_left(&k);
    }
    if let Some(kill) = k.kill {
        let master = me.master(c.w.master_tables(), c.player)?.clone();
        c.w.count_killed(c.player, &master, kill)?;
        if kill.to_gaia {
            me.ai_change_owner(actions, PlayerId::GAIA, c, roles)?;
        }
    }
    c.w.count_razed(&k.razed)
}

/// A strike's write-back where it ran ([`write_back`]), each object's kill and razings right
/// after it ([`file_kill`], `me` the updating object): the blast's objects (`blasted`, which go
/// back to the update's) in their order, then its target `v`.
///
/// # Errors
///
/// A notify a unit AI's queue does not take, or what a kill does not model.
pub(crate) fn write_struck(
    c: &mut Step<'_>,
    me: &mut Unit,
    actions: &mut Action,
    mut blasted: Vec<Victim>,
    v: &mut Victim,
    roles: &Roles,
) -> Result<(), GenieError> {
    for b in &mut blasted {
        if let Some(k) = write_back(c, b)? {
            file_kill(c, (me, actions), k, roles)?;
        }
    }
    c.blasted = blasted;
    if let Some(k) = write_back(c, v)? {
        file_kill(c, (me, actions), k, roles)?;
    }
    Ok(())
}

/// A strike's write-back where it ran ([`write_struck`] on the update's `blasted`, then its
/// target `v`; `me` the updating object), each kill filed after its object's.
///
/// # Errors
///
/// A notify a unit AI's queue does not take, or what a kill does not model.
pub(crate) fn write_strike_back(
    c: &mut Step<'_>,
    me: &mut Unit,
    actions: &mut Action,
    v: &mut Victim,
    roles: &Roles,
) -> Result<(), GenieError> {
    let blasted = take(&mut c.blasted);
    write_struck(c, me, actions, blasted, v, roles)
}

impl World {
    /// What a hit's write-back left ([`KillLeft`]) on an object the world holds live: its kill
    /// ([`World::killed`]), then its razings ([`World::count_razed`]).
    ///
    /// # Errors
    ///
    /// What the kill does not model, or an attribute a player lacks.
    pub(crate) fn kill_left(&mut self, k: &KillLeft) -> Result<(), GenieError> {
        let Some(Place::Live(q, j)) = self.place(k.id) else {
            return Ok(());
        };
        if let Some(kill) = k.kill {
            self.killed(q, j, kill)?;
        }
        self.count_razed(&k.razed)
    }

    /// Each razing a hit left ([`crate::combat::damage::Razed`]): the history event of the
    /// attacker's player, the attribute adds in their order.
    ///
    /// # Errors
    ///
    /// An attribute a player lacks.
    pub(crate) fn count_razed(&mut self, razed: &[Razed]) -> Result<(), GenieError> {
        for r in razed {
            if let Some(kind) = r.event {
                let now = self.world_time;
                self.players
                    .player_mut(r.attacker_player)?
                    .history
                    .add_event(kind, r.owner.index() as f32, 0.0, 0, now);
            }
            for &(p, attribute, amount) in &r.adds {
                self.players
                    .player_mut(p)?
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
        q: PlayerId,
        j: ListIndex,
        kill: Kill,
    ) -> Result<(), GenieError> {
        let u = &self.players.live(q, j)?;
        let master = u.master(self.master_tables(), q)?.clone();
        self.count_killed(q, &master, kill)?;
        if kill.to_gaia {
            self.give_gaia(q, j)?;
        }
        Ok(())
    }

    /// [`World::killed`]'s counts of an object of player `q` and `master`: its owner's computer
    /// AI (not modelled), a villager's counts for its owner, then the kill's
    /// ([`World::count_kill_of`]).
    ///
    /// # Errors
    ///
    /// A computer player's object, and an attribute a player lacks.
    pub(crate) fn count_killed(
        &mut self,
        q: PlayerId,
        master: &Master,
        kill: Kill,
    ) -> Result<(), GenieError> {
        if self.players.player(q)?.kind.is_computer() {
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
                self.players
                    .player_mut(q)?
                    .add_attribute(attribute::VILLAGER_KILLED_BY_ANIMAL, Amount::new(1.0))?;
            }
            let by_kind = match self.players.player(ap)?.kind {
                PlayerType::HUMAN => Some(attribute::VILLAGER_KILLED_BY_TYPE_1),
                PlayerType::GAIA => Some(attribute::VILLAGER_KILLED_BY_TYPE_2),
                PlayerType::COMPUTER => Some(attribute::VILLAGER_KILLED_BY_TYPE_3),
                _ => None,
            };
            if let Some(kind) = by_kind {
                self.players
                    .player_mut(q)?
                    .add_attribute(kind, Amount::new(1.0))?;
            }
        }
        self.count_kill_of(q, master, ap)
    }

    /// `FUN_1412005b0` on the object `id` killed for player `killer` (a movement damage's,
    /// [`crate::combat::abilities`]): the counts of [`World::killed`] by its master (its own one, else
    /// its player's).
    ///
    /// # Errors
    ///
    /// An object the slice does not hold, and an attribute whose change the slice does not
    /// model.
    pub(crate) fn count_kill(&mut self, id: ObjectId, killer: PlayerId) -> Result<(), GenieError> {
        let (q, u) = self.object(id).ok_or(NOT_HELD)?;
        let master = u.master(self.master_tables(), q)?.clone();
        self.count_kill_of(q, &master, killer)
    }

    /// `FUN_1412005b0`'s counts of a kill of an object of player `q` and `master` for player `ap`:
    /// unless its class is one of the non-military ones (`FUN_141188b20`) `ap` counts a kill
    /// (attribute 20) and `q` a loss (154); `ap` counts it against `q` (300 + `q`) and `q`
    /// against `ap` (325 + `ap`); both count its paid cost (170 and 152, and 400 + `q`).
    pub(crate) fn count_kill_of(
        &mut self,
        q: PlayerId,
        master: &Master,
        ap: PlayerId,
    ) -> Result<(), GenieError> {
        if !self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::PEACEFUL)
        {
            self.players
                .player_mut(ap)?
                .add_attribute(attribute::KILLS, Amount::new(1.0))?;
            self.players
                .player_mut(q)?
                .add_attribute(attribute::KILLS_OTHER, Amount::new(1.0))?;
        }
        self.players
            .player_mut(ap)?
            .add_attribute(attribute::KILLS_OF.offset(q.index())?, Amount::new(1.0))?;
        self.players
            .player_mut(q)?
            .add_attribute(attribute::KILLED_BY.offset(ap.index())?, Amount::new(1.0))?;
        let cost = paid_cost(master);
        self.players
            .player_mut(ap)?
            .add_attribute(attribute::KILLS_VALUE, cost)?;
        self.players
            .player_mut(q)?
            .add_attribute(attribute::KILLS_VALUE_OTHER, cost)?;
        self.players
            .player_mut(ap)?
            .add_attribute(attribute::KILLS_VALUE_OF.offset(q.index())?, cost)?;
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
        && !master.weapons.is_empty()
}

/// `master`'s paid cost (`+0x880`: each entry with a resource and the paid flag), summed as
/// floats in its order, as the kill counts read it (`FUN_1412005b0`, `FUN_1411cea90`).
pub(crate) fn paid_cost(master: &Master) -> Amount {
    let mut cost = Amount::ZERO;
    for (_, amount, paid) in cost_entries(&master.cost) {
        if paid {
            cost += Amount::new(amount.as_f32());
        }
    }
    cost
}

/// A victim the world located without its object.
const NOT_LOCATED: GenieError =
    GenieError::unreachable("a victim the world located without its object");
