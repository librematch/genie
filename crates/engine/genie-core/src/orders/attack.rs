//! An attack's start, a hunt's and an action's retarget.

use crate::GenieError;
use crate::combat::attack::{Attack, AttackReads, Fight};
use crate::construction::MainBuildingId;
use crate::gathering::hunt::HuntRetarget;
use crate::map::Distance;
use crate::map::Position;
use crate::map::Tile;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::Voice;
use crate::objects::masters::ClassId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::master_type::BUILDING;
use crate::players::PlayerId;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlots;

use crate::combat::attack::{AttackCommand, ReadyReads, arms, engagement, show_arrived};
use crate::combat::victims::victim_of;
use crate::combat::{AttackCount, AttackTargetId};
use crate::gathering::HuntTargetId;
use crate::objects::masters::{Master, MasterTables, OwnerMasters, Task};
use crate::objects::sprite::new_sprite;
use crate::unit_ai::ActionKind;
use crate::unit_ai::Notify;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::{ATTACKER_UNPLACED, NO_STANDING_SPRITE, TARGET_UNPLACED};

/// Where an attack task's target aims ([`World::attack_aim`]).
pub(super) struct Aim {
    /// The target's owner.
    pub(super) owner: PlayerId,
    /// The target's master's class and fog visibility.
    pub(super) class: ClassId,
    pub(super) fog_visibility: FogVisibility,
    /// The object aimed at (the target, or a building annex's main building), its place and its
    /// master's class.
    pub(super) aimed: ObjectId,
    pub(super) at: Position,
    pub(super) aimed_class: ClassId,
}

impl World {
    /// What the attack's first update reads of the attacker at `idx` of player `player` and of
    /// its `target` as combat objects; none when the world cannot read the target.
    fn attack_ready(
        &self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        (master, world_time): (&Master, WorldTime),
    ) -> Result<Option<ReadyReads>, GenieError> {
        let u = self.players.live(player, idx)?;
        let held = self.players.player(player)?;
        let reads = AttackReads::of(
            self.parts.combat_of(u.id),
            master,
            (
                OwnerMasters::new(&self.parts.own_master, self.masters.of(player)),
                &self.graphics,
            ),
            self.parts.unit_ai.get(u.id),
            (held.kind, world_time),
        );
        let own = victim_of(
            (
                u,
                self.parts.unit_ai.contains(u.id),
                self.parts.building_of(u.id),
                self.parts.combat_of(u.id),
                self.parts.actions.of(u.id),
                self.parts.unread.get(u.id),
            ),
            master,
            &held.attributes,
            player,
            self.spatial.terrain.as_ref(),
            self.mode_680,
        );
        Ok(self.victim(target, player)?.map(|victim| ReadyReads {
            reads,
            fight: Fight { victim, own },
        }))
    }

    /// The work command's Attack (its task of action type 7, the factory `FUN_140a97e00` case 7):
    /// the unit AI takes action 600 (`FUN_140c64b50`) and the target (an annex its main building,
    /// `FUN_141189b20`) at its place (`FUN_140c64c10`); the Attack (`FUN_140a7c210`, sub-action 0:
    /// the task's sprites, the task's range or slot 0x238's for 0) takes its target (slot 0x178);
    /// the notify (`FUN_14106e4e0` kind 9: the last retarget time `+0xbc` takes the world's time,
    /// then order 700, action 600, the target with its class and the work's point); the install
    /// clears the action list and starts the Attack (slot 5, `0x140a7f8c0`: `+0x900` cleared, the
    /// standing sprite, the range test of [`Attack::command`]; a unit that cannot move in reach
    /// goes ready, `set_state(6)`, [`Attack::enter_ready`]; one that may move ends in
    /// `set_state(4)`, whose `MoveTo` starting arrived shows the standing sprite at its speed and
    /// clears `+0x7b8`, `0x140aa29e0`); then the Attack takes the target as
    /// its second target too (`FUN_140a7a190`, slot 0x178). The unit AI's `+0x1d5`, cleared, is
    /// not held; a task range other than 0 is not modelled.
    pub(super) fn start_attack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &Task,
        point: Position,
        clears: bool,
    ) -> Result<(), GenieError> {
        if task.work_range != Distance::ZERO {
            return Err(GenieError::unsupported(
                "an attack task with a range of its own",
            ));
        }
        let aim = self.attack_aim(target)?;
        let world_time = self.world_time;
        self.attack_notifies(player, idx, (target, &aim), point, clears)?;
        self.clear_actions(player, idx)?;
        let master = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?;
        // the task factory (`FUN_140a97e00` case 7) gives the Attack the task's moving sprite
        let arms = arms(master, &self.graphics, task.sprites, task.sprites.moving);
        let standing = arms.idle_sprite.ok_or(NO_STANDING_SPRITE)?;
        let me_at = *self
            .spatial
            .get(self.players.live(player, idx)?.id)
            .ok_or(ATTACKER_UNPLACED)?;
        let target_at = *self.spatial.get_known(target)?.ok_or(TARGET_UNPLACED)?;
        let (tx, ty) = (target_at.position.x() as i32, target_at.position.y() as i32);
        let visible = self.spatial.sees_tile(player, Tile::new(tx, ty))
            || (aim.fog_visibility != FogVisibility::INVISIBLE
                && self.spatial.explored(player, Tile::new(tx, ty))?);
        let ready = self.attack_ready(player, idx, target, (master, world_time))?;
        let speed = self
            .parts
            .mover_of(self.players.live(player, idx)?.id)
            .map_or(Speed::ZERO, |m| m.stats.speed);
        let e = engagement(
            self.parts.unit_ai.get(self.players.live(player, idx)?.id),
            speed,
            master,
            &me_at,
            &target_at,
            world_time,
        )?;
        let unit = self.players.live_mut(player, idx)?;
        if unit.sprite.is_none() {
            return Err(GenieError::unsupported(
                "an attack task of a unit whose sprite the slice does not hold",
            )
            .with("object", unit.id)
            .with("master", unit.life.master_id));
        }
        self.parts.combat_mut(unit.id)?.attack_count = AttackCount::ZERO;
        new_sprite(
            &mut unit.sprite,
            self.parts.move_stats_mut(unit.id),
            standing,
            self.streams.get(Stream::Sprite),
        );
        let cmd = AttackCommand {
            target,
            engagement: &e,
            arms,
            visible,
            target_owner: aim.owner,
            ready,
        };
        self.install_attack(player, idx, target, &cmd)
    }

    /// The Attack's start on object `idx` of `player` ([`Attack::command`] with `cmd`): the
    /// start's notifies go to the unit AI, the Attack takes `target` as its second target too
    /// (`FUN_140a7a190`) and goes on the action list, and the unit takes the target (slot 0x178).
    fn install_attack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        cmd: &AttackCommand<'_>,
    ) -> Result<(), GenieError> {
        let (mut attack, posted) =
            self.run_on_step(player, idx, |unit, _, c| Attack::command(c, unit, cmd))?;
        // the start's notifies go to the unit AI (the unit's slot 0x328 -> the AI's slot 0x38)
        let unit = self.players.live_mut(player, idx)?;
        let (id, state) = (unit.id, unit.life.state);
        for (kind, p) in posted {
            if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
                ai.post(Notify::own(id, kind, p), state)?;
            }
        }
        // the attack command gives the Attack its second target
        attack.target2 = Some(AttackTargetId::new(target));
        self.parts
            .actions
            .of_mut(unit.id)?
            .list
            .set_attack(Some(attack));
        self.take(target, self.releaser(player, idx)?)?;
        Ok(())
    }

    /// Where an attack task on `target` aims (`FUN_141189b20`): a building annex at its main
    /// building, anything else at itself; with the target's owner, its master's class and fog
    /// visibility, and the aimed object's place and class.
    pub(super) fn attack_aim(&self, target: ObjectId) -> Result<Aim, GenieError> {
        let (owner, t) = self.object(target).ok_or(GenieError::unsupported(
            "an attack task's target the slice does not hold",
        ))?;
        let tm = t.master(self.master_tables(), owner)?;
        // A building's master type (`'P'`); types below 30 do not move.
        let aimed = if tm.kind == BUILDING {
            self.parts
                .building_of(t.id)
                .annex_of
                .map_or(target, MainBuildingId::id)
        } else {
            target
        };
        let (ap, a) = self.object(aimed).ok_or(GenieError::unsupported(
            "an attack task's main building is gone",
        ))?;
        Ok(Aim {
            owner,
            class: tm.class,
            fog_visibility: tm.fog_visibility,
            aimed,
            at: a.position(),
            aimed_class: a.master(self.master_tables(), ap)?.class,
        })
    }

    /// The attack task's word to the unit AI of object `idx` of `player`: action 600 and the
    /// aimed object at its place (`FUN_140c64b50`, `FUN_140c64c10`); the target taken (slot
    /// 0x178); the last retarget time the world's; the work notify (`FUN_14106e4e0` kind 9:
    /// order 700, action 600, the target with its class and the work's `point`, its notifies
    /// cleared with `clears`) and the order history.
    pub(super) fn attack_notifies(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, aim): (ObjectId, &Aim),
        point: Position,
        clears: bool,
    ) -> Result<(), GenieError> {
        let world_time = self.world_time;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.set_action(Some(ActionKind::Attack));
            ai.set_target(Some(aim.aimed), aim.aimed_class.or_none(), aim.at);
        }
        self.take(target, self.releaser(player, idx)?)?;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.last_retarget_time = world_time;
            ai.notify_work(
                Some(OrderKind::Attack),
                Some(ActionKind::Attack),
                Some((target, aim.class)),
                point,
                clears,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(
                player,
                idx,
                (Some(OrderKind::Attack), Some(ActionKind::Attack)),
            )?;
        }
        Ok(())
    }

    /// The hunt's slot 9 (`0x140ac97c0`) on the work command's new target: a new target is
    /// released for the old one (slot 0x68, `FUN_140ac7980`, through `FUN_140a7a230`) and the hunt
    /// starts over.
    pub(super) fn retarget_hunt(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> Result<(), GenieError> {
        let quarry = self.quarry(player, target)?;
        let engagement = self.engagement(player, idx, target)?;
        let task = self
            .parts
            .actions_of(self.players.live(player, idx)?.id)
            .list
            .hunt()
            .map(|h| h.task)
            .ok_or(GenieError::unsupported(
                "a hunt's retarget without its hunt",
            ))?;
        let arms = self.arms(player, idx, &task)?;
        // FUN_14109d4f0: the target's master is one of the unit's master's drop sites
        let target_master = self.object(target).map(|(_, t)| t.life.master_id);
        let drops_here = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .drop_sites
            .iter()
            .any(|&d| Some(d) == target_master);
        let unit = self.players.live_mut(player, idx)?;
        let old = self
            .parts
            .actions_of(unit.id)
            .list
            .hunt()
            .map(|h| h.target)
            .filter(|&t| t.is_some());
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            MasterTables::new(&self.masters, &self.parts.own_master),
            player,
            (
                &self.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
        )?;
        let (Some(hunt), Some(mover)) = (
            self.parts.actions.of_mut(unit.id)?.list.hunt_mut(),
            self.parts.motion.mover_mut(unit.id),
        ) else {
            return Err(GenieError::unsupported(
                "a hunter without its hunt or movement state",
            ));
        };
        let unit_player = player;
        if let Some(arrived) = hunt.retarget(
            HuntRetarget {
                target: &quarry,
                player: unit_player,
                walks,
                drops_here,
            },
            &engagement,
            arms,
            mover,
        )? {
            if arrived {
                show_arrived(
                    &arms,
                    &mut unit.sprite,
                    self.parts.motion.layer_mut(unit.id),
                    self.streams.get(Stream::Sprite),
                )?;
            }
            // the hunt's slot 0x68 (FUN_140ac7980): FUN_140a7a230 on the new target, which the
            // work command's task finder took below state 3
            let old = old.flatten().map(HuntTargetId::id);
            let _ = self.retarget_action(player, idx, old, target, Voice::Loud)?;
        }
        Ok(())
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
    ) -> Result<Option<ObjectId>, GenieError> {
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
        let Some((_, t)) = self.object(new) else {
            return Ok(None);
        };
        if ObjectState::Removed <= t.life.state {
            return Ok(None);
        }
        if voice.is_loud() {
            self.take(new, self.releaser(player, idx)?)?;
        }
        Ok(Some(new))
    }
}
