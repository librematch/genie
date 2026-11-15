//! An Unpack bound to its building and the world: its update with its walk and its reach, the
//! building's transform at work (`set_state(6)`), its work rate, its sprite's frame, its standing
//! sprite, its annexes and its unit AI's attack, and its destructor's transform back.

use crate::GenieError;
use crate::GenieResult;
use crate::construction::pack::{PACK_NOT_BUILDING, PackTime, pack_time};
use crate::construction::unpack::{Reach, UnpackAction, UnpackState, UnpackStatus};
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::movement::footprint_gap_squared;
use crate::movement::move_to::{GiveUp, ListUpdate, MoveTo, SubCode, update_sub_actions};
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::UpdateCode;
use crate::objects::layers::Action;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::teardown::{HeadCode, stop_if_moving};
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::orders::ActionsOnChange;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::lets::release;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

/// An Unpack whose `set_state(6)` finds no action at its list's head: only its own update calls
/// it, on the head it runs.
const UNPACK_OFF_HEAD: GenieError =
    GenieError::unreachable("an Unpack's set_state(6) without an action at its list's head");

/// An Unpack's walk to a target the world does not hold, which its update's check let go first.
const UNPACK_TARGET_GONE: GenieError =
    GenieError::unreachable("an Unpack's held target the world does not hold after its check");

/// The master an Unpack's building turns into: its master's transform (`+0x9ba`, −1 none) when
/// its owner's masters (`player+0x28`) hold it (`FUN_140ae75d0`, `FUN_140ae67c0`).
///
/// # Errors
///
/// A master without its building part ([`PACK_NOT_BUILDING`]).
fn held_transform(
    world: &World,
    player: PlayerId,
    master: &Master,
) -> GenieResult<Option<MasterId>> {
    let building = master.building.as_ref().ok_or(PACK_NOT_BUILDING)?;
    Ok(building.transform_unit.filter(|new| {
        world
            .model
            .masters
            .of(player)
            .is_some_and(|masters| masters.get(*new).is_some())
    }))
}

impl Unit {
    /// A building's Unpack in its update (slot 0x30, `FUN_140ae78f0`) and its code: its target's
    /// check first ([`Unit::unpack_target`]); then done (1) in states 1 and 13, at 6 its work
    /// ([`Unit::unpack_work`]), at 11 its walk ([`Unit::unpack_walk`]), and on (0) in any other
    /// state. Nothing without one.
    ///
    /// # Errors
    ///
    /// The target's release, the work's and the walk's.
    pub(crate) fn unpack_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
    ) -> GenieResult<HeadCode> {
        let Some(mut unpack) = actions.list.unpack().copied() else {
            return Ok(HeadCode::Keeps);
        };
        self.unpack_target(&mut unpack, actions, step)?;
        let code = match unpack.state {
            UnpackState::Done | UnpackState::Failed => UpdateCode::Done,
            UnpackState::Working => return self.unpack_work(unpack, actions, step),
            UnpackState::Moving { .. } => {
                self.unpack_walk(unpack, actions, step, me)?;
                UpdateCode::UnderWay
            }
            UnpackState::New => UpdateCode::UnderWay,
        };
        Ok(HeadCode::of(code))
    }

    /// The update's check of the Unpack's target (`0x140ae7907` to `0x140ae7952`): a target that
    /// the world's table does not hold (`FUN_1415417a0`) or that is past state 2 (`CMP byte
    /// [+0x640], 3`, `JC`) goes by the base's slot 0x68 with none (`FUN_140a7a230`): the Unpack,
    /// which is loud, lets it go (slot 0x180, [`release`]) after it holds none. The game's first
    /// test, which clears the pointer `+0x20` of a target id −1, holds nothing genie keeps.
    ///
    /// # Errors
    ///
    /// The release's.
    fn unpack_target(
        &mut self,
        unpack: &mut UnpackAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let Some(target) = unpack.target else {
            return Ok(());
        };
        let state = step.world.world_object(target).map(|held| held.state());
        if state.is_some_and(|state| state <= ObjectState::Alive) {
            return Ok(());
        }
        unpack.target = None;
        actions.list.set_unpack(Some(*unpack));
        let by = self.releaser(step)?;
        release(step, self, target, by)
    }

    /// The Unpack's work at 6 (`0x140ae7b84` on): the work of the update
    /// ([`Unit::building_work`] with [`UnpackTuning::instant_work`]) goes onto its timer
    /// ([`UnpackAction::work`]), the sprite takes the frame of the timer
    /// ([`UnpackAction::frame`], [`Unit::show_work_frame`]), and once the timer reached the
    /// unpack time (the master's [`pack_time`]) the building unpacked ([`Unit::unpack_done`]) and
    /// the Unpack is done (1); else it goes on (0).
    ///
    /// [`UnpackTuning::instant_work`]: crate::world::layer::generated::UnpackTuning::instant_work
    ///
    /// # Errors
    ///
    /// A building of a master the world lacks, one without its sprite, and what its end does.
    fn unpack_work(
        &mut self,
        mut unpack: UnpackAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<HeadCode> {
        let (work, time) = {
            let master = object_master(
                step.world.parts.own_master.get(self.id),
                (self.id, self.life.master_id),
                step.world.model.masters.of(step.player),
            )?;
            let instant = Seconds::new(step.world.model.tuning.unpack.instant_work);
            (self.building_work(master, step, instant), pack_time(master))
        };
        let reached = unpack.work(work, time);
        actions.list.set_unpack(Some(unpack));
        self.show_work_frame(|frames| unpack.frame(time, frames))?;
        if reached == PackTime::Left {
            return Ok(HeadCode::of(UpdateCode::UnderWay));
        }
        self.unpack_done(unpack, actions, step)?;
        Ok(HeadCode::of(UpdateCode::Done))
    }

    /// The end of the Unpack's work (`0x140ae7d2c` to `0x140ae8183`): the building takes its
    /// master's standing sprite and makes its annexes, the building lent back to its slot
    /// ([`World::stand_with_annexes`]); the status becomes [`UnpackStatus::Unpacked`], then
    /// `set_state(1)` ([`UnpackAction::set_bare`]: state 6 holds no sub-action); with a held
    /// target its unit AI takes the attack order on it first, within the building's weapon range
    /// (slot 0x230, `+0x828`; [`Unit::queue_siege_attack`]).
    ///
    /// # Errors
    ///
    /// The annexes' making, a building of a master the world lacks, and the attack order's.
    fn unpack_done(
        &mut self,
        mut unpack: UnpackAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        step.lend_at((self, actions), |world, player, idx| {
            world.stand_with_annexes(player, idx)
        })?;
        unpack.status = UnpackStatus::Unpacked;
        let walk = unpack.set_bare(UnpackState::Done);
        actions.list.set_unpack(Some(unpack));
        stop_if_moving(step.world.parts.motion_mut(self.id), walk)?;
        let Some(target) = unpack.target else {
            return Ok(());
        };
        let range = self
            .master(step.world.master_tables(), step.player)?
            .combat_part()
            .weapon_range;
        self.queue_siege_attack(target, range, step)
    }

    /// The Unpack's walk at 11 (`0x140ae79a8` on): its sub-action list's update
    /// ([`Unit::unpack_walk_update`]), then by its code: done (1) unpacks
    /// ([`Unit::unpack_set_working`]); on its way (0) and failed (3) test the reach
    /// ([`Unit::unpack_reach`]): within it the building unpacks, out of it a failed walk fails the
    /// Unpack (`set_state(13)`) and one on its way goes on. A target the walk let go (5) changes
    /// nothing. The update answers 0 in every case. The game's arms for 2 and 4 (to 6, and the
    /// target let go and `set_state(11)`) never run: a quiet `MoveTo` gives neither
    /// ([`SubCode`]).
    ///
    /// # Errors
    ///
    /// The walk's, the reach's and the state changes'.
    fn unpack_walk(
        &mut self,
        mut unpack: UnpackAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
    ) -> GenieResult<()> {
        let code = self.unpack_walk_update(&mut unpack, step, me);
        actions.list.set_unpack(Some(unpack));
        let code = code?;
        if code == SubCode::Done {
            return self.unpack_set_working(unpack, actions, step);
        }
        if code == SubCode::TargetGone {
            return Ok(());
        }
        match (self.unpack_reach(&unpack, code, step)?, code) {
            (Reach::Within, _) => self.unpack_set_working(unpack, actions, step),
            (Reach::Out, SubCode::Failed) => {
                let walk = unpack.set_bare(UnpackState::Failed);
                actions.list.set_unpack(Some(unpack));
                stop_if_moving(step.world.parts.motion_mut(self.id), walk)
            }
            (Reach::Out, _) => Ok(()),
        }
    }

    /// The update of the Unpack's sub-action list (`FUN_140a992b0`: its quiet `MoveTo`'s,
    /// [`update_sub_actions`]) and its [`SubCode`]: in state 3 its path asks what the unit's AI
    /// asks of it ([`World::start_ask`], `0x140aa2a70`: toward a held target's place, and nothing
    /// where the `MoveTo`'s first check lets its target go); a human player's villager waits once
    /// before it gives up, and the AI's stance decides the wait ([`GiveUp`], an AI's 0 without
    /// one); after a start or a move the units in the unit's way go to its unit AI at once (notify
    /// 0x215). Done for a list without a `MoveTo` ("There is no list.").
    ///
    /// # Errors
    ///
    /// An owner the world lacks, a unit of a master it lacks, the walk's and the notifies'.
    fn unpack_walk_update(
        &mut self,
        unpack: &mut UnpackAction,
        step: &mut Step<'_>,
        me: Me,
    ) -> GenieResult<SubCode> {
        let walked = unpack.state.move_to().copied();
        let at = self.place.position().ground();
        let ask = walked.map_or_default(|move_to| step.world.start_ask(&move_to, self.id, at));
        let class = self.master(step.world.master_tables(), step.player)?.class;
        let gives_up = GiveUp::of(
            step.world.players.player(step.player)?.kind,
            step.world.model.roles.of_class(class),
            step.world.parts.unit_ai.get(self.id),
        );
        let how = ListUpdate {
            gives_up,
            ask,
            ..ListUpdate::default()
        };
        let code = match unpack.state.walk_mut() {
            Some(move_to) => update_sub_actions(move_to, step, self, me, how)?,
            // a state that does not walk has no sub-action: its list's update is the empty one's
            None => update_sub_actions(&mut None, step, self, me, how)?,
        };
        if walked.is_some_and(|move_to| matches!(move_to.state, MoveTo::Start | MoveTo::Moving)) {
            self.post_in_the_way(
                step.world.parts.motion.mover_mut(self.id),
                &mut step.world.parts.unit_ai,
            )?;
        }
        Ok(code)
    }

    /// The reach test of the Unpack's walk after its `code` (`0x140ae79ee` on): with a target, the
    /// squared gap between the unit's and the target's footprints by their masters' radii
    /// (`FUN_14118f8c0`, [`footprint_gap_squared`]; [`UnpackAction::reach_target`]); without one a
    /// failed walk is within reach (`CMP BL, 3`: it unpacks where it stands), else the gaps to the
    /// point ([`UnpackAction::reach_point`]).
    ///
    /// # Errors
    ///
    /// A unit or a target of a master the world lacks, and a target the world does not hold
    /// ([`UNPACK_TARGET_GONE`]).
    fn unpack_reach(
        &self,
        unpack: &UnpackAction,
        code: SubCode,
        step: &Step<'_>,
    ) -> GenieResult<Reach> {
        let radius = self.master(step.world.master_tables(), step.player)?.radius;
        let Some(target) = unpack.target else {
            if code == SubCode::Failed {
                return Ok(Reach::Within);
            }
            return Ok(unpack.reach_point(self.position(), radius));
        };
        let held = step
            .world
            .world_object(target)
            .ok_or_else(|| UNPACK_TARGET_GONE.with("target", target))?;
        let target_radius = step
            .world
            .master(held.player(), held.master_id(step.world.model.slots))?
            .radius;
        let gap = footprint_gap_squared(self.position(), radius, held.position(), target_radius);
        Ok(unpack.reach_target(gap.get()))
    }

    /// `set_state(6)` (slot 0x80, `FUN_140ae75d0`) of the Unpack `unpack` at the head of the
    /// unit's list, from its update: its sub-actions go (its `MoveTo`'s teardown stops the unit)
    /// and the state becomes 6. Then with its master's transform held by its owner
    /// ([`held_transform`]) the building turns into it with the Unpack out of its list
    /// ([`Unit::unpack_transform`]); without one the Unpack fails (`set_state(13)`, the
    /// fall-through at its end).
    ///
    /// # Errors
    ///
    /// A unit of a master the world lacks or without its building part, the stop's and the
    /// transform's.
    pub(crate) fn unpack_set_working(
        &mut self,
        mut unpack: UnpackAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let walk = unpack.set_bare(UnpackState::Working);
        actions.list.set_unpack(Some(unpack));
        stop_if_moving(step.world.parts.motion_mut(self.id), walk)?;
        let master = self.master(step.world.master_tables(), step.player)?;
        let Some(new) = held_transform(step.world, step.player, master)? else {
            let walk = unpack.set_bare(UnpackState::Failed);
            actions.list.set_unpack(Some(unpack));
            return stop_if_moving(step.world.parts.motion_mut(self.id), walk);
        };
        self.unpack_transform(new, actions, step)
    }

    /// `set_state(6)`'s transform to the master `new` (`0x140ae763e` to `0x140ae76d0`): the head
    /// (the Unpack) leaves the list (`FUN_140a98e90`: for a unit below state 3 a next head's slot
    /// 0x28 with 0, [`Unit::restart_head`]); the building takes `new` (slot 0x90 with 0,
    /// [`World::transform`], the building lent back to its slot): the rest of its list goes. Then
    /// the Unpack goes back to the head (`FUN_140a97900`) of a unit below state 3; for one in state
    /// 3 or later its destructor runs instead (slot 0 with 1, [`Unit::let_go_own`]). The unit
    /// takes its new master's construction sprite (slot 0x68 with `+0x950`; its standing one for
    /// none, `FUN_141158620`), which the game calls also when it deleted the Unpack (it reads the
    /// unit from the freed block); and the Unpack back at the head starts its timer at the unit's
    /// pack progress (`+0x88a`, a short to a float) at work ([`UnpackStatus::Unpacking`]).
    ///
    /// # Errors
    ///
    /// A list without its head ([`UNPACK_OFF_HEAD`]), the restart's, the transform's, the
    /// destructor's, a unit of a master the world lacks or without its building part, and the
    /// sprite's.
    fn unpack_transform(
        &mut self,
        new: MasterId,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let head = actions.list.pop_head().ok_or(UNPACK_OFF_HEAD)?;
        if !actions.list.is_empty() && self.life.state < ObjectState::Dying {
            self.restart_head(actions, step)?;
        }
        step.lend_at((self, actions), |world, player, idx| {
            world.transform(player, idx, new, ActionsOnChange::Clear)
        })?;
        if self.life.state < ObjectState::Dying {
            actions.list.push_head(head);
        } else {
            let top = step.world.top_speed(step.player, self)?;
            let torn = head.tear_down(step.world.parts.motion_mut(self.id), top)?;
            let by = self.releaser(step)?;
            self.let_go_own(torn, actions, step, by)?;
        }
        self.show_construction(step)?;
        let progress = actions.pack_progress;
        if let Some(own) = actions.list.unpack_mut() {
            own.timer = Seconds::new(f32::from(progress.get()));
            own.status = UnpackStatus::Unpacking;
        }
        Ok(())
    }

    /// Slot 0x68 with the unit's master's construction sprite (`+0x950`), its standing one
    /// (`+0x470`) for none (`FUN_141158620`): [`Unit::show_graphic`].
    ///
    /// # Errors
    ///
    /// A unit of a master the world lacks or without its building part, and a sprite the slice
    /// holds without the one shown.
    fn show_construction(&mut self, step: &mut Step<'_>) -> GenieResult<()> {
        let master = self.master(step.world.master_tables(), step.player)?;
        let building = master.building.as_ref().ok_or(PACK_NOT_BUILDING)?;
        let shown = building
            .construction_sprite
            .or(master.standing_sprite)
            .and_then(|sprite_id| step.world.model.graphics.get(sprite_id))
            .copied();
        let id = self.id;
        self.show_graphic(
            step.world.parts.motion.layer_mut(id),
            shown,
            step.world.streams.get_mut(Stream::Sprite),
            step.world.model.tuning.sprite,
        )
    }

    /// The Unpack's destructor (`FUN_140ae67c0`) as the unit's own deletion runs it, `status` the
    /// torn-down Unpack's: [`World::unpack_torn_down`] on the building lent back to its slot,
    /// which turns it back where the game undoes the Unpack. Its base teardown is the caller's.
    ///
    /// # Errors
    ///
    /// The destructor's.
    pub(crate) fn unpack_left(
        &mut self,
        status: UnpackStatus,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        step.lend_at((self, actions), |world, player, idx| {
            world.unpack_torn_down(player, idx, status)
        })
    }
}

impl World {
    /// The Unpack's destructor (`FUN_140ae67c0`) on player `player`'s unit at `idx`, `status` the
    /// torn-down Unpack's, as a deletion outside the unit's step runs it: in a played game at work
    /// ([`UnpackStatus::is_undone_in`]) the status 0, which nothing reads after it, and with its
    /// master's transform held by its owner ([`held_transform`]) the building turns into it (slot
    /// 0x90 with 0, [`World::transform`]): the rest of its list goes. The local player's interface
    /// flag (`DAT_143fb1d40 +0x1871`, for the selected object of the local player,
    /// `FUN_140c88440` and `FUN_141663f50`) is the screen's. The base teardown (`FUN_140a73fc0`)
    /// lets the target go after it ([`World::let_go_torn`]).
    ///
    /// # Errors
    ///
    /// A unit the world does not hold live, of a master it lacks or without its building part, and
    /// the transform's.
    pub(crate) fn unpack_torn_down(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        status: UnpackStatus,
    ) -> GenieResult<()> {
        if !status.is_undone_in(self.game_state) {
            return Ok(());
        }
        let unit = self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        let Some(new) = held_transform(self, player, master)? else {
            return Ok(());
        };
        self.transform(player, idx, new, ActionsOnChange::Clear)
    }
}
