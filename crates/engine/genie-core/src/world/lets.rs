//! What an object's update does to the objects its actions take and let go (slots 0x178 and
//! 0x180) and to the building a counted builder leaves: each the world's call where the game
//! makes it (FD-045-D07, D28; [`World::take`], [`World::release`], [`World::leave_builders`]),
//! which runs [`Lets`] on a view of the whole world; the unit AI's side runs them so too, its
//! own AI's notifies held until its call returns ([`World::with_lets_for`]).
//!
//! The updating object itself is out of its slot (its stand-in holds it), so no lookup by id
//! finds it: a call on it runs on the unit its caller holds (`me`), at once, its unit AI's
//! notifies held while the AI's side runs ([`Step::side_notifies`]).

use crate::GenieError;
use crate::construction::MainBuildingId;
use crate::objects::layers::{Building, Combat};
use crate::objects::lines::UnitLines;
use crate::objects::masters::MasterTables;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::release::{Releaser, released, released_fields};
use crate::objects::unit::Unit;
use crate::objects::{ObjectId, ObjectKey};
use crate::objects::{ObjectState, WorkerCount};
use crate::orders::object_gone;
use crate::players::PlayerId;
use crate::players::tracker::Tracker;
use crate::players::{NO_RELATIONS, Relations};
use crate::unit_ai::Notify;
use crate::unit_ai::UnitAi;
use crate::unit_ai::task_finder::allied as task_finder_allied;
use crate::world::others::{Others, OthersRef};
use crate::world::parts::{PartTable, repairers_in};
use crate::world::step::Step;
use crate::world::{Player, World};

/// What the calls on the objects let go and taken reach besides the view: the updating player
/// and its relations (whether a taker or releaser is allied), every player's masters and the
/// unit lines (a wake's tracker key), the updating player's tracker, and the notifies for the
/// updating object's own unit AI while the AI runs (taken out of the unit, it is posted them,
/// with the unit's state, when its side's call returns: [`crate::objects::unit::Unit::with_side`]).
pub(crate) struct Lets<'l> {
    pub(crate) player: PlayerId,
    pub(crate) relations: &'l Relations,
    pub(crate) masters: MasterTables<'l>,
    /// The units' AIs ([`crate::world::parts::Parts::unit_ai`]).
    pub(crate) ais: &'l mut PartTable<UnitAi>,
    /// The buildings' layers ([`crate::world::parts::Parts::building`]).
    pub(crate) buildings: &'l mut PartTable<Building>,
    /// The combat layers ([`crate::world::parts::Parts::combat`]).
    pub(crate) combats: &'l mut PartTable<Combat>,
    pub(crate) lines: &'l UnitLines,
    pub(crate) tracker: &'l mut Tracker,
    pub(crate) own_notifies: &'l mut Vec<(Notify, ObjectState)>,
}

/// A counted builder leaves the building `site` (`+0x9e8` one less): its construction's
/// builders in `buildings`, or another object's repairers in its combat layer in `combats`.
///
/// # Errors
///
/// None (an unreachable error where a table lost the part it took).
fn leave_site(
    buildings: &mut PartTable<Building>,
    combats: &mut PartTable<Combat>,
    site: ObjectId,
) -> Result<(), GenieError> {
    let n = repairers_in(buildings, combats, site)?;
    *n = n.left();
    Ok(())
}

impl World {
    /// `f` with the world's [`Lets`] for player `player`'s calls and a view of every player's
    /// objects: [`World::take`], [`World::release`] and [`World::leave_builders`] run the
    /// update's calls on it, and an update's own ([`take`], [`release`], [`leave_builders`])
    /// with the updating object, out of its slot, as `me`.
    pub(crate) fn with_lets<R>(
        &mut self,
        player: PlayerId,
        f: impl FnOnce(&mut Lets<'_>, &mut Others<'_>) -> R,
    ) -> Result<R, GenieError> {
        // no unit AI's side runs here: an updating unit holds its AI (one without has none to
        // tell), so nothing waits for a side's return
        self.with_lets_for(player, &mut Vec::new(), f)
    }

    /// [`World::with_lets`] for the side of the updating unit's AI ([`crate::unit_ai::world_view::UnitView`]),
    /// whose AI is out of the unit: the notifies for it go to `own_notifies`.
    pub(crate) fn with_lets_for<R>(
        &mut self,
        player: PlayerId,
        own_notifies: &mut Vec<(Notify, ObjectState)>,
        f: impl FnOnce(&mut Lets<'_>, &mut Others<'_>) -> R,
    ) -> Result<R, GenieError> {
        let no_player = || {
            GenieError::unreachable("lets for a player the world does not hold")
                .with("player", player)
        };
        let (before, rest) = self
            .players
            .split_at_mut_checked(player.index())
            .ok_or_else(no_player)?;
        let (own, after) = rest.split_first_mut().ok_or_else(no_player)?;
        let Player {
            objects,
            sleeping,
            relations,
            tracker,
            ..
        } = own;
        let mut others = Others::new(
            player,
            (objects, sleeping),
            (before, after),
            &mut self.index,
        );
        let mut lets = Lets {
            player,
            relations,
            masters: MasterTables::new(&self.masters, &self.parts.own_master),
            ais: &mut self.parts.unit_ai,
            buildings: &mut self.parts.building,
            combats: &mut self.parts.combat,
            lines: &self.lines,
            tracker,
            own_notifies,
        };
        Ok(f(&mut lets, &mut others))
    }
}

/// `f` with the updating player's [`Lets`] for a call on the updating object itself: its unit
/// AI's notifies go to the side's buffer while its side runs ([`Step::side_notifies`]).
fn own_lets<R>(
    c: &mut Step<'_>,
    f: impl FnOnce(&mut Lets<'_>, &mut Others<'_>) -> R,
) -> Result<R, GenieError> {
    match c.side_notifies.as_mut() {
        Some(own) => c.w.with_lets_for(c.player, own, f),
        None => c.w.with_lets(c.player, f),
    }
}

/// Slot 0x178 on object `id`, which an action of `by` takes, from an update: the world's take
/// ([`World::take`]); the updating object's own runs on `me` ([`Lets::take`]). An id that
/// names none takes nothing, as the game's lookup of −1 finds no object.
///
/// # Errors
///
/// [`World::take`]'s and [`Lets::take`]'s.
pub(crate) fn take(
    c: &mut Step<'_>,
    me: &mut Unit,
    id: impl ObjectKey,
    by: Releaser,
) -> Result<(), GenieError> {
    let Some(id) = id.key() else {
        return Ok(());
    };
    if id == c.me {
        own_lets(c, |lets, others| lets.take(others, Some(me), id, by))?
    } else {
        c.w.take(id, by)
    }
}

/// Slot 0x180 on object `id`, which an action of `by` lets go, from an update: the world's
/// release ([`World::release`]); the updating object's own runs on `me` ([`Lets::release`]).
/// An id that names none lets nothing go, as the game's lookup of −1 finds no object.
///
/// # Errors
///
/// [`World::release`]'s and [`Lets::release`]'s.
pub(crate) fn release(
    c: &mut Step<'_>,
    me: &mut Unit,
    id: impl ObjectKey,
    by: Releaser,
) -> Result<(), GenieError> {
    let Some(id) = id.key() else {
        return Ok(());
    };
    if id == c.me {
        return own_lets(c, |lets, others| lets.release(others, Some(me), id, by))?;
    }
    c.w.release(id, by)
}

/// A counted builder's leave of the building a Build on `id` counted it in, from an update
/// ([`World::leave_builders`]): one on the updating object `me` itself, or on an annex whose
/// main building it is, counts on `me` ([`Lets::leave_builders`]). An id that names none
/// leaves nothing, as the game's lookup of −1 finds no object.
///
/// # Errors
///
/// An object of a master its owner lacks.
pub(crate) fn leave_builders(
    c: &mut Step<'_>,
    me: &mut Unit,
    id: impl ObjectKey,
) -> Result<(), GenieError> {
    let Some(id) = id.key() else {
        return Ok(());
    };
    // the updating player's lets: the updating object's own site is reckoned by its owner
    own_lets(c, |lets, others| lets.leave_builders(others, Some(me), id))?
}

/// What an action list's teardown left the world, applied at once from an update: the buildings
/// its counted builders leave ([`leave_builders`]), then the targets it lets go ([`release`]),
/// as [`World::clear_actions`] applies them; `me` is the updating object.
///
/// # Errors
///
/// [`leave_builders`]' and [`release`]'s.
pub(crate) fn torn_down(
    c: &mut Step<'_>,
    me: &mut Unit,
    (builder_leaves, releases): (Vec<ObjectId>, Vec<(ObjectId, Releaser)>),
) -> Result<(), GenieError> {
    for id in builder_leaves {
        leave_builders(c, me, id)?;
    }
    for (id, by) in releases {
        release(c, me, id, by)?;
    }
    Ok(())
}

impl Lets<'_> {
    /// Slot 0x178 on object `id`, which an action of `by` takes ([`World::take`] through the
    /// view): a live one counts its worker ([`crate::objects::unit::Unit::taken`]; a building
    /// first lists an unallied taker of a foundation with a stack unit); a sleeping one counts
    /// its worker (at most 550) and wakes, the end of its player's live list and last in its
    /// tracker ([`Others::wake`]). An object the world does not hold is not taken. The updating
    /// object `me` of the lets' player (None outside an update), which no lookup finds while it
    /// updates, is taken itself, a live object.
    ///
    /// # Errors
    ///
    /// An object the lookup located that the view then does not hold.
    ///
    /// [`World::take`]: crate::world::World::take
    pub(crate) fn take(
        &mut self,
        others: &mut Others<'_>,
        me: Option<&mut Unit>,
        id: ObjectId,
        by: Releaser,
    ) -> Result<(), GenieError> {
        if let Some(u) = me.filter(|u| u.id == id) {
            let allied = self.allied(others.view(), by.player, self.player);
            self.taken_live(u, (self.player, allied), by)?;
            return Ok(());
        }
        let Some((p, _, sleeps)) = others.locate(id) else {
            return Ok(());
        };
        if sleeps {
            let (_, u) = others.get_mut(id).ok_or_else(|| object_gone(id))?;
            if u.workers < WorkerCount::MOST {
                u.workers = u.workers.plus(1);
            }
            let _ = others.wake(id, self.tracker, (self.masters.players, self.lines));
            return Ok(());
        }
        let allied = self.allied(others.view(), by.player, p);
        let (_, u) = others.get_mut(id).ok_or_else(|| object_gone(id))?;
        self.taken_live(u, (p, allied), by)
    }

    /// Slot 0x178 on the live object `u` of player `p`, `allied` to the taker's player: it counts
    /// its worker ([`crate::objects::unit::Unit::taken`]; a building first lists an unallied
    /// taker of a foundation with a stack unit).
    fn taken_live(
        &mut self,
        u: &mut Unit,
        (p, allied): (PlayerId, bool),
        by: Releaser,
    ) -> Result<(), GenieError> {
        let listed = if u.lists_taker(u.master(self.masters, p)?, allied) {
            Some(self.buildings.get_or_insert_with(u.id, || Building::NONE)?)
        } else {
            None
        };
        u.taken(listed, by.id);
        Ok(())
    }

    /// Slot 0x180 on object `id`, which an action of `by` lets go ([`World::release`] through
    /// the view, [`released`]); a sleeping one only counts a worker less, and an object the
    /// world does not hold is not let go. The updating object `me` of the lets' player (None
    /// outside an update), which no lookup finds while it updates, is let go itself
    /// ([`Lets::release_own`]).
    ///
    /// # Errors
    ///
    /// A live object of a master its owner lacks, and a notify its unit AI's queue does not
    /// take.
    ///
    /// [`World::release`]: crate::world::World::release
    pub(crate) fn release(
        &mut self,
        others: &mut Others<'_>,
        me: Option<&mut Unit>,
        id: ObjectId,
        by: Releaser,
    ) -> Result<(), GenieError> {
        if let Some(u) = me.filter(|u| u.id == id) {
            return self.release_own(others, u, by);
        }
        let Some((p, _, sleeps)) = others.locate(id) else {
            return Ok(());
        };
        if sleeps {
            // a sleeping object (static: only those sleep) counts a worker less
            // (FUN_141197d00); asleep already (`+0x641`), it is not flagged again
            let (_, u) = others.get_mut(id).ok_or_else(|| object_gone(id))?;
            if WorkerCount::ZERO < u.workers {
                u.workers = u.workers.plus(-1);
            }
            return Ok(());
        }
        let allied = self.allied(others.view(), by.player, p);
        let (_, u) = others.get_mut(id).ok_or_else(|| object_gone(id))?;
        let m = u.master(self.masters, p)?;
        let (kind, listed) = (m.kind, u.lists_taker(m, allied));
        released(
            u,
            (self.ais.get_mut(id), self.buildings.get_mut(id)),
            (kind, listed),
            by,
        )
    }

    /// [`Lets::release`] on the updating object `me` itself, of the lets' player: a live
    /// object's ([`released_fields`]). Its unit AI is told (notify 699) at once, or, while the AI
    /// runs (taken out of the unit), when its side's call returns ([`Lets`]' `own_notifies`).
    ///
    /// # Errors
    ///
    /// An object of a master its owner lacks, and a notify its unit AI's queue does not take.
    fn release_own(
        &mut self,
        others: &Others<'_>,
        me: &mut Unit,
        by: Releaser,
    ) -> Result<(), GenieError> {
        let owner = self.player;
        let allied = self.allied(others.view(), by.player, owner);
        let m = me.master(self.masters, owner)?;
        let (kind, listed) = (m.kind, me.lists_taker(m, allied));
        let Some(n) = released_fields(me, self.buildings.get_mut(me.id), (kind, listed), by) else {
            return Ok(());
        };
        let state = me.life.state;
        if let Some(ai) = self.ais.get_mut(me.id) {
            return ai.post(n, state);
        }
        self.own_notifies.push((n, state));
        Ok(())
    }

    /// The builder count of the building a Build on `id` counted its builder in
    /// ([`World::leave_builders`] on a view, `FUN_140aaddb0`): the object itself, or for
    /// a building that is an annex its main building, counts one less (`+0x9e8`); none once the
    /// world no longer holds `id`. The updating object `me` (None outside an update), which no
    /// lookup finds, counts on itself.
    ///
    /// # Errors
    ///
    /// An object of a master its owner lacks.
    ///
    /// [`World::leave_builders`]: crate::world::World::leave_builders
    pub(crate) fn leave_builders(
        &mut self,
        others: &mut Others<'_>,
        me: Option<&mut Unit>,
        id: ObjectId,
    ) -> Result<(), GenieError> {
        let site = if let Some(u) = me.as_deref().filter(|u| u.id == id) {
            self.site_of(u, self.player, id)?
        } else {
            let Some((p, f, _)) = others.locate(id) else {
                return Ok(());
            };
            self.site_of(f, p, id)?
        };
        let held = me.is_some_and(|u| u.id == site) || others.get_mut(site).is_some();
        if held {
            leave_site(self.buildings, self.combats, site)?;
        }
        Ok(())
    }

    /// The building whose builders object `f` (`id`, of player `p`) counts in: an annex's main
    /// building, else the object itself.
    fn site_of(&self, f: &Unit, p: PlayerId, id: ObjectId) -> Result<ObjectId, GenieError> {
        // The building master type RESEARCH asks for (`'P'`, 80).
        Ok(if f.master(self.masters, p)?.kind == BUILDING {
            self.buildings
                .get(f.id)
                .and_then(|b| b.annex_of)
                .map_or(id, MainBuildingId::id)
        } else {
            id
        })
    }

    /// Player `from` counts player `to` as its own or allied, with its allies (`FUN_141663dc0`
    /// with 1): the updating player's relations, another's from the view.
    fn allied(&self, view: OthersRef<'_>, from: PlayerId, to: PlayerId) -> bool {
        let relations = if from == self.player {
            self.relations
        } else {
            let other = view.players().player(from);
            other.map_or(&NO_RELATIONS, |p| &p.relations)
        };
        task_finder_allied(relations, from, to, true)
    }
}
