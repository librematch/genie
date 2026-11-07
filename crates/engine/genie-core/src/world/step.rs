//! What a tick's update of one object reads and writes ([`Step`]) and its walks.

use crate::combat::damage::Victim;
use crate::math::rng::Well512;
use crate::movement::move_to::Walk;
use crate::movement::{Me, Planning};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::layers::Static;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::unit_ai::Notify;
use crate::unit_ai::UnitAi;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::objects_ref::ObjectsRef;
use crate::world::parts::PartTable;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};
use genie_model::model::Model;
use std::mem::{replace, take};

pub(crate) struct Step<'world> {
    /// The world the update reaches (FD-045-D28): its objects by id, every player and the
    /// world's parts. The updating object is out of its slot (its stand-in, which no lookup by
    /// id finds), so a call on another object is the world's routine at once, and a call on
    /// the updating object itself goes to this `Step` for after the step.
    pub(crate) world: &'world mut World,
    /// The updating object's id.
    pub(crate) me: ObjectId,
    /// The tick's dt in milliseconds (`world+0x164`; in seconds, [`seconds`]).
    ///
    /// [`seconds`]: crate::world::Millis::seconds
    pub(crate) dt: Millis,
    /// The owner's accumulated time when it has the turn: the full unit-AI update runs.
    pub(crate) turn: Option<Millis>,
    /// The objects about the target of a unit with an area effect (or about the unit, for its
    /// blast at death), which its blast may reach, read as the update starts; each hit writes
    /// them back where it ran ([`crate::combat::victims::write_back`]).
    pub(crate) blasted: Vec<Victim>,
    /// The player the unit's own AI gave it to (slot 0x70, [`Unit::ai_change_owner`]) and
    /// whether it has the unit's master: the update ends with the takeover ([`crate::objects::unit::Outcome::Captured`]).
    pub(crate) taken_over: Option<(PlayerId, bool)>,
    /// The unit is a building whose slot 0x70 ran in the update: the whole change is the
    /// caller's ([`crate::ownership::capture::Captured::building`]).
    pub(crate) building_handover: bool,
    /// The owner's id: the updating object's player, whose fields the update reads through
    /// the world (`w.players[player]`).
    pub(crate) player: PlayerId,
    /// While the unit AI's side runs ([`Unit::with_side`]: the AI out of the unit), the notifies
    /// the step's calls give that AI, posted to it as the side returns; None otherwise.
    pub(crate) side_notifies: Option<Vec<(Notify, ObjectState)>>,
}

/// What a walk takes of the update beside the world: the dt in seconds, the world's time and
/// the owner's type.
#[derive(Clone, Copy)]
pub(crate) struct Pace {
    pub(crate) dt: Seconds,
    pub(crate) now: WorldTime,
    pub(crate) player_kind: PlayerKind,
}

/// The world's parts beside a walk ([`World::walk_beside`]), which an update reads and changes
/// while its unit walks.
pub(crate) struct Beside<'step> {
    /// The Sprite stream, which a sprite the walk shows draws from.
    pub(crate) sprite_rng: &'step mut Well512,
    /// The unit AIs, which the walk's notifies go to.
    pub(crate) ais: &'step mut PartTable<UnitAi>,
}

impl World {
    /// A walk at `pace` ([`Walk`]) and the world's parts beside it, each its own
    /// borrow.
    pub(crate) fn walk_beside<'step>(
        &'step mut self,
        pace: Pace,
        place: &'step mut Static,
        me: Me,
    ) -> (Walk<'step>, Beside<'step>) {
        let Self {
            spatial,
            pathing,
            groups,
            streams,
            players,
            index,
            parts,
            model:
                Model {
                    masters,
                    roles,
                    tuning,
                    ..
                },
            ..
        } = self;
        let (object, sprite_rng) = streams.object_and_sprite();
        let (motion, motions) = parts.motion.lend(me.id);
        let walk = Walk {
            place,
            motion,
            me,
            dt: pace.dt,
            world: spatial,
            plan: Some(Planning {
                pathing,
                now: pace.now,
                player_kind: pace.player_kind,
                object,
                groups,
            }),
            by_id: ObjectsRef::of_tables(
                players,
                index,
                MasterTables::new(masters, &parts.own_master),
                (
                    &parts.building,
                    &parts.combat,
                    &parts.actions,
                    &parts.unread,
                    motions,
                ),
                roles,
                tuning,
            ),
        };
        let beside = Beside {
            sprite_rng,
            ais: &mut parts.unit_ai,
        };
        (walk, beside)
    }
}

impl Step<'_> {
    /// Runs `work` on the world with the updating object `me` lent back to its slot and its
    /// action layer `actions` to the table (FD-045-Q03: the game's world holds the object while
    /// its update calls a routine that scans the objects, a technology's effects or a search
    /// among them), then takes both out again from where `work` left them; `me` and `actions` hold
    /// what `work` changed on them. While the unit AI's side runs ([`Step::side_notifies`]), a
    /// stand-in AI holds the AI's place on the lent unit.
    ///
    /// # Errors
    ///
    /// `work`'s, the object's stand-in or the object itself gone from the owner's live list, and
    /// a routine that reached the stand-in AI.
    pub(crate) fn lend<R>(
        &mut self,
        me: (&mut Unit, &mut Action),
        work: impl FnOnce(&mut World) -> GenieResult<R>,
    ) -> GenieResult<R> {
        self.lend_to(me, |world, _| work(world))
    }

    /// [`Step::lend`], `work` given the slot the object went back to.
    ///
    /// # Errors
    ///
    /// As [`Step::lend`]'s.
    fn lend_to<R>(
        &mut self,
        (me, actions): (&mut Unit, &mut Action),
        work: impl FnOnce(&mut World, ListIndex) -> GenieResult<R>,
    ) -> GenieResult<R> {
        let player_id = self.player;
        // while the unit AI's side runs, its AI is out of the unit: a stand-in holds its place,
        // which no routine on the lent unit may reach
        let side = self.side_notifies.is_some();
        if side {
            self.world.return_unit_ai(me.id, UnitAi::default());
        }
        let lent = replace(me, Unit::taken_out());
        self.world.parts.put_actions(self.me, take(actions));
        let answer = self
            .world
            .put_back_own(player_id, lent)
            .and_then(|list_index| work(self.world, list_index));
        // `work` may move the lent object's slot (another's removal), and no routine run on a
        // lent object removes it (a gate's toggle, an unload, a group's leave)
        let list_index = self.world.live_index(player_id, self.me).ok_or_else(|| {
            GenieError::unreachable("a lent object left its list").with("object", self.me)
        })?;
        *me = self.world.take_out(player_id, list_index)?;
        *actions = self.world.parts.take_actions(self.me);
        let stand_in = if side {
            self.world.lend_unit_ai(me.id)
        } else {
            None
        };
        let answer = answer?;
        if side && stand_in != Some(UnitAi::default()) {
            return Err(GenieError::unreachable(
                "a world routine the unit AI's side called reached the unit's AI",
            )
            .with("object", self.me));
        }
        Ok(answer)
    }

    /// [`Step::lend`] for a world routine on the updating object by its slot: `work` gets the
    /// world, the object's player and its index in the owner's live list.
    ///
    /// # Errors
    ///
    /// `work`'s and the lend's.
    pub(crate) fn lend_at<R>(
        &mut self,
        me: (&mut Unit, &mut Action),
        work: impl FnOnce(&mut World, PlayerId, ListIndex) -> GenieResult<R>,
    ) -> GenieResult<R> {
        let player_id = self.player;
        self.lend_to(me, |world, list_index| work(world, player_id, list_index))
    }

    /// A notify for the updating unit `me`'s own AI from a call of the step (an Enter's own
    /// failure): posted at once, or held for the side's return while the AI is out of the unit
    /// ([`Step::side_notifies`]); a unit without a unit AI has none to tell.
    ///
    /// # Errors
    ///
    /// A notify the unit AI's queue does not take.
    pub(crate) fn notify_own(&mut self, me: &Unit, notify: Notify) -> GenieResult<()> {
        let state = me.life.state;
        if let Some(ai) = self.world.parts.unit_ai.get_mut(me.id) {
            return ai.post(notify, state);
        }
        if let Some(own) = self.side_notifies.as_mut() {
            own.push((notify, state));
        }
        Ok(())
    }

    /// The owner's slot 0xf8 with 1 and 1 on a unit's load (`FUN_141616820`,
    /// [`World::give_load`]): the owner gains the resource and its gathered total at once.
    ///
    /// # Errors
    ///
    /// [`World::give_load`]'s.
    pub(crate) fn give_load(&mut self, kind: AttributeId, amount: Amount) -> GenieResult<()> {
        self.world.give_load(self.player, kind, amount)
    }

    /// What a walk takes of the update beside the world ([`World::walk_beside`]).
    ///
    /// # Errors
    ///
    /// An updating player the world has not got.
    pub(crate) fn pace(&self) -> GenieResult<Pace> {
        Ok(Pace {
            dt: self.dt.seconds(),
            now: self.world.world_time,
            player_kind: self.world.players.player(self.player)?.kind,
        })
    }
}

impl Step<'_> {
    /// A walk and the Sprite stream, for an attacker's update.
    ///
    /// # Errors
    ///
    /// An updating player the world has not got.
    pub(crate) fn walk_and_sprites<'step>(
        &'step mut self,
        place: &'step mut Static,
        me: Me,
    ) -> GenieResult<(Walk<'step>, &'step mut Well512)> {
        let player_kind = self.world.players.player(self.player)?.kind;
        let (object, sprite) = self.world.streams.object_and_sprite();
        let parts = &mut self.world.parts;
        let (motion, motions) = parts.motion.lend(me.id);
        Ok((
            Walk {
                place,
                motion,
                me,
                dt: self.dt.seconds(),
                world: &mut self.world.spatial,
                plan: Some(Planning {
                    pathing: &mut self.world.pathing,
                    now: self.world.world_time,
                    player_kind,
                    object,
                    groups: &self.world.groups,
                }),
                by_id: ObjectsRef::of_tables(
                    &self.world.players,
                    &self.world.index,
                    MasterTables::new(&self.world.model.masters, &parts.own_master),
                    (
                        &parts.building,
                        &parts.combat,
                        &parts.actions,
                        &parts.unread,
                        motions,
                    ),
                    &self.world.model.roles,
                    &self.world.model.tuning,
                ),
            },
            sprite,
        ))
    }
}
