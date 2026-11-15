//! A Pack bound to its building and the world: its update with the building's work rate, its
//! sprite's frame, its unit AI's attack and the transform, and its destructor's notify and the
//! building it puts back.

use crate::GenieError;
use crate::GenieResult;
use crate::construction::MainBuildingId;
use crate::construction::pack::{
    PACK_NOT_BUILDING, PackAction, PackState, PackStatus, PackTime, pack_time,
};
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::objects::Frame;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::UpdateCode;
use crate::objects::layers::Action;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::sprite::NO_SPRITE;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::orders::ActionsOnChange;
use crate::orders::Dispatch;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::Issuer;
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::OrderTargetKind;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::QueuedOrder;
use crate::world::PlayState;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::World;
use crate::world::lets::release;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

/// The priority of the attack order a Pack gives its unit AI once its building transformed
/// (`MOV dword ptr [RSP + 0x60], 0x32` at `0x140ad6d8c`): below 100, so it waits while the AI
/// has an action.
const ATTACK_PRIORITY: OrderPriority = OrderPriority::of_data(0x32);

/// A Pack or an Unpack that gives its target to a unit AI the building has not got: the game
/// calls slot 0x30 through `+0x6b0` with no null test (`0x140ad6d69`, `0x140ae7fd5`) and faults.
const PACK_WITHOUT_UNIT_AI: GenieError =
    GenieError::unreachable("a Pack's or Unpack's attack order through a null unit AI");
/// A Pack's or an Unpack's held target that the world does not hold, which its update's check
/// let go first.
const PACK_TARGET_GONE: GenieError = GenieError::unreachable(
    "a Pack's or Unpack's held target the world does not hold after its check",
);

impl PackAction {
    /// Whether its destructor puts its building back (`0x140ad548e`): in a played game
    /// (`FUN_140c896f0` with a world, `world+0x88` 0, [`PlayState::Playing`]) at work
    /// ([`PackStatus::Packing`]).
    pub(crate) fn is_undone_in(self, game_state: PlayState) -> bool {
        game_state == PlayState::Playing && self.status == PackStatus::Packing
    }
}

/// The notify a Pack's destructor posts to its unit's own unit AI once its timer reached the
/// pack time (`0x140ad545a`, slot 0x328 with `0x1fa` and `0x277`): its action on a target
/// completed.
const fn pack_completed(id: ObjectId) -> Notify {
    Notify::own(
        id,
        NotifyKind::ACTION_COMPLETED,
        [AiAction::OnTarget.value(), 0, 0],
    )
}

/// What a Pack's update reads of its building's master: the work rate, the pack time, the
/// transform and the weapon range.
#[derive(Clone, Copy, Debug)]
struct PackMaster {
    /// The pack time ([`pack_time`]).
    time: WholeSeconds,
    /// `+0x9ba`, `transform_unit`.
    transform: Option<MasterId>,
    /// Slot 0x230 (`0x14108d960`): the master's weapon range (`+0x828`).
    range: Distance,
}

impl PackMaster {
    /// The values of `master`.
    ///
    /// # Errors
    ///
    /// A master without its building part ([`PACK_NOT_BUILDING`]).
    fn of(master: &Master) -> GenieResult<Self> {
        let building = master.building.as_ref().ok_or(PACK_NOT_BUILDING)?;
        Ok(Self {
            time: pack_time(master),
            transform: building.transform_unit,
            range: master.combat_part().weapon_range,
        })
    }
}

impl Unit {
    /// A building's Pack in its update (slot 0x30, `0x140ad69d0`) and its code: its target's
    /// check first ([`Unit::pack_target`]); then done (1) in states 1 and 13, at 6 its work
    /// ([`Unit::pack_work`]), and on (0) in any other state. Nothing without one.
    ///
    /// # Errors
    ///
    /// The target's release and the work's.
    pub(crate) fn pack_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<HeadCode> {
        let Some(mut pack) = actions.list.pack().copied() else {
            return Ok(HeadCode::Keeps);
        };
        self.pack_target(&mut pack, actions, step)?;
        let code = match pack.state {
            PackState::Done | PackState::Failed => UpdateCode::Done,
            PackState::Working => return self.pack_work(pack, actions, step),
            PackState::New => UpdateCode::UnderWay,
        };
        Ok(HeadCode::of(code))
    }

    /// The update's check of the Pack's target (`0x140ad69d0` to `0x140ad6a2c`): a target that
    /// the world's table does not hold (`FUN_1415417a0`) or that is past state 2 (`CMP byte
    /// [+0x640], 3`, `JC`) goes by the base's slot 0x68 with none (`FUN_140a7a230`): the Pack,
    /// which is loud, lets it go (slot 0x180, [`release`]) after it holds none. The game's first
    /// test, which clears the pointer `+0x20` of a target id −1, holds nothing genie keeps.
    ///
    /// # Errors
    ///
    /// The release's.
    fn pack_target(
        &mut self,
        pack: &mut PackAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let Some(target) = pack.target else {
            return Ok(());
        };
        let state = step.world.world_object(target).map(|held| held.state());
        if state.is_some_and(|state| state <= ObjectState::Alive) {
            return Ok(());
        }
        pack.target = None;
        actions.list.set_pack(Some(*pack));
        let by = self.releaser(step)?;
        release(step, self, target, by)
    }

    /// The Pack's work at 6 (`0x140ad69d0`): the work of the update ([`Unit::building_work`])
    /// goes onto its timer ([`PackAction::work`]), the sprite takes the frame of the timer
    /// ([`Unit::show_work_frame`]), and once the timer reached the pack time the Pack is done
    /// (`set_state(1)`, 1) and its building transforms ([`Unit::pack_transform`]); else it goes
    /// on (0).
    ///
    /// # Errors
    ///
    /// A building of a master the world lacks or without its building part, one without its
    /// sprite, and the transform's.
    fn pack_work(
        &mut self,
        mut pack: PackAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<HeadCode> {
        let (work, master) = {
            let master = object_master(
                step.world.parts.own_master.get(self.id),
                (self.id, self.life.master_id),
                step.world.model.masters.of(step.player),
            )?;
            let instant = step.world.model.tuning.pack.instant_work;
            (
                self.building_work(master, step, instant),
                PackMaster::of(master)?,
            )
        };
        let reached = pack.work(work, master.time);
        self.show_work_frame(|frames| pack.frame(master.time, frames))?;
        if reached == PackTime::Left {
            actions.list.set_pack(Some(pack));
            return Ok(HeadCode::of(UpdateCode::UnderWay));
        }
        pack.finish();
        actions.list.set_pack(Some(pack));
        self.pack_transform(pack, master, actions, step)?;
        Ok(HeadCode::of(UpdateCode::Done))
    }

    /// The work a building's Pack (`0x140ad69d0`) or Unpack (`FUN_140ae78f0`) does in this
    /// update: the quick-build cheat's `instant` while the game's `+0xc14` is not 0
    /// (`FUN_140c884c0`, [`World::instant_work`]; [`PackTuning::instant_work`],
    /// [`UnpackTuning::instant_work`]); else its work rate, its powerup (slot 0x6e8, `+0xa30`)
    /// plus its `master`'s (`+0x798`, `ADDSS` in that order), times the update's seconds
    /// (`world+0x164`). Its hit points take no part, as they do in the Housing's.
    ///
    /// [`PackTuning::instant_work`]: crate::world::layer::generated::PackTuning::instant_work
    /// [`UnpackTuning::instant_work`]: crate::world::layer::generated::UnpackTuning::instant_work
    pub(crate) fn building_work(&self, master: &Master, step: &Step<'_>, instant: f32) -> Seconds {
        if step.world.instant_work != 0 {
            return Seconds::new(instant);
        }
        Seconds::new(
            (step.world.parts.combat_of(self.id).work_rate_powerup
                + master.action_part().work_rate)
                * step.dt.seconds(),
        )
    }

    /// The building's sprite takes the frame of a Pack's or an Unpack's timer (`frame_of` the
    /// frames of the sprite it shows, `+0x140`'s `+0x8`: [`PackAction::frame`],
    /// [`UnpackAction::frame`]) when that is below the frames and not the frame its sprite list
    /// shows of it (`FUN_1411933f0`, the list's slot 8): its sprite list's slot 0x28
    /// (`FUN_14119d2e0`, `FUN_1413ba880`) sets it ([`ActiveSprite::set_frame`]).
    ///
    /// [`ActiveSprite::set_frame`]: crate::objects::sprite::ActiveSprite::set_frame
    /// [`UnpackAction::frame`]: crate::construction::unpack::UnpackAction::frame
    ///
    /// # Errors
    ///
    /// A building without its sprite, whose frames the game reads through a null `+0x140`
    /// ([`NO_SPRITE`]).
    pub(crate) fn show_work_frame(
        &mut self,
        frame_of: impl FnOnce(Frame) -> Option<Frame>,
    ) -> GenieResult<()> {
        let sprite = self.sprite.as_mut().ok_or(NO_SPRITE)?;
        if let Some(frame) = frame_of(sprite.graphic.frames)
            && frame != sprite.frame
        {
            sprite.set_frame(frame);
        }
        Ok(())
    }

    /// The transform of a done Pack's building (`0x140ad6ce2` on): with its master's transform
    /// (`+0x9ba`) that its owner's masters hold (`player+0x28`), a held target goes to its unit
    /// AI first ([`Unit::queue_siege_attack`]), the Pack's status becomes
    /// [`PackStatus::Transformed`], and the building takes that master (slot 0x90 with 0,
    /// [`World::transform`], the building lent back to its slot): its action list goes, this Pack
    /// with it. Without such a master the building stays.
    ///
    /// # Errors
    ///
    /// The attack order's and the transform's.
    fn pack_transform(
        &mut self,
        mut pack: PackAction,
        master: PackMaster,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let Some(new) = master.transform.filter(|new| {
            step.world
                .model
                .masters
                .of(step.player)
                .is_some_and(|masters| masters.get(*new).is_some())
        }) else {
            return Ok(());
        };
        if let Some(target) = pack.target {
            self.queue_siege_attack(target, master.range, step)?;
        }
        pack.status = PackStatus::Transformed;
        actions.list.set_pack(Some(pack));
        step.lend_at((self, actions), |world, player, idx| {
            world.transform(player, idx, new, ActionsOnChange::Clear)
        })
    }

    /// The attack order (700) on a Pack's or an Unpack's `target` that its update gives the
    /// building's unit AI (`0x140ad6d69` to `0x140ad6faa`, `0x140ae7fd1` to `0x140ae8183`). It goes
    /// through the unit AI's slot 0x30
    /// (`FUN_140c563d0`, which passes on to slot 0x28, `FUN_140c55c90`) with the issuer the
    /// building, the target's id, master (`+0xa0`'s id), owner (`+0x138`'s `+0x718`)
    /// and place (`+0x3c8`, `+0x4c8`, `+0x5a8`), the building's weapon `range` (slot 0x230),
    /// not immediate (0), in front (1), at [`ATTACK_PRIORITY`], shown at the target's place,
    /// ready (1): the order goes into the queue's front ([`UnitAi::queue_order`]).
    ///
    /// [`UnitAi::queue_order`]: crate::unit_ai::UnitAi::queue_order
    ///
    /// # Errors
    ///
    /// A target the world lost or a building without a unit AI (unreachable errors), and the
    /// queue's.
    pub(crate) fn queue_siege_attack(
        &self,
        target: ObjectId,
        range: Distance,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let held = step
            .world
            .world_object(target)
            .ok_or_else(|| PACK_TARGET_GONE.with("target", target))?;
        let point: Position = held.position();
        let order = QueuedOrder {
            issuer: Some(Issuer::Object(self.id)),
            order: OrderKind::Attack,
            priority: ATTACK_PRIORITY,
            target: Some(target),
            target_owner: Some(held.player()),
            point,
            range,
            shown: point.ground(),
            ready: true,
            target_kind: Some(OrderTargetKind::of_master(
                held.master_id(step.world.model.slots),
            )),
        };
        let ai = step
            .world
            .parts
            .unit_ai
            .get_mut(self.id)
            .ok_or_else(|| PACK_WITHOUT_UNIT_AI.with("object", self.id))?;
        let _ = ai.queue_order(self.life.state, order, Dispatch::Queued, QueueEnd::Front)?;
        Ok(())
    }

    /// The Pack's destructor (`FUN_140ad5370`) as the unit's own deletion runs it, `pack` the
    /// torn-down Pack: with its timer at the pack time ([`PackTime::Reached`]) the unit's notify
    /// to its own unit AI ([`Step::notify_own`]: none without one, held while the AI's side runs);
    /// then a Pack the game undoes ([`PackAction::is_undone_in`]) puts the building back, the
    /// building lent back to its slot ([`World::stand_with_annexes`]). Its base teardown is the
    /// caller's.
    ///
    /// # Errors
    ///
    /// A unit of a master the world lacks, the notify's and the undoing's.
    pub(crate) fn pack_left(
        &mut self,
        pack: PackAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let time = pack_time(self.master(step.world.master_tables(), step.player)?);
        if PackTime::of(pack.timer, time) == PackTime::Reached {
            step.notify_own(self, pack_completed(self.id))?;
        }
        if pack.is_undone_in(step.world.game_state) {
            step.lend_at((self, actions), |world, player, idx| {
                world.stand_with_annexes(player, idx)
            })?;
        }
        Ok(())
    }
}

impl World {
    /// The Pack's destructor (`FUN_140ad5370`) on player `player`'s unit at `idx`, `pack` the
    /// torn-down Pack, as a deletion outside the unit's step runs it: with a unit AI
    /// (`+0x6b0`) and its timer at the pack time, the unit's notify (slot 0x328,
    /// [`pack_completed`]) to its own unit AI; then a Pack the game undoes
    /// ([`PackAction::is_undone_in`]) takes the status 0, which nothing reads after it, and puts
    /// the building back ([`World::stand_with_annexes`]). The local
    /// player's interface flag (`DAT_143fb1d40 +0x1871`, for the selected object of the local
    /// player, `FUN_141663f50`) is the screen's. The base teardown (`FUN_140a73fc0`) lets the
    /// target go after it ([`World::let_go_torn`]).
    ///
    /// # Errors
    ///
    /// A unit the world does not hold live or of a master it lacks, the notify's and the
    /// undoing's.
    pub(crate) fn pack_torn_down(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        pack: PackAction,
    ) -> GenieResult<()> {
        let unit = self.players.live(player, idx)?;
        let time = pack_time(unit.master(self.master_tables(), player)?);
        if PackTime::of(pack.timer, time) == PackTime::Reached {
            let unit = self.players.live(player, idx)?;
            unit.post_own(
                &mut self.parts.unit_ai,
                NotifyKind::ACTION_COMPLETED,
                pack_completed(unit.id).params,
            )?;
        }
        if pack.is_undone_in(self.game_state) {
            self.stand_with_annexes(player, idx)?;
        }
        Ok(())
    }

    /// The building a Pack's destructor puts back (`0x140ad5498` to `0x140ad574c`) and an
    /// Unpack's update makes at its end (`0x140ae7d2c` to `0x140ae7f9b`), player `player`'s unit at
    /// `idx`: it takes its master's standing sprite (slot 0x68 with `+0x470`,
    /// [`Unit::show_standing`]); then each annex slot of its master (`+0x990`, −1 none) makes its
    /// annex again ([`World::rebuild_annex`]).
    ///
    /// # Errors
    ///
    /// A unit the world does not hold live, of a master it lacks or without its building part,
    /// and the sprite's and the annexes' making.
    pub(crate) fn stand_with_annexes(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
    ) -> GenieResult<()> {
        let unit = self.players.live_mut(player, idx)?;
        let rng = self.streams.get_mut(Stream::Sprite);
        unit.show_standing(
            self.parts.motion.layer_mut(unit.id),
            OwnerMasters::new(&self.parts.own_master, self.model.masters.of(player)),
            &self.model.graphics,
            rng,
            self.model.tuning.sprite,
        )?;
        let unit = self.players.live(player, idx)?;
        let (id, at, hp) = (unit.id, unit.position(), unit.life.hp);
        let building = unit
            .master(self.master_tables(), player)?
            .building
            .ok_or(PACK_NOT_BUILDING)?;
        for (slot, (annex, offset)) in building
            .annexes
            .iter()
            .zip(&building.annex_offsets)
            .enumerate()
        {
            let Some(annex) = *annex else {
                continue;
            };
            let place = Position::new(at.x() + offset.x(), at.y() + offset.y(), at.z());
            self.rebuild_annex(player, id, slot, annex, place, hp)?;
        }
        Ok(())
    }

    /// One annex a Pack's destructor or an Unpack's update makes for the building `id` in its
    /// slot `slot`: the
    /// annex master's slot 0x40 with its owner `player`'s master (`player+0x28`, read with no
    /// null test) at `place` ([`World::remains`]: a building master's `FUN_14123d1a0`); the
    /// building's slot holds it (`+0xbc0` on), a building's main (`+0xbb8`) is the building, and
    /// it takes the building's hit points `hp` (`+0x278`). Unlike the building constructor's link
    /// ([`World::link_annex`]), its owner's watched list keeps it.
    ///
    /// # Errors
    ///
    /// An annex master the owner has not got, and what its making does not model.
    fn rebuild_annex(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        slot: usize,
        annex: MasterId,
        place: Position,
        hp: HitPoints,
    ) -> GenieResult<()> {
        let made = self.remains(player, annex, place, None)?;
        self.parts.building_mut(id)?.annexes.link(slot, made)?;
        let made_idx = self
            .live_index(player, made)
            .ok_or(GenieError::unreachable(
                "a new annex out of its owner's live list",
            ))?;
        let kind = self
            .players
            .live(player, made_idx)?
            .master(self.master_tables(), player)?
            .kind;
        if kind == MasterKind::BUILDING {
            self.parts.building_mut(made)?.annex_of = Some(MainBuildingId::new(id));
        }
        self.players.live_mut(player, made_idx)?.life.hp = hp;
        Ok(())
    }
}
