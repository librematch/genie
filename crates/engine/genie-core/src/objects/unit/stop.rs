//! A unit's stops: its actions torn down or cleared, its standing sprite, an action's effects
//! applied.

use crate::math::rng::{Stream, Well512};
use crate::movement::Speed;
use crate::objects::Graphics;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::actions::Action as ActionsAction;
use crate::objects::layers::Action;
use crate::objects::layers::MotionMut;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::release::Releaser;
use crate::objects::sprite::Cargo;
use crate::objects::sprite::Graphic;
use crate::objects::sprite::{new_sprite, standing};
use crate::objects::teardown::{NextHead, admit};
use crate::objects::unit::STOPPED_SPRITE_LACKING;
use crate::objects::unit::Unit;
use crate::orders::Work;
use crate::unit_ai::{AiCommand, Answer, Effect, GatheringCommand};
use crate::world::layer::Roles;
use crate::world::layer::generated::SpriteTuning;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// stopAction (slot 0x3c0, `FUN_14108c160`, the combat objects' and the missiles'): the
    /// action list goes ([`Unit::tear_down_actions`], with the target of the unit AI the world
    /// holds: the AI's calls return it there,
    /// [`crate::unit_ai::world_view::UnitView::effect_now`]), then a unit in state 2 shows slot
    /// 0x798's sprite ([`Unit::standing_shown`]) with slot 0x68 ([`Unit::show_graphic_standing`]).
    /// The debug log around them (`DAT_143f09029`) reads the world time and changes nothing.
    ///
    /// # Errors
    ///
    /// The list's deletion's and the standing sprite's.
    pub(crate) fn stop_action(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let by = self.releaser(step)?;
        let ai_target = step.world.ai_target(self.id);
        self.tear_down_actions(actions, step, ai_target, by)?;
        if self.life.state == ObjectState::Alive {
            let standing = self.standing_shown(step)?;
            self.show_graphic_standing(
                step.world.parts.motion.layer_mut(self.id),
                standing,
                step.world.streams.get_mut(Stream::Sprite),
                step.world.model.tuning.sprite,
            )?;
        }
        Ok(())
    }

    /// Slot 0x798 of the unit, its standing sprite where the slice's graphics hold it: a TRIBE
    /// combat object's or a building's (types 70 and 80, `FUN_1412029f0`, [`standing`]) is its
    /// terrain look's, else its master's (`+0x470`); any other object's (`FUN_141188900`) is its
    /// master's alone. A trade cart's goods (`+0x7a0`) are not held: it counts as empty
    /// ([`Cargo::Empty`]), as every other read of slot 0x798 does.
    ///
    /// # Errors
    ///
    /// A master the slice does not have.
    fn standing_shown(&self, step: &Step<'_>) -> GenieResult<Option<Graphic>> {
        let master = self.master_in(OwnerMasters::new(
            &step.world.parts.own_master,
            step.world.model.masters.of(step.player),
        ))?;
        let sprite_id = if MasterKind::COMBAT <= master.kind {
            let look = step.world.parts.combat_of(self.id).abilities.look.as_ref();
            standing(master, look, Cargo::Empty, &step.world.model.roles)
        } else {
            master.standing_sprite
        };
        Ok(sprite_id
            .and_then(|sprite_id| step.world.model.graphics.get(sprite_id))
            .copied())
    }

    /// The action list's deletion (`FUN_140a98620`) as the unit's own commands and stops run it
    /// ([`Unit::clear_own_actions`]) on its action layer `actions`, the target `ai_target` of
    /// its unit AI (which its top speed reads) and the unit as `by`. A unit the load could not
    /// read whole stops the run with the import's error ([`Unread::error`]): the slice does not
    /// hold the list the game deletes.
    ///
    /// [`Unread::error`]: crate::world::Unread::error
    ///
    /// # Errors
    ///
    /// A unit the load could not read whole, and the deletion's.
    pub(crate) fn tear_down_actions(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        ai_target: Option<ObjectId>,
        by: Releaser,
    ) -> GenieResult<()> {
        if let Some(unread) = step.world.parts.unread.get(self.id) {
            return Err(unread.error());
        }
        self.clear_own_actions(actions, step, ai_target, by)
    }

    /// The unit's slot 0x68 with its master's standing sprite (its `+0x470`, which the Repair's
    /// and the Transport's `set_state` read themselves, not through slot 0x798), its master its
    /// own or its owner's (`masters`): [`Unit::show_graphic_standing`]. A routine that calls slot
    /// 0x798 shows that slot's sprite instead ([`Unit::standing_shown`]).
    ///
    /// # Errors
    ///
    /// A master the slice does not have, and a sprite the slice holds without its standing one.
    pub(crate) fn show_standing(
        &mut self,
        motion: MotionMut<'_>,
        masters: OwnerMasters<'_>,
        graphics: &Graphics,
        sprite: &mut Well512,
        sprite_tuning: SpriteTuning,
    ) -> GenieResult<()> {
        let standing = self
            .master_in(masters)?
            .standing_sprite
            .and_then(|sprite_id| graphics.get(sprite_id))
            .copied();
        self.show_graphic_standing(motion, standing, sprite, sprite_tuning)
    }

    /// The unit's slot 0x68 with the standing sprite `standing` (none where the slice's graphics
    /// lack it): its top speed becomes the sprite's ([`MoveToAction::shows_standing`]) and,
    /// where the slice holds its sprite, the sprite starts ([`new_sprite`] on the Sprite stream).
    ///
    /// [`MoveToAction::shows_standing`]: crate::movement::move_to::MoveToAction::shows_standing
    ///
    /// # Errors
    ///
    /// A sprite the slice holds without the standing one.
    fn show_graphic_standing(
        &mut self,
        mut motion: MotionMut<'_>,
        standing: Option<Graphic>,
        sprite: &mut Well512,
        sprite_tuning: SpriteTuning,
    ) -> GenieResult<()> {
        if let Some(mover) = motion.mover_mut() {
            mover.shows(standing.as_ref());
        }
        if self.sprite.is_some() {
            let standing = standing.ok_or(STOPPED_SPRITE_LACKING)?;
            new_sprite(
                &mut self.sprite,
                motion.move_stats_mut(),
                standing,
                sprite,
                sprite_tuning,
            );
        }
        Ok(())
    }

    /// The action list's deletion (`FUN_140a98620`) from the unit's step, as its death, its stops
    /// and its commands share it: [`Unit::delete_own_head`] until the list is empty, each
    /// destructor ([`crate::objects::actions::Action::tear_down`]) on the unit's motion layer and
    /// its top speed (slot 0x208, which the Fly's reads) as it stands at that destructor's turn:
    /// a restart before it can show a sprite. The top speed reads `ai_target`, the target of the
    /// unit's AI, which a restart only posts to. What each destructor let go is applied at once
    /// by `by`, then the next head restarts where one is due. A building's production is a list
    /// of its own, which stays.
    ///
    /// # Errors
    ///
    /// The deletions' and the top speed's.
    pub(crate) fn clear_own_actions(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        ai_target: Option<ObjectId>,
        by: Releaser,
    ) -> GenieResult<()> {
        while !actions.list.is_empty() {
            let top = step.world.top_speed_with(step.player, self, ai_target)?;
            self.delete_own_head(actions, step, top, by)?;
        }
        Ok(())
    }

    /// The action list's deletion (`FUN_140a98620`) on the unit's own list
    /// ([`Unit::clear_own_actions`]) with the target of the unit AI the world holds and the unit
    /// as the releaser.
    ///
    /// # Errors
    ///
    /// The deletion's.
    pub(crate) fn delete_own_actions(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let by = self.releaser(step)?;
        let ai_target = step.world.ai_target(self.id);
        self.clear_own_actions(actions, step, ai_target, by)
    }

    /// `FUN_140a98e20` on the unit's own list from its step: the head's destructor
    /// ([`ActionList::delete_head`] on its motion layer and its top speed `top`, slot 0x208 as the
    /// destructor reads it), what it lets go applied at once by `by`
    /// ([`crate::objects::teardown::TornDown::let_go`]: the game's slot 0x180 inside slot 0), then,
    /// with another action at the head of a unit below state 3, that action's slot 0x28 with 0
    /// ([`Unit::restart_head`]).
    ///
    /// [`ActionList::delete_head`]: crate::objects::actions::ActionList::delete_head
    ///
    /// # Errors
    ///
    /// The deletion's, what it lets go and the restart's.
    pub(crate) fn delete_own_head(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        top: Speed,
        by: Releaser,
    ) -> GenieResult<()> {
        let (torn, next) =
            actions
                .list
                .delete_head(step.world.parts.motion_mut(self.id), top, self.life.state)?;
        torn.let_go(step, self, by)?;
        if next == NextHead::Restarts {
            self.restart_head(actions, step)?;
        }
        Ok(())
    }

    /// `set_only_action` (slot 0x808, `FUN_14106e990`) on the unit's own list from its update, as
    /// [`crate::world::World::set_only_action`] outside one: the list's deletion
    /// ([`Unit::clear_own_actions`]), then its add ([`admit`]): the new `action` back to start and
    /// hold below state 3, else deleted unstarted (its destructor reads the top speed after the
    /// deletion), what its destructor left applied at once.
    ///
    /// # Errors
    ///
    /// The deletions' and what they let go.
    pub(crate) fn set_only_own_action<A: Into<ActionsAction>>(
        &mut self,
        actions: &mut Action,
        action: Option<A>,
        step: &mut Step<'_>,
    ) -> GenieResult<Option<A>> {
        let by = self.releaser(step)?;
        let ai_target = step.world.ai_target(self.id);
        self.clear_own_actions(actions, step, ai_target, by)?;
        let Some(action) = action else {
            return Ok(None);
        };
        let top = step.world.top_speed_with(step.player, self, ai_target)?;
        let (action, torn) = admit(
            action,
            step.world.parts.motion_mut(self.id),
            top,
            self.life.state,
        )?;
        torn.let_go(step, self, by)?;
        Ok(action)
    }

    /// A call its unit AI makes on it ([`Effect`]), at once where the AI makes it
    /// ([`crate::unit_ai::world_view::UnitView::effect_now`]: the unit holds its AI meanwhile);
    /// `class` is its master's, which a move reads. An order on a target runs on the unit lent back
    /// to its slot ([`World::order_on_target`]).
    ///
    /// # Errors
    ///
    /// The call's.
    ///
    /// [`World::order_on_target`]: crate::world::World::order_on_target
    pub(crate) fn run_effect(
        &mut self,
        actions: &mut Action,
        effect: Effect,
        class: Option<ClassId>,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        match effect {
            Effect::Stop => self.stop_action(actions, step),
            Effect::CapHumanOrder(cap) => {
                self.last_human_order = self.last_human_order.min(cap);
                Ok(())
            }
            Effect::Move(_) | Effect::RestorePatrol => {
                let class = class.ok_or(GenieError::unsupported(
                    "an AI move of a master the slice does not have",
                ))?;
                let moves = match effect {
                    Effect::Move(ai_move) => Some(ai_move),
                    _ => Self::restore_patrol(
                        step.world.parts.unit_ai.get(self.id),
                        step.world.parts.motion.mover_mut(self.id),
                        &step.world.model.tuning.movement,
                    ),
                };
                moves.map_or(Ok(()), |ai_move| {
                    self.ai_move(actions, ai_move, class, step, roles)
                })
            }
            Effect::OrderOnTarget(order) => step.lend_at(
                (&mut *self, &mut *actions),
                |world, player_id, list_index| world.order_on_target(player_id, list_index, order),
            ),
            Effect::Command(command) => self.ai_command(actions, command, step),
            Effect::ChangeOwner(new) => self.ai_change_owner(actions, new, step, roles),
            Effect::Explore { point, force } => self.explore_to(actions, point, force, step, roles),
            Effect::MoveToObject {
                target,
                state,
                range,
            } => self.move_to_object(actions, (target, state), range, step, roles),
        }
    }

    /// The unit's own command its unit AI gives ([`AiCommand`]), at once on the unit lent back
    /// to its slot ([`Step::lend_at`]): a move off a footprint, a work (slot 0x7e8),
    /// `BuildObject`'s build command or a drop elsewhere.
    ///
    /// # Errors
    ///
    /// The command's and the lend's.
    fn ai_command(
        &mut self,
        actions: &mut Action,
        command: AiCommand,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        match command {
            AiCommand::MoveAside(ai_move) => step.lend_at(
                (&mut *self, &mut *actions),
                |world, player_id, list_index| world.move_aside(player_id, list_index, ai_move),
            ),
            AiCommand::WorkOn { target, point } => {
                let work = Work::on(Some(target), point);
                step.lend_at(
                    (&mut *self, &mut *actions),
                    |world, player_id, list_index| world.order_work(player_id, list_index, &work),
                )
            }
            AiCommand::WorkAt { target, point } => step.lend_at(
                (&mut *self, &mut *actions),
                |world, player_id, list_index| {
                    world.work_at(player_id, list_index, Some(target), point)
                },
            ),
            AiCommand::BuildObject(target) => step.lend_at(
                (&mut *self, &mut *actions),
                |target_world, target_player_id, target_list_index| {
                    target_world
                        .build_command(target_player_id, target_list_index, target)
                        .map(drop)
                },
            ),
            AiCommand::DropElsewhere { site, at, back } => step.lend_at(
                (&mut *self, &mut *actions),
                |world, player_id, list_index| {
                    world.drop_elsewhere(player_id, list_index, (site, at), back)
                },
            ),
        }
    }

    /// The unit's gather or hunt command its unit AI's `GatherObject` or `HuntObject` gives
    /// ([`GatheringCommand`]), at once on the unit lent back to its slot ([`Step::lend_at`],
    /// [`World::gather_command`], [`World::hunt_command`]): taken when the unit takes it (the
    /// slot's 1).
    ///
    /// # Errors
    ///
    /// The command's and the lend's.
    ///
    /// [`World::gather_command`]: crate::world::World::gather_command
    /// [`World::hunt_command`]: crate::world::World::hunt_command
    pub(crate) fn gathering_command(
        &mut self,
        actions: &mut Action,
        command: GatheringCommand,
        step: &mut Step<'_>,
    ) -> GenieResult<Answer> {
        step.lend_at(
            (&mut *self, &mut *actions),
            |world, player_id, list_index| match command {
                GatheringCommand::Gather { target, force } => {
                    world.gather_command(player_id, list_index, target, force)
                }
                GatheringCommand::Hunt { target, force } => {
                    world.hunt_command(player_id, list_index, target, force)
                }
            },
        )
    }
}
