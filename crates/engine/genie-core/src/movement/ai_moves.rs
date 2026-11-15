//! A unit's moves its unit AI asks: to a point (slot 0x340, its answer, a trebuchet's Pack in
//! its place, the head action's slot 0x40 may take the point) and to an object (slot 0x348), a
//! move that gave up, and the standing sprite of a `MoveTo` that ends at its start.

use crate::map::Distance;
use crate::map::Position;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::move_to::Patience;
use crate::objects::ObjectId;
use crate::objects::layers::MotionMut;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::sprite::Cargo;
use crate::players::PlayerId;
use crate::unit_ai::Answer;
use crate::unit_ai::OrderForce;
use crate::unit_ai::Pace;
use crate::{GenieError, GenieResult};

use crate::gathering::gather::Walks;
use crate::gathering::hunt::HuntMove;
use crate::math::rng::{Stream, Well512};
use crate::movement::move_to::{
    MoveTo, MoveToAction, MoveToReads, SpriteReads, ai_sprite, object_sprite, point_sprite,
};
use crate::objects::release::Releaser;
use crate::objects::teardown::TornWalk;
use crate::objects::unit::Unit;

use crate::combat::abilities::Look;
use crate::construction::repair::RepairAction;
use crate::garrison::transport::SailWorld;
use crate::garrison::transport::TransportAction;
use crate::gathering::gather::GATHERER_STATE_LACKING;
use crate::gathering::hunt::HUNTER_STATE_LACKING;
use crate::gathering::{GatherTargetId, HuntTargetId};
use crate::movement::move_to::ToObject;
use crate::movement::move_to::{MOVE_TO, TARGET_LACKING, holds_object, object_range};
use crate::objects::actions::Action as ActionsAction;
use crate::objects::layers::Action;
use crate::objects::masters::OwnerMasters;
use crate::objects::sprite::new_sprite;
use crate::objects::{Graphics, ObjectState};
use crate::ownership::artifact::SubActionsGone;
use crate::unit_ai::AiMove;
use crate::unit_ai::UnitStance;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::MoveToTuning;
use crate::world::layer::generated::SpriteTuning;
use crate::world::lets::{take, torn_down};
use crate::world::step::Step;
use crate::world::unread::UNREAD_ACTION;
use crate::world::{Unread, WALKER_UNMOVING, World};

/// What the sprites of a move the unit AI gives read: the owner's masters, the sprites' table,
/// the unit's terrain look and the build's roles, and what its slot 0x208 reads
/// ([`Unit::ai_move_with`], [`Unit::head_takes_point`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct MoveSprites<'world> {
    /// The unit's owner's masters.
    pub(crate) masters: OwnerMasters<'world>,
    /// The sprites' table.
    pub(crate) graphics: &'world Graphics,
    /// The unit's terrain look.
    pub(crate) look: Option<&'world Look>,
    /// The build's roles.
    pub(crate) roles: &'world Roles,
    /// The `MoveTo`'s tuning values.
    pub(crate) move_to_tuning: &'world MoveToTuning,
    /// What the unit's slot 0x208 reads besides its mover's own speed and its place.
    pub(crate) top_speed: TopSpeed,
}

impl Unit {
    /// The unit's slot 0x340 on its unit AI's move ([`Unit::ai_move_with`]) in the context
    /// `step`, and its answer ([`MoveWith::answer`]): what the head lets go applied at once; the
    /// `MoveTo` made for a list whose head declines the point replaces the list through the
    /// install on the step ([`Unit::install_own_move_to`]: a deleted head's restart at once); a
    /// Transport sails; an unpacked trebuchet's Pack installs on the unit lent back to its slot
    /// ([`World::install_pack_to_move`]: its constructor writes the unit AI that the world
    /// holds).
    ///
    /// # Errors
    ///
    /// The move's, the deletion's, the sail's and the Pack's install.
    pub(crate) fn ai_move(
        &mut self,
        actions: &mut Action,
        requested: AiMove,
        class: ClassId,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<Answer> {
        let by = self.releaser(step)?;
        let top_speed = step.world.top_speed_terms(step.player, self)?;
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        let move_with = self.ai_move_with(
            (actions, step.world.parts.unread.get(self.id)),
            step.world.parts.motion.layer_mut(self.id),
            requested,
            (class, step.player),
            (&mut releases, &mut leaves, by),
            MoveSprites {
                masters: OwnerMasters::new(
                    &step.world.parts.own_master,
                    step.world.model.masters.of(step.player),
                ),
                graphics: &step.world.model.graphics,
                look: step.world.parts.combat.of(self.id).abilities.look.as_ref(),
                roles,
                move_to_tuning: &step.world.model.tuning.move_to,
                top_speed,
            },
        )?;
        torn_down(step, self, (leaves, releases))?;
        match move_with {
            MoveWith::Sails(transport_action) => {
                let sprite_rng = step.world.streams.get_mut(Stream::Sprite);
                self.transport_sail(
                    transport_action,
                    actions,
                    step.world.parts.motion.layer_mut(self.id),
                    top_speed,
                    requested.point,
                    SailWorld {
                        spatial: &step.world.spatial,
                        zones: &mut step.world.zones,
                        pathing: &mut step.world.pathing,
                        model: &step.world.model,
                        masters: OwnerMasters::new(
                            &step.world.parts.own_master,
                            step.world.model.masters.of(step.player),
                        ),
                        look: step.world.parts.combat.of(self.id).abilities.look.as_ref(),
                        sprite_rng,
                    },
                )?;
            }
            MoveWith::Replaces(move_to) => self.install_own_move_to(actions, move_to, step)?,
            MoveWith::Packs => {
                step.lend_at((&mut *self, &mut *actions), World::install_pack_to_move)?;
            }
            MoveWith::Walks | MoveWith::Stays => {}
        }
        Ok(move_with.answer())
    }

    /// The unit's slot 0x340 (`FUN_141216c50`) on a move to `ai_move.point` its unit AI gives it
    /// (also its move off a footprint, [`crate::world::World::move_aside`]), the unit of `class` of
    /// player `player`: a trebuchet packs or stays ([`trebuchet_move`]), any other unit moves
    /// (`FUN_1410866e0`, which answers 1). Without `ai_move.force` a head `MoveTo` whose point
    /// (`+0x38`..`+0x40`, [`MoveToAction::point`]) is `ai_move.point` stands: the move is done.
    /// Then the head action's slot 0x40 may take the point ([`Unit::head_takes_point`]; the
    /// target it lets go into `releases` and a counted repairer's building into
    /// `builder_leaves`, by `by`). Otherwise a `MoveTo` to `ai_move.point` within
    /// `ai_move.range` is made (`FUN_140aa0d30`, the master's walking sprite with [`Pace::Walk`],
    /// else its running one; the sprite is not held), which replaces the action list
    /// (`FUN_140a98620`, then slot 0x808): the caller installs it with the unit's step or the
    /// world ([`MoveWith::Replaces`]: a deleted head's restart runs at once).
    /// [`MoveWith::Sails`] when a Transport on top takes the point,
    /// which the caller sails there ([`Unit::transport_sail`]). `motion` is the unit's motion
    /// layer, `actions` its action layer and `unread` why the load could not read it whole
    /// ([`crate::world::Unread`]).
    ///
    /// # Errors
    ///
    /// A trebuchet whose list the load could not read ([`UNREAD_ACTION`]), and what the head's
    /// slot 0x40 does not model.
    pub(crate) fn ai_move_with(
        &self,
        (actions, unread): (&mut Action, Option<&Unread>),
        motion: MotionMut<'_>,
        ai_move: AiMove,
        (class, player): (ClassId, PlayerId),
        (releases, builder_leaves, by): (
            &mut Vec<(ObjectId, Releaser)>,
            &mut Vec<ObjectId>,
            Releaser,
        ),
        sprites: MoveSprites<'_>,
    ) -> GenieResult<MoveWith> {
        if let Some(siege) = trebuchet_move(actions, unread, class, sprites.roles)? {
            return Ok(siege);
        }
        if ai_move.force == OrderForce::Unforced
            && actions.top_action_kind() == Some(MOVE_TO)
            && actions
                .list
                .move_to()
                .is_some_and(|mt| mt.point == ai_move.point)
        {
            return Ok(MoveWith::Walks);
        }
        match self.head_takes_point((actions, unread), motion, ai_move.point, player, sprites)? {
            PointMove::Taken(released) => {
                if let Some(object_id) = released {
                    releases.push((object_id, by));
                }
                Ok(MoveWith::Walks)
            }
            PointMove::Repairs(released, left) => {
                builder_leaves.extend(left);
                if let Some(object_id) = released {
                    releases.push((object_id, by));
                }
                Ok(MoveWith::Walks)
            }
            PointMove::Sails(transport_action) => Ok(MoveWith::Sails(transport_action)),
            PointMove::Declined => Ok(MoveWith::Replaces(self.ai_move_to(ai_move, sprites)?)),
        }
    }

    /// The `MoveTo` slot 0x340 makes for a list whose head declined the point (`FUN_140aa0d30`
    /// before the list's deletion): to `ai_move.point` within `ai_move.range`, the master's
    /// walking sprite with [`Pace::Walk`], else its running one, else the constructor's default
    /// ([`point_sprite`] with the unit's terrain look in `sprites`).
    ///
    /// # Errors
    ///
    /// A master the owner's masters lack.
    fn ai_move_to(&self, ai_move: AiMove, sprites: MoveSprites<'_>) -> GenieResult<MoveToAction> {
        let master = self.master_in(sprites.masters)?;
        let sprite = ai_sprite(master, ai_move.pace)
            .or_else(|| point_sprite(master, sprites.look, Cargo::Empty, sprites.roles))
            .and_then(|id| sprites.graphics.get(id).copied());
        Ok(MoveToAction::to_point(ai_move.point, ai_move.range, sprite))
    }

    /// The head action's slot 0x40 on a move to `point` without a target object (its second
    /// argument 0), the unit of player `player`. An empty list takes nothing (`FUN_140a98730`), nor
    /// does the base slot (`FUN_140a78490`, which answers 0) of a `MoveTo`, an Explore, a Build
    /// (`0x140aade60`), an Attack, an Enter, a Fly, a missile, a Make, a wait for a foundation, a
    /// Housing, a Pack or an Unpack.
    /// The Artifact's (`FUN_140aaca30`) takes it ([`Artifact::take_point`]; a `MoveTo` it walked
    /// goes and stops the unit); the Hunt's (`FUN_140ac52c0`, [`HuntAction::take_move`]) unless its
    /// task has a work range; the Gather's (`0x140a8b740`,
    /// [`crate::gathering::gather::GatherAction::take_move`]) always. The Hunt and the Gather let
    /// their target go (slot 0x68 with none), which [`PointMove::Taken`] names for the caller to
    /// release; their second target stays.
    ///
    /// # Errors
    ///
    /// An action the slice does not hold, and what the Hunt's and the Gather's walks do not
    /// model.
    ///
    /// [`Artifact::take_point`]: crate::ownership::artifact::Artifact::take_point
    /// [`HuntAction::take_move`]: crate::gathering::hunt::HuntAction::take_move
    pub(crate) fn head_takes_point(
        &self,
        (actions, unread): (&mut Action, Option<&Unread>),
        mut motion: MotionMut<'_>,
        point: Position,
        player: PlayerId,
        sprites: MoveSprites<'_>,
    ) -> GenieResult<PointMove> {
        let MoveSprites {
            masters,
            graphics,
            look,
            roles,
            move_to_tuning,
            top_speed,
            ..
        } = sprites;
        if unread.is_some() {
            return Err(UNREAD_ACTION);
        }
        match actions.list.head_mut() {
            None
            | Some(
                ActionsAction::MoveTo(_)
                | ActionsAction::Explore(_)
                | ActionsAction::Build(_)
                | ActionsAction::Attack(_)
                | ActionsAction::Enter(_)
                | ActionsAction::Fly(_)
                | ActionsAction::Missile(_)
                | ActionsAction::Make(_)
                | ActionsAction::WaitSolidify(_)
                | ActionsAction::Housing(_)
                | ActionsAction::Pack(_)
                | ActionsAction::Unpack(_),
            ) => Ok(PointMove::Declined),
            Some(ActionsAction::Artifact(_)) => {
                let sprites = self.point_sprites(sprites)?;
                self.artifact_takes_point(
                    actions,
                    motion.mover_mut(),
                    point,
                    &sprites,
                    top_speed,
                    move_to_tuning,
                )
            }
            Some(ActionsAction::Hunt(_)) => {
                let walks = Walks::of(
                    self.point_sprites(sprites)?,
                    self,
                    motion.mover(),
                    top_speed,
                );
                self.hunt_takes_point(
                    actions,
                    motion.mover_mut(),
                    point,
                    player,
                    walks,
                    move_to_tuning,
                )
            }
            Some(ActionsAction::Gather(_)) => {
                let walks = Walks::of(
                    self.point_sprites(sprites)?,
                    self,
                    motion.mover(),
                    top_speed,
                );
                self.gather_takes_point(
                    actions,
                    motion.mover_mut(),
                    point,
                    player,
                    walks,
                    move_to_tuning,
                )
            }
            // the Transport's (0x140aa3e40): the caller sails it there
            Some(ActionsAction::Transport(transport_action)) => {
                Ok(PointMove::Sails(*transport_action))
            }
            // the Repair's (FUN_140ad9700): its point, slot 0x68 with none, set_state(11)
            Some(ActionsAction::Repair(repair)) => {
                let top = top_speed.of_object(motion.mover(), self.position().ground());
                let move_reads =
                    MoveToReads::of(self, top, self.master_in(masters)?, graphics, look, roles);
                repair_takes_point(
                    repair,
                    motion.mover_mut(),
                    point,
                    &move_reads,
                    move_to_tuning,
                )
            }
        }
    }

    /// The sprites a `MoveTo` to a point that the unit's head action makes takes for none
    /// (`FUN_140aa0d30`'s default from the unit's master and terrain look).
    fn point_sprites<'world>(
        &self,
        sprites: MoveSprites<'world>,
    ) -> GenieResult<SpriteReads<'world>> {
        Ok(SpriteReads::of(
            self.master_in(sprites.masters)?,
            sprites.look,
            sprites.graphics,
            sprites.roles,
        ))
    }

    /// The Artifact's slot 0x40 (its slot 8 with the point, [`Artifact::take_point`]): its
    /// sub-actions go, a walking one stopping the unit, and the Enter it held lets its building
    /// or transport go.
    ///
    /// [`Artifact::take_point`]: crate::ownership::artifact::Artifact::take_point
    fn artifact_takes_point(
        &self,
        actions: &mut Action,
        mover: Option<&mut Mover>,
        point: Position,
        sprites: &SpriteReads<'_>,
        top_speed: TopSpeed,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<PointMove> {
        let top = top_speed.of_object(mover.as_deref(), self.position().ground());
        let reads = MoveToReads::with_sprites(*sprites, self, mover.as_deref(), top);
        let gone = match actions.list.artifact_mut() {
            Some(artifact) => artifact.take_point(point, &reads, move_to_tuning)?,
            None => SubActionsGone::default(),
        };
        if gone.walk == TornWalk::Stops {
            mover.ok_or(WALKER_UNMOVING)?.stop()?;
        }
        Ok(PointMove::Taken(gone.released))
    }

    /// The Hunt's slot 0x40 ([`HuntAction::take_move`] with the point): the target it had is
    /// let go, unless the hunt declines the move.
    ///
    /// [`HuntAction::take_move`]: crate::gathering::hunt::HuntAction::take_move
    fn hunt_takes_point(
        &self,
        actions: &mut Action,
        mover: Option<&mut Mover>,
        point: Position,
        player: PlayerId,
        walks: Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<PointMove> {
        let my_pasture = self.my_pasture;
        let (Some(hunt), Some(mover)) = (actions.list.hunt_mut(), mover) else {
            return Err(HUNTER_STATE_LACKING);
        };
        let old = hunt.target.map(HuntTargetId::id);
        if hunt.take_move(
            HuntMove {
                target: None,
                point,
                player,
                walks,
                drops_here: false,
                my_pasture,
                gap: Distance::ZERO,
            },
            mover,
            move_to_tuning,
        )? == Answer::NotTaken
        {
            return Ok(PointMove::Declined);
        }
        Ok(PointMove::Taken(old))
    }

    /// The Gather's slot 0x40 (its move to the point): the target it had is let go; the
    /// `MoveTo` it makes reads the unit's `walks` (its sprites' default for a sprite the task
    /// lacks).
    #[expect(
        clippy::unused_self,
        reason = "the unit's call of `TRIBE_Action_Gather::move_to` (slot 0x40, `FUN_140a97730`), which reads the Gather, not the unit"
    )]
    fn gather_takes_point(
        &self,
        actions: &mut Action,
        mover: Option<&mut Mover>,
        point: Position,
        player: PlayerId,
        walks: Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<PointMove> {
        let (Some(gather), Some(mover)) = (actions.list.gather_mut(), mover) else {
            return Err(GATHERER_STATE_LACKING);
        };
        let old = gather.target.map(GatherTargetId::id);
        gather.take_move(None, point, player, mover, &walks, move_to_tuning)?;
        Ok(PointMove::Taken(old))
    }

    /// The unit's slot 0x348 (`FUN_141086440`), forced, on its unit AI's move to an object (in
    /// `state`): a `MoveTo` to the target within `range` (none: slot 0x268's,
    /// [`crate::movement::move_to::object_range`]), showing the master's running sprite (the
    /// walking one without it), takes the target it holds (`FUN_140aa0df0`, quiet 0: slot 0x178,
    /// [`crate::movement::move_to::holds_object`], [`MoveToAction::to_object`]), then replaces
    /// its action list (`FUN_140a98620`, then slot 0x808: [`Unit::install_own_move_to`], which
    /// deletes it unstarted for a unit in state 3 or later, its target let go).
    pub(crate) fn move_to_object(
        &mut self,
        actions: &mut Action,
        (target, state): (ObjectId, ObjectState),
        range: Distance,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        if step.world.parts.unread.contains(self.id) {
            return Err(GenieError::unsupported(
                "an AI move to an object of a unit with an action",
            ));
        }
        let livestock = roles
            .of_class(self.master(step.world.master_tables(), step.player)?.class)
            .contains(ClassRoles::LIVESTOCK);
        let master = step
            .world
            .spatial
            .get_known(target)?
            .ok_or(TARGET_LACKING)?
            .master;
        if holds_object(livestock, master, state, roles) {
            let by = self.releaser(step)?;
            take(step, self, target, by, roles)?;
        }
        let own = self.master(step.world.master_tables(), step.player)?;
        let range = object_range(range, Some(own), &step.world.model.tuning.order);
        let sprite = ai_sprite(own, Pace::Run)
            .or_else(|| object_sprite(own))
            .and_then(|id| step.world.model.graphics.get(id).copied());
        let move_to = MoveToAction::to_object(
            &step.world.spatial,
            ToObject {
                target,
                state,
                range,
                sprite,
                livestock,
            },
            &step.world.model,
        )?;
        self.install_own_move_to(actions, move_to, step)
    }

    /// `LAB_140aa320c`, the motion step gave up on the unit's own `MoveTo`: a human's villager
    /// (`waits`) waits once (`set_state(2)`, 1 s or 2 s by its `stance`, which shows nothing); any
    /// other unit's `MoveTo` fails (`set_state(13)`), which its unit AI's check between turns may
    /// interrupt with its next order the same update. The failure's `set_state` (`0x140aa29e0`, an
    /// own `MoveTo` is never quiet) shows the standing sprite at its speed and clears `+0x7b8`.
    ///
    /// # Errors
    ///
    /// A unit with a sprite whose master's standing sprite the slice does not hold.
    pub(crate) fn own_move_gave_up(
        &mut self,
        actions: &mut Action,
        motion: MotionMut<'_>,
        patience: Patience,
        stance: UnitStance,
        (masters, graphics, sprite_rng): (OwnerMasters<'_>, &Graphics, &mut Well512),
        sprite_tuning: SpriteTuning,
    ) -> GenieResult<()> {
        let Some(move_to_action) = actions.list.move_to_mut() else {
            return Ok(());
        };
        move_to_action.state = move_to_action.wait.give_up(patience, stance);
        if move_to_action.state == MoveTo::Failed {
            self.shows_standing_sprite(motion, masters, graphics, sprite_rng, sprite_tuning)?;
        }
        Ok(())
    }

    /// A `MoveTo`'s `set_state` 1, not quiet, on the unit (`0x140aa29e0`): the master's standing
    /// sprite where the slice holds the unit's sprite (slot 0x68 with slot 0x798's, a draw of the
    /// Sprite stream), and its animation speed (`+0x7b8`) cleared where known.
    pub(crate) fn shows_standing_sprite(
        &mut self,
        mut motion: MotionMut<'_>,
        masters: OwnerMasters<'_>,
        graphics: &Graphics,
        sprite_rng: &mut Well512,
        sprite_tuning: SpriteTuning,
    ) -> GenieResult<()> {
        let standing = self
            .master_in(masters)?
            .standing_sprite
            .and_then(|sprite_id| graphics.get(sprite_id))
            .copied();
        if let Some(mover) = motion.mover_mut() {
            mover.shows(standing.as_ref());
        }
        if self.sprite.is_some() {
            let standing = standing.ok_or(GenieError::unsupported(
                "an arrived walker without its standing sprite",
            ))?;
            new_sprite(
                &mut self.sprite,
                motion.move_stats_mut(),
                standing,
                sprite_rng,
                sprite_tuning,
            );
        }
        motion.set_anim_speed(Speed::ZERO);
        Ok(())
    }
}

/// The Repair's slot 0x40 (`FUN_140ad9700`, [`RepairAction::take_point`]): its point, slot 0x68
/// with none, `set_state(11)`, whose `MoveTo` reads `move_reads`. Returns the target let go and
/// the one whose repairers it leaves.
///
/// # Errors
///
/// The point's walk.
fn repair_takes_point(
    repair: &mut RepairAction,
    mover: Option<&mut Mover>,
    point: Position,
    move_reads: &MoveToReads<'_>,
    move_to_tuning: &MoveToTuning,
) -> GenieResult<PointMove> {
    let (old, counted) = repair.take_point(point, mover, move_reads, move_to_tuning)?;
    Ok(PointMove::Repairs(old, old.filter(|_| counted)))
}

/// `FUN_141216c50`'s trebuchet part for a unit of `class` whose action layer is `actions` and
/// whose load could not read its list for `unread`: a class of [`ClassRoles::STILL_WHILE_PACKING`]
/// (54 at `0x141216d21`, 51 at `0x141216da5`) whose head (`FUN_140a986f0`) packs or unpacks (type
/// `0x7c` or `0x7d`: `SUB AX, 0x7c; CMP AX, 1; JBE` at `0x141216dc4`) stays
/// ([`MoveWith::Stays`]); else a class of [`ClassRoles::PACKS_TO_MOVE`] (54 at `0x141216e4c`)
/// packs ([`MoveWith::Packs`]). None for any other class, and for class 51 under another head:
/// they move (`FUN_1410866e0` at `0x141216ee6`).
///
/// # Errors
///
/// A trebuchet whose list the load could not read ([`UNREAD_ACTION`]): its head may be a Pack or
/// an Unpack.
fn trebuchet_move(
    actions: &Action,
    unread: Option<&Unread>,
    class: ClassId,
    roles: &Roles,
) -> GenieResult<Option<MoveWith>> {
    let class_roles = roles.of_class(class);
    if !class_roles.contains(ClassRoles::STILL_WHILE_PACKING) {
        return Ok(None);
    }
    if unread.is_some() {
        return Err(UNREAD_ACTION);
    }
    if matches!(
        actions.top_action_kind(),
        Some(ActionKind::PACK | ActionKind::UNPACK)
    ) {
        return Ok(Some(MoveWith::Stays));
    }
    Ok(class_roles
        .contains(ClassRoles::PACKS_TO_MOVE)
        .then_some(MoveWith::Packs))
}

/// What slot 0x340 does with the unit AI's move: by sea, on foot, a Pack in its place, or
/// nothing ([`Unit::ai_move_with`]).
#[derive(Clone, Copy, Debug, PartialEq)]
#[must_use]
pub(crate) enum MoveWith {
    /// A Transport on top takes the point: the caller sails there with this copy of it
    /// ([`Unit::transport_sail`]).
    Sails(TransportAction),
    /// The head declined the point: the caller installs this `MoveTo` of the unit's own
    /// (`FUN_140a98620`, then slot 0x808: [`Unit::install_own_move_to`] on the unit's step,
    /// [`crate::world::World::install_move_to`] from the world), a deleted head's restart at once.
    Replaces(MoveToAction),
    /// The unit moves itself, or nothing moves.
    Walks,
    /// An unpacked trebuchet not under its own Pack or Unpack: the caller installs a Pack in
    /// place of the move (`FUN_140ad4ff0` with −1, then slot 0x808 at `0x141216eaa`:
    /// [`crate::world::World::install_pack_to_move`]).
    Packs,
    /// A trebuchet under its own Pack or Unpack: nothing moves.
    Stays,
}

impl MoveWith {
    /// Slot 0x340's answer: 0 for a trebuchet that packs or stays (`XOR EAX, EAX` at
    /// `0x141216eb0`, whatever the install did), else `FUN_1410866e0`'s 1.
    pub(crate) const fn answer(self) -> Answer {
        match self {
            Self::Packs | Self::Stays => Answer::NotTaken,
            Self::Sails(_) | Self::Replaces(_) | Self::Walks => Answer::Taken,
        }
    }
}

/// What the head action's slot 0x40 did with a move to a point ([`Unit::head_takes_point`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PointMove {
    /// It answered 0 (an empty list, the base slot, a Hunt whose task has a work range): the
    /// caller's `MoveTo` replaces the actions.
    Declined,
    /// It answered 1: the action walks there itself. The target it let go (slot 0x68 with none),
    /// if any, which the caller releases.
    Taken(Option<ObjectId>),
    /// A Repair answered 1 (`FUN_140ad9700`): it walks to the point; the target it let go, and the
    /// target whose repairers a counted repairer left ([`crate::world::World::leave_builders`]).
    Repairs(Option<ObjectId>, Option<ObjectId>),
    /// The Transport answers 1 (`0x140aa3e40`): it sails to the point, which the caller runs
    /// with the map on this copy of it ([`Unit::transport_sail`]).
    Sails(TransportAction),
}
