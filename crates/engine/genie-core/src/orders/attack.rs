//! An attack's start, a hunt's and an action's retarget.

use crate::combat::AttackCount;
use crate::combat::AttackTargetId;
use crate::combat::attack::{Attack, BeginLayer, arms, engagement, show_arrived};
use crate::construction::MainBuildingId;
use crate::gathering::gather::unit::WalkSprites;
use crate::gathering::hunt::{HuntAction, HuntRetarget, NewTarget, Retargeted, stand_sprite};
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::MoveStart;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::Voice;
use crate::objects::actions::LAST_TAKEN;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{MasterTables, Task};
use crate::objects::sprite::new_sprite;
use crate::objects::teardown::HeadStart;
use crate::orders::work::WORK_WITHOUT_UNIT_AI;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;
use crate::world::{ATTACKER_UNPLACED, HUNTER_UNMOVING, TARGET_UNPLACED};
use crate::{GenieError, GenieResult};

/// The work command's started Attack is not on its unit's list: [`Unit::start_listed_attack`]
/// puts it back, before an error too.
///
/// [`Unit::start_listed_attack`]: crate::objects::unit::Unit::start_listed_attack
const STARTED_ATTACK_UNLISTED: GenieError =
    GenieError::unreachable("the started attack is not on its list");

/// The install (slot 0x808) deleted the work command's Attack unstarted (state 3 or later) of a
/// unit with a unit AI. Only a combat object or a building holds one (`TRIBE_Combat_Object`), and
/// for those slot 0x7e8 (`FUN_141227190`, [`World::order_work`]) ignores the order past state 2
/// before the work command runs.
pub(super) const DELETED_ATTACK_WITH_UNIT_AI: GenieError = GenieError::unreachable(
    "the work command's attack of a unit with a unit AI was deleted unstarted",
);

/// Where an attack task's target aims ([`World::attack_aim`]).
pub(super) struct Aim {
    /// The target's master's class.
    pub(super) class: ClassId,
    /// The object aimed at (the target, or a building annex's main building), its place and its
    /// master's class.
    pub(super) aimed: ObjectId,
    pub(super) at: Position,
    pub(super) aimed_class: ClassId,
}

/// The attack task's part of the list's factory (`FUN_140a97e00` case 7) for the unit's `ai`, on
/// the command's `target`, which aims at `aim` ([`World::attack_aim`]). A unit with a unit AI
/// (`+0x6b0`) sets the AI's action 600 (`FUN_140c64b50`) and its target, the aimed object at its
/// place with its class (`FUN_140c64c10`), and the factory hands the constructor the aimed object
/// (`FUN_141189b20`'s answer, `0x140a97f00`: an annex's main building, any other target itself).
/// A unit without one jumps to the constructor with `target` itself (`0x140a97edf`). Returns the
/// object the constructor takes.
pub(super) const fn attack_task_ai(
    ai: Option<&mut UnitAi>,
    target: ObjectId,
    aim: &Aim,
) -> ObjectId {
    let Some(ai) = ai else {
        return target;
    };
    ai.set_action(Some(AiAction::Attack));
    ai.set_target(Some(aim.aimed), aim.aimed_class.or_none(), aim.at);
    aim.aimed
}

impl World {
    /// The work command's Attack (`FUN_14106ff70`'s tail for a task of action type 7) of object
    /// `idx` of `player` on `target` at `point`: the list's factory (`FUN_140a97e00` case 7: the
    /// unit AI's part, [`attack_task_ai`], then the constructor on the object that part hands
    /// it, [`World::construct_attack`]), the work notify (slot 0x828 with the Attack's type 9,
    /// [`World::attack_notifies`]), the install (slot 0x808, [`World::set_only_action`]: the
    /// list's deletion, then its add), and the Attack's second target (`FUN_140a7a190`), the
    /// command's `target` (`R14`, `0x141070b11`). So a unit with a unit AI attacks an annex's
    /// main building, which the constructor takes, and holds the annex as its second target,
    /// which `FUN_140a7a190` takes ([`World::give_second_target`]). Below state 3 the add starts
    /// the Attack ([`World::install_attack`]). Between the install and `FUN_140a7a190` the work
    /// command clears the unit AI's `+0x1d5` (not held: the task unit-ai-1d5-attack-ready)
    /// through a pointer it does not test. A task range other than 0 is not modelled.
    ///
    /// In state 3 or later the add deletes the Attack unstarted: its destructor (`FUN_140a7f640`)
    /// lets the target go and frees it. Only an action object (types 40 to 60, whose slot 0x7e8
    /// is the work command in any state) reaches the work command there, and it holds no unit
    /// AI: the game faults at the `+0x1d5` write before `FUN_140a7a190` reads the freed Attack.
    ///
    /// # Errors
    ///
    /// A task range of its own, and what the aim, the factory, the notify, the install and the
    /// start cannot read or do not model. A unit without a unit AI, where the game writes through
    /// a null pointer ([`WORK_WITHOUT_UNIT_AI`]), and a unit with one whose Attack the install
    /// deleted unstarted, which no caller reaches ([`DELETED_ATTACK_WITH_UNIT_AI`]).
    pub(super) fn start_attack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &Task,
        point: Position,
        notifies: Notifies,
    ) -> GenieResult<()> {
        if task.work_range != Distance::ZERO {
            return Err(GenieError::unsupported(
                "an attack task with a range of its own",
            ));
        }
        let aim = self.attack_aim(target)?;
        let id = self.players.live(player, idx)?.id;
        let constructed_on = attack_task_ai(self.parts.unit_ai.get_mut(id), target, &aim);
        let attack = self.construct_attack(player, idx, constructed_on, task)?;
        self.attack_notifies(player, idx, target, aim.class, point, notifies)?;
        let installed = self.set_only_action(player, idx, Some(attack))?;
        if let Some(attack) = &installed {
            self.install_attack(player, idx, attack)?;
        }
        if self.parts.unit_ai.get(id).is_none() {
            return Err(WORK_WITHOUT_UNIT_AI.with("object", id));
        }
        if installed.is_none() {
            return Err(DELETED_ATTACK_WITH_UNIT_AI.with("object", id));
        }
        self.give_second_target(player, idx, target)
    }

    /// The Attack's constructor (`FUN_140a7c210`) from the attack task's factory, for object
    /// `idx` of `player` on `target`, the object the factory hands it ([`attack_task_ai`]'s
    /// answer): the target, which differs from the new Attack's none, clears the unit's attack
    /// count (`+0x900`), and the Attack takes it (`FUN_140a7a230`, slot 0x178). The Attack holds
    /// the task's sprites and moving sprite ([`arms`]), and the range, the least range, the
    /// target's place and whether the target is the unit's player's as the two stand
    /// ([`Attack::own`]; the start reads the rest of their [`engagement`] anew). The
    /// constructor's slot 0x1f8, whose answer it drops, and `FUN_140c87e70`, a counter, change
    /// nothing. Its own choice of what it takes (an area-effect unit's annex target its main
    /// building, here reached only by a unit without a unit AI, whose factory hands it the
    /// annex; a class 60 none; one past state 6 not taken by `FUN_140a7a230`) is the task
    /// attack-constructor-past-state-6.
    ///
    /// # Errors
    ///
    /// A unit or a target the map does not place, the unit's master and top speed, and a
    /// charge's range ([`engagement`]'s).
    fn construct_attack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &Task,
    ) -> GenieResult<Attack> {
        let id = self.players.live(player, idx)?.id;
        self.parts.combat_mut(id)?.attack_count = AttackCount::ZERO;
        self.take(target, self.releaser(player, idx)?)?;
        let unit = self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        // the task factory (`FUN_140a97e00` case 7) gives the Attack the task's moving sprite
        let arms = arms(
            master,
            &self.model.graphics,
            task.sprites,
            task.sprites.moving,
            &self.model.roles,
        );
        let me_at = self.spatial.get(id).ok_or(ATTACKER_UNPLACED)?;
        let target_at = self.spatial.get_known(target)?.ok_or(TARGET_UNPLACED)?;
        let engagement = engagement(
            self.parts.unit_ai.get(id),
            self.top_speed(player, unit)?,
            master,
            me_at,
            target_at,
            self.world_time,
            self.parts.combat_of(id).next_volley,
        )?;
        Ok(Attack::own(target, &engagement, arms))
    }

    /// The add's part of the install (`FUN_140a977e0` below state 3) of the work command's new
    /// `attack` on object `idx` of `player`: the list holds it as its head, and its slot 0x28
    /// with 1 starts it ([`Unit::start_listed_attack`] with [`HeadStart::Added`]).
    ///
    /// [`Unit::start_listed_attack`]: crate::objects::unit::Unit::start_listed_attack
    ///
    /// # Errors
    ///
    /// The start's.
    fn install_attack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        attack: &Attack,
    ) -> GenieResult<()> {
        self.run_on_step(player, idx, |unit, actions, step| {
            actions.list.set_attack(Some(*attack));
            unit.start_listed_attack(*attack, HeadStart::Added, actions, step)
        })
    }

    /// `FUN_140a7a190` on the work command's started Attack of object `idx` of `player`: the
    /// command's `target` becomes its second target (`+0x28`, `+0x34`). The new Attack held
    /// none, so nothing is let go (slot 0x180), and the target is taken (slot 0x178 with the
    /// unit, `+0x10`).
    ///
    /// # Errors
    ///
    /// An Attack the list no longer holds ([`STARTED_ATTACK_UNLISTED`]), and the take's.
    fn give_second_target(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> GenieResult<()> {
        let id = self.players.live(player, idx)?.id;
        self.parts
            .actions
            .of_mut(id)?
            .list
            .attack_mut()
            .ok_or(STARTED_ATTACK_UNLISTED)?
            .target2 = Some(AttackTargetId::new(target));
        self.take(target, self.releaser(player, idx)?)
    }

    /// Where an attack task on `target` aims (`FUN_141189b20`, which the list's factory calls for
    /// a unit with a unit AI, [`attack_task_ai`]): a building annex at its main
    /// building, anything else at itself; with the target's master's class, and the aimed
    /// object's place and class.
    pub(super) fn attack_aim(&self, target: ObjectId) -> GenieResult<Aim> {
        let (owner, target_unit) = self.object(target).ok_or(GenieError::unsupported(
            "an attack task's target the slice does not hold",
        ))?;
        let tm = target_unit.master(self.master_tables(), owner)?;
        // A building's master type (`'P'`); types below 30 do not move.
        let aimed = if tm.kind == MasterKind::BUILDING {
            self.parts
                .building_of(target_unit.id)
                .annex_of
                .map_or(target, MainBuildingId::id)
        } else {
            target
        };
        let (ap, unit) = self.object(aimed).ok_or(GenieError::unsupported(
            "an attack task's main building is gone",
        ))?;
        Ok(Aim {
            class: tm.class,
            aimed,
            at: unit.position(),
            aimed_class: unit.master(self.master_tables(), ap)?.class,
        })
    }

    /// The attack task's work notify (slot 0x828 with the Attack's type 9, `FUN_14106e4e0`) to the
    /// unit AI of object `idx` of `player`: the last retarget time the world's, then order 700,
    /// action 600, `target` with its master's `class` and the work's `point`, its notifies cleared
    /// with `notifies` ([`UnitAi::notify_work`]), and the order history
    /// ([`World::remember_order`]). A unit without a unit AI hears nothing.
    ///
    /// [`UnitAi::notify_work`]: crate::unit_ai::UnitAi::notify_work
    ///
    /// # Errors
    ///
    /// A unit the world does not hold, and the notify's and the order history's.
    pub(super) fn attack_notifies(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        class: ClassId,
        point: Position,
        notifies: Notifies,
    ) -> GenieResult<()> {
        let world_time = self.world_time;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.last_retarget_time = world_time;
            ai.notify_work(
                Some(OrderKind::Attack),
                Some(AiAction::Attack),
                Some((target, class)),
                point,
                notifies,
                self.streams.get_mut(Stream::Ai),
            )?;
            self.remember_order(
                player,
                idx,
                (Some(OrderKind::Attack), Some(AiAction::Attack)),
            )?;
        }
        Ok(())
    }

    /// The action list's top action's slot 0x48 (`FUN_140a977a0`) on the Hunt `hunt` that the
    /// caller found at the head of the hunter at `idx`: its slot 9 ([`HuntAction::retarget`],
    /// `0x140ac97c0`) on the new `target` of the work command or the hunt command (none for
    /// none) with `point`. The Hunt goes back to the head. A Hunt whose Attack's `MoveTo`
    /// starts arrived shows its idle sprite; one that failed at once the sprite it stands with
    /// (`set_state(13)`'s slot 0x68 with `FUN_140ac5210`'s, [`stand_sprite`]). Then the targets
    /// that changed are let go and taken ([`World::exchange_targets`]).
    ///
    /// # Errors
    ///
    /// What [`World::hunt_new_target`] cannot read; a hunter without its mover; what the Hunt's
    /// walks and Attack do not model.
    ///
    /// [`HuntAction::retarget`]: crate::gathering::hunt::HuntAction::retarget
    pub(super) fn retarget_hunt(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        mut hunt: HuntAction,
        target: Option<ObjectId>,
        point: Position,
    ) -> GenieResult<()> {
        let new = match target {
            Some(target) => Some(self.hunt_new_target(player, idx, target)?),
            None => None,
        };
        let arms = self.arms(player, idx, &hunt.task)?;
        let by = self.releaser(player, idx)?;
        let unit = self.players.live(player, idx)?;
        let top_speed = self.top_speed_terms(player, unit)?;
        // FUN_140ac5210 reads the unit's load, which the retarget leaves as it is
        let stand = stand_sprite(
            &hunt.task,
            unit.master(self.master_tables(), player)?.standing_sprite,
            unit.life.held(),
            &self.model.graphics,
        );
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            top_speed,
            WalkSprites {
                every: MasterTables::new(&self.model.masters, &self.parts.own_master),
                player,
                graphics: &self.model.graphics,
                look: self.parts.combat_of(unit.id).abilities.look.as_ref(),
            },
            &self.model.roles,
        )?;
        let mover = self
            .parts
            .motion
            .mover_mut(unit.id)
            .ok_or(HUNTER_UNMOVING)?;
        let old = hunt.targets();
        let retargeted = hunt.retarget(
            HuntRetarget {
                target: new.as_ref(),
                point,
                player,
                walks,
            },
            arms,
            mover,
            BeginLayer {
                roles: &self.model.roles,
                attack_tuning: &self.model.tuning.attack,
                move_to_tuning: &self.model.tuning.move_to,
            },
            &self.model.tuning.hunt,
        );
        self.parts
            .actions
            .of_mut(unit.id)?
            .list
            .set_hunt(Some(hunt));
        match retargeted? {
            Retargeted::Started(MoveStart::Arrived) => show_arrived(
                &arms,
                &mut unit.sprite,
                self.parts.motion.layer_mut(unit.id),
                self.streams.get_mut(Stream::Sprite),
                self.model.tuning.sprite,
            )?,
            Retargeted::Failed => {
                if let Some(graphic) = stand {
                    new_sprite(
                        &mut unit.sprite,
                        self.parts.move_stats_mut(unit.id),
                        graphic,
                        self.streams.get_mut(Stream::Sprite),
                        self.model.tuning.sprite,
                    );
                }
            }
            Retargeted::Kept | Retargeted::Started(MoveStart::UnderWay) => {}
        }
        self.exchange_targets(old, hunt.targets(), by)
    }

    /// What the Hunt's slot 9 reads of the new `target` of the hunter at `idx` ([`NewTarget`]):
    /// the target ([`World::quarry`]), the hunter's engagement with it, and whether the target's
    /// own master (`+0x58`, not an annex's main building's) is one of the hunter's master's drop
    /// sites (`FUN_14109d4f0`).
    ///
    /// # Errors
    ///
    /// A target the world does not hold or the hunter cannot reach; a building that the load
    /// could not read whole (its [`Unread::error`]): slot 9 reads its action list's top action.
    ///
    /// [`NewTarget`]: crate::gathering::hunt::NewTarget
    /// [`Unread::error`]: crate::world::Unread::error
    fn hunt_new_target(
        &self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> GenieResult<NewTarget> {
        let quarry = self.quarry(player, target)?;
        if quarry.kind == MasterKind::BUILDING
            && let Some(unread) = self.parts.unread.get(target)
        {
            return Err(unread.error());
        }
        let target_master = self
            .object(target)
            .map(|(_, target_unit)| target_unit.life.master_id);
        let drops_here = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .action_part()
            .drop_sites
            .iter()
            .any(|&master_id| Some(master_id) == target_master);
        Ok(NewTarget {
            quarry,
            engagement: self.engagement(player, idx, target)?,
            drops_here,
        })
    }

    /// An action's slot 0x68 base (`FUN_140a7a230`) for the unit at `idx`: its target `old`
    /// becomes `new`. The same target changes nothing. A different old target the
    /// world still holds is let go (slot 0x180, [`World::release`]) unless the action is a
    /// quiet sub-action (`voice`, `+0x60`); a new target in state 7 or more is not taken, and
    /// the action is left without one; else it is taken (slot 0x178 with the action's type,
    /// [`World::take`]) unless quiet. Returns the action's target (`+0x30`) after it.
    pub(crate) fn retarget_action(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        old: Option<ObjectId>,
        new: ObjectId,
        voice: Voice,
    ) -> GenieResult<Option<ObjectId>> {
        if old == Some(new) {
            return Ok(old);
        }
        if let Some(old) = old
            && voice.is_loud()
            && self.object(old).is_some()
        {
            let by = self.releaser(player, idx)?;
            self.release(old, by)?;
        }
        let Some((_, unit)) = self.object(new) else {
            return Ok(None);
        };
        if LAST_TAKEN < unit.life.state {
            return Ok(None);
        }
        if voice.is_loud() {
            self.take(new, self.releaser(player, idx)?)?;
        }
        Ok(Some(new))
    }
}
