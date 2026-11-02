//! A unit's stops: its actions torn down or cleared, its standing sprite, an action's effects
//! applied.

use crate::GenieError;
use crate::math::rng::{Stream, Well512};
use crate::objects::Graphics;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::layers::MotionMut;
use crate::objects::masters::ClassId;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMasters;
use crate::objects::release::Releaser;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::orders::Work;
use crate::unit_ai::{AiCommand, Effect};
use crate::world::Unread;
use crate::world::lets::torn_down;
use crate::world::step::Step;

impl Unit {
    /// stopAction (slot 0x3c0, `FUN_14108c160`): the action list goes ([`Unit::clear_own_actions`])
    /// and a unit in state 2 takes its standing sprite (slot 0x68 with slot 0x798's,
    /// [`crate::objects::sprite::new_sprite`]) where the slice holds its sprite.
    pub(crate) fn stop_action(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let by = self.releaser(c)?;
        let sprite = c.w.streams.get(Stream::Sprite);
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        self.stop_with(
            (
                actions,
                c.w.parts.unread.get(self.id),
                c.w.parts.motion.layer_mut(self.id),
            ),
            (&mut releases, &mut leaves),
            by,
            OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
            &c.w.model.graphics,
            sprite,
        )?;
        torn_down(c, self, (leaves, releases))
    }

    /// [`Unit::stop_action`] on the parts of the world it changes: the targets its actions let
    /// go and the foundations its Build leaves (`by` the unit, [`Unit::clear_own_actions`]), its
    /// masters and sprites, and the Sprite stream; `actions` and `motion` are its action and
    /// motion layers, `unread` why the load could not read it whole ([`Unread`]).
    pub(crate) fn stop_with(
        &mut self,
        (actions, unread, mut motion): (&mut Action, Option<&Unread>, MotionMut<'_>),
        (releases, builder_leaves): (&mut Vec<(ObjectId, Releaser)>, &mut Vec<ObjectId>),
        by: Releaser,
        masters: OwnerMasters<'_>,
        graphics: &Graphics,
        sprite: &mut Well512,
    ) -> Result<(), GenieError> {
        self.tear_down_actions(
            (actions, unread),
            motion.reborrow(),
            releases,
            builder_leaves,
            by,
        )?;
        if self.life.state == ObjectState::Alive {
            self.show_standing(motion, masters, graphics, sprite)?;
        }
        Ok(())
    }

    /// The action list's clearing (`FUN_140a98620`) as the unit's own commands and stops run it
    /// ([`Unit::clear_own_actions`]) on its action layer `actions`; an action the slice does not
    /// hold has a teardown it cannot run (`unread`: the load could not read it whole,
    /// [`Unread`]).
    pub(crate) fn tear_down_actions(
        &self,
        (actions, unread): (&mut Action, Option<&Unread>),
        motion: MotionMut<'_>,
        releases: &mut Vec<(ObjectId, Releaser)>,
        builder_leaves: &mut Vec<ObjectId>,
        by: Releaser,
    ) -> Result<(), GenieError> {
        if unread.is_some() {
            return Err(GenieError::unsupported(
                "a unit AI's stop of an action list the slice does not model",
            ));
        }
        self.clear_own_actions(actions, motion, releases, builder_leaves, by)
    }

    /// The unit's slot 0x68 with its master's standing sprite (slot 0x798's, the master's
    /// `+0x470`): its top speed becomes the standing sprite's ([`Mover::shows_standing`]) and,
    /// where the slice holds its sprite, the sprite starts ([`crate::objects::sprite::new_sprite`] on the
    /// Sprite stream). Its master is its own or its owner's (`masters`).
    ///
    /// [`Mover::shows_standing`]: crate::movement::Mover::shows_standing
    pub(crate) fn show_standing(
        &mut self,
        mut motion: MotionMut<'_>,
        masters: OwnerMasters<'_>,
        graphics: &Graphics,
        sprite: &mut Well512,
    ) -> Result<(), GenieError> {
        let standing = self
            .master_in(masters)?
            .standing_sprite
            .and_then(|s| graphics.get(s))
            .copied();
        if let Some(m) = motion.mover_mut() {
            m.shows(standing.as_ref());
        }
        if self.sprite.is_some() {
            let standing = standing.ok_or(GenieError::unsupported(
                "a stopped unit without its standing sprite",
            ))?;
            new_sprite(&mut self.sprite, motion.move_stats_mut(), standing, sprite);
        }
        Ok(())
    }

    /// The action list's deletion (`FUN_140a98620`, [`ActionList::delete_all`]), the one every
    /// caller shares (the unit's death, its stops and commands,
    /// [`crate::world::World::clear_actions`]): each action's destructor
    /// ([`crate::objects::actions::Action::tear_down`]) in the list's order. The targets they let
    /// go go into `releases` with the unit as `by`, the buildings a counted builder or repairer
    /// leaves into `builder_leaves` ([`crate::world::World::leave_builders`]). A building's
    /// production is a list of its own, which stays.
    ///
    /// [`ActionList::delete_all`]: crate::objects::actions::ActionList::delete_all
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model, and the list's deletion's.
    pub(crate) fn clear_own_actions(
        &self,
        actions: &mut Action,
        motion: MotionMut<'_>,
        releases: &mut Vec<(ObjectId, Releaser)>,
        builder_leaves: &mut Vec<ObjectId>,
        by: Releaser,
    ) -> Result<(), GenieError> {
        let torn = actions.list.delete_all(motion, self.life.state)?;
        releases.extend(torn.releases.into_iter().map(|t| (t, by)));
        builder_leaves.extend(torn.builder_leaves);
        Ok(())
    }

    /// `FUN_140a98e20` on the unit's own list ([`ActionList::delete_head`]): the head's
    /// destructor, what it lets go applied at once
    /// ([`crate::objects::teardown::TornDown::let_go`]).
    ///
    /// [`ActionList::delete_head`]: crate::objects::actions::ActionList::delete_head
    ///
    /// # Errors
    ///
    /// The deletion's, and what it lets go.
    pub(crate) fn delete_own_head(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let by = self.releaser(c)?;
        let torn = actions
            .list
            .delete_head(c.w.parts.motion_mut(self.id), self.life.state)?;
        torn.let_go(c, self, by)
    }

    /// A call its unit AI makes on it ([`Effect`]), at once where the AI makes it
    /// ([`crate::unit_ai::UnitSide::effect_now`]: the unit holds its AI meanwhile); `class` is its
    /// master's, which a move reads. An order on a target runs on the unit lent back to its slot
    /// ([`World::order_on_target`]).
    ///
    /// # Errors
    ///
    /// The call's.
    ///
    /// [`World::order_on_target`]: crate::world::World::order_on_target
    pub(crate) fn run_effect(
        &mut self,
        actions: &mut Action,
        e: Effect,
        class: Option<ClassId>,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        match e {
            Effect::Stop => self.stop_action(actions, c),
            Effect::CapHumanOrder(cap) => {
                self.last_human_order = self.last_human_order.min(cap);
                Ok(())
            }
            Effect::Move(_) | Effect::RestorePatrol => {
                let class = class.ok_or(GenieError::unsupported(
                    "an AI move of a master the slice does not have",
                ))?;
                let m = match e {
                    Effect::Move(m) => Some(m),
                    _ => Self::restore_patrol(
                        c.w.parts.unit_ai.get(self.id),
                        c.w.parts.motion.mover_mut(self.id),
                    ),
                };
                m.map_or(Ok(()), |m| self.ai_move(actions, m, class, c))
            }
            Effect::OrderOnTarget(order) => c.lend_at((&mut *self, &mut *actions), |w, p, i| {
                w.order_on_target(p, i, order)
            }),
            Effect::Command(command) => self.ai_command(actions, command, c),
            Effect::ChangeOwner(new) => self.ai_change_owner(actions, new, c),
            Effect::Explore(point) => self.explore_to(actions, point, c),
            Effect::MoveToObject {
                target,
                state,
                range,
            } => self.move_to_object(actions, (target, state), range, c),
        }
    }

    /// The unit's own command its unit AI gives ([`AiCommand`]), at once on the unit lent back
    /// to its slot ([`Step::lend_at`]): a move off a footprint, `HuntObject`'s hunt command,
    /// `GatherObject`'s gather command, a work (slot 0x7e8), `BuildObject`'s build
    /// command or a drop elsewhere.
    ///
    /// # Errors
    ///
    /// The command's and the lend's.
    fn ai_command(
        &mut self,
        actions: &mut Action,
        command: AiCommand,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        match command {
            AiCommand::MoveAside(m) => {
                c.lend_at((&mut *self, &mut *actions), |w, p, i| w.move_aside(p, i, m))
            }
            AiCommand::HuntObject(target) => c.lend_at((&mut *self, &mut *actions), |w, p, i| {
                w.hunt_command(p, i, target)
            }),
            AiCommand::GatherObject(target) => c.lend_at((&mut *self, &mut *actions), |w, p, i| {
                w.gather_command(p, i, target)
            }),
            AiCommand::WorkOn { target, point } => {
                let work = Work {
                    target: Some(target),
                    point,
                    carry_test: true,
                    filter: None,
                    notified: false,
                };
                c.lend_at((&mut *self, &mut *actions), |w, p, i| {
                    w.order_work(p, i, &work)
                })
            }
            AiCommand::BuildObject(target) => c.lend_at((&mut *self, &mut *actions), |w, p, i| {
                w.build_command(p, i, target).map(drop)
            }),
            AiCommand::DropElsewhere { site, back } => c
                .lend_at((&mut *self, &mut *actions), |w, p, i| {
                    w.drop_elsewhere(p, i, site, back)
                }),
        }
    }
}
