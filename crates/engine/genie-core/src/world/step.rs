//! What a tick's update of one object reads and writes ([`Step`]) and its walks.

use crate::GenieError;
use crate::combat::damage::Victim;
use crate::math::rng::{Stream, Well512};
use crate::movement::move_to::Walk;
use crate::movement::{Me, Planning};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::unit_ai::UnitAi;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlots;

pub(crate) struct Step<'a> {
    /// The world the update reaches (FD-045-D28): its objects by id, every player and the
    /// world's parts. The updating object is out of its slot (its stand-in, which no lookup by
    /// id finds), so a call on another object is the world's routine at once, and a call on
    /// the updating object itself goes to this `Step` for after the step.
    pub(crate) w: &'a mut World,
    /// The updating object's id.
    pub(crate) me: ObjectId,
    /// The tick's dt in milliseconds (`world+0x164`; in seconds, [`seconds`]).
    ///
    /// [`seconds`]: crate::objects::lifecycle::seconds
    pub(crate) dt: Millis,
    /// The ticks so far, this one included.
    pub(crate) tick: crate::world::Tick,
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
    pub(crate) side_notifies: Option<Vec<(crate::unit_ai::Notify, crate::objects::ObjectState)>>,
}

/// What a walk takes of the update beside the world: the dt in seconds, the world's time and
/// the owner's computer flag.
#[derive(Clone, Copy)]
pub(crate) struct Pace {
    pub(crate) dt: Seconds,
    pub(crate) now: WorldTime,
    pub(crate) computer: bool,
}

/// The world's parts beside a walk ([`World::walk_beside`]), which an update reads and changes
/// while its unit walks.
pub(crate) struct Beside<'b> {
    /// The Sprite stream, which a sprite the walk shows draws from.
    pub(crate) sprite_rng: &'b mut Well512,
}

impl World {
    /// A walk at `pace` ([`Step::walk`]) and the world's parts beside it, each its own
    /// borrow.
    pub(crate) fn walk_beside<'b>(
        &'b mut self,
        pace: Pace,
        body: &'b mut crate::objects::layers::Body,
        me: Me,
    ) -> (Walk<'b>, Beside<'b>) {
        let Self {
            spatial,
            pathing,
            groups,
            streams,
            ..
        } = self;
        let (object, sprite_rng) = streams.two(Stream::Object, Stream::Sprite);
        let walk = Walk {
            body,
            me,
            dt: pace.dt,
            world: spatial,
            plan: Some(Planning {
                pathing,
                now: pace.now,
                computer: pace.computer,
                object,
                groups,
            }),
        };
        let beside = Beside { sprite_rng };
        (walk, beside)
    }
}

impl Step<'_> {
    /// Runs `f` on the world with the updating object `me` lent back to its slot (FD-045-Q03:
    /// the game's world holds the object while its update calls a routine that scans the
    /// objects, a technology's effects or a search among them), then takes it out again from
    /// where `f` left it; `me` holds what `f` changed on it. While the unit AI's side runs
    /// ([`Step::side_notifies`]), a stand-in AI holds the AI's place on the lent unit.
    ///
    /// # Errors
    ///
    /// `f`'s, the object's stand-in or the object itself gone from the owner's live list, and
    /// a routine that reached the stand-in AI.
    pub(crate) fn lend<R>(
        &mut self,
        me: &mut Unit,
        f: impl FnOnce(&mut World) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        let p = self.player;
        // while the unit AI's side runs, its AI is out of the unit: a stand-in holds its place,
        // which no routine on the lent unit may reach
        let side = self.side_notifies.is_some();
        if side {
            me.ai = Some(UnitAi::default());
        }
        let lent = std::mem::replace(me, Unit::taken_out());
        let answer = self.w.put_back_own(p, lent).and_then(|_| f(self.w));
        let own = &mut self.w.players.player_mut(p)?.objects;
        let i = own.position_of(self.me).ok_or_else(|| {
            GenieError::unreachable("a lent object left its list").with("object", self.me)
        })?;
        *me = own.take_out(i)?;
        let stand_in = if side { me.ai.take() } else { None };
        let answer = answer?;
        if side && stand_in != Some(UnitAi::default()) {
            return Err(GenieError::unreachable(
                "a world routine the unit AI's side called reached the unit's AI",
            )
            .with("object", self.me));
        }
        Ok(answer)
    }

    /// [`Step::lend`] for a world routine on the updating object by its slot: `f` gets the world,
    /// the object's player and its index in the owner's live list.
    ///
    /// # Errors
    ///
    /// `f`'s and the lend's.
    pub(crate) fn lend_at<R>(
        &mut self,
        me: &mut Unit,
        f: impl FnOnce(&mut World, PlayerId, ListIndex) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        let (p, id) = (self.player, self.me);
        self.lend(me, |w| {
            let i = w
                .players
                .player(p)?
                .objects
                .position_of(id)
                .ok_or_else(|| {
                    GenieError::unreachable("a lent object left its list").with("object", id)
                })?;
            f(w, p, i)
        })
    }

    /// A notify for the updating unit `me`'s own AI from a call of the step (an Enter's own
    /// failure): posted at once, or held for the side's return while the AI is out of the unit
    /// ([`Step::side_notifies`]); a unit without a unit AI has none to tell.
    ///
    /// # Errors
    ///
    /// A notify the unit AI's queue does not take.
    pub(crate) fn notify_own(
        &mut self,
        me: &mut Unit,
        n: crate::unit_ai::Notify,
    ) -> Result<(), GenieError> {
        let state = me.life.state;
        if let Some(ai) = me.ai.as_mut() {
            return ai.post(n, state);
        }
        if let Some(own) = self.side_notifies.as_mut() {
            own.push((n, state));
        }
        Ok(())
    }

    /// The owner's slot 0xf8 with 1 and 1 on a unit's load (`FUN_141616820`,
    /// [`give_load`]): the owner gains the resource and its gathered total at once.
    ///
    /// [`give_load`]: crate::gathering::gather::unit::give_load
    ///
    /// # Errors
    ///
    /// [`give_load`]'s, or an attribute the owner does not have.
    pub(crate) fn give_load(
        &mut self,
        kind: AttributeId,
        amount: crate::players::Amount,
    ) -> Result<(), GenieError> {
        let mut adds = Vec::new();
        crate::gathering::gather::unit::give_load(
            (
                &self.w.spatial.relations,
                self.w.players.player(self.player)?.sharing,
            ),
            self.player,
            &mut adds,
            kind,
            amount,
        )?;
        self.w.players.player_mut(self.player)?.add_attributes(adds)
    }

    /// What a walk takes of the update beside the world ([`World::walk_beside`]).
    pub(crate) fn pace(&self) -> Pace {
        Pace {
            dt: self.dt.seconds(),
            now: self.w.world_time,
            computer: self.w.players[self.player.index()].computer,
        }
    }
}

impl Step<'_> {
    /// A walk and the Sprite stream, for an attacker's update.
    pub(crate) fn walk_and_sprites<'b>(
        &'b mut self,
        body: &'b mut crate::objects::layers::Body,
        me: Me,
    ) -> (Walk<'b>, &'b mut Well512) {
        let (object, sprite) = self.w.streams.two(Stream::Object, Stream::Sprite);
        (
            Walk {
                body,
                me,
                dt: self.dt.seconds(),
                world: &mut self.w.spatial,
                plan: Some(Planning {
                    pathing: &mut self.w.pathing,
                    now: self.w.world_time,
                    computer: self.w.players[self.player.index()].computer,
                    object,
                    groups: &self.w.groups,
                }),
            },
            sprite,
        )
    }
}
