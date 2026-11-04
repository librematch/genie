//! A unit's moves its unit AI asks: to a point (slot 0x340, the head action's slot 0x40 may take
//! the point) and to an object (slot 0x348), a move that gave up, and the standing sprite of a
//! `MoveTo` that ends at its start.

use crate::map::Distance;
use crate::map::Position;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::move_to::Patience;
use crate::objects::ObjectId;
use crate::objects::layers::MotionMut;
use crate::objects::masters::ClassId;
use crate::objects::masters::PlayersMasters;
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
use crate::objects::unit::Unit;

use crate::combat::abilities::Look;
use crate::construction::repair::RepairAction;
use crate::garrison::transport::TransportAction;
use crate::gathering::{GatherTargetId, HuntTargetId};
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
use crate::world::{Unread, WALKER_UNMOVING};

/// What the sprites of a move the unit AI gives read: the owner's masters, the sprites' table,
/// the unit's terrain look and the build's roles ([`Unit::ai_move_with`],
/// [`Unit::head_takes_point`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct MoveSprites<'a> {
    /// The unit's owner's masters.
    pub(crate) masters: OwnerMasters<'a>,
    /// The sprites' table.
    pub(crate) graphics: &'a Graphics,
    /// The unit's terrain look.
    pub(crate) look: Option<&'a Look>,
    /// The build's roles.
    pub(crate) roles: &'a Roles,
    /// The sprites' tuning values.
    pub(crate) sprite_tuning: SpriteTuning,
    /// The `MoveTo`'s tuning values.
    pub(crate) move_to_tuning: &'a MoveToTuning,
}

impl Unit {
    /// The unit's slot 0x340 on its unit AI's move ([`Unit::ai_move_with`]), what it lets go and
    /// the standing sprite of a `MoveTo` that fails at its start into the context `c`.
    pub(crate) fn ai_move(
        &mut self,
        actions: &mut Action,
        m: AiMove,
        class: ClassId,
        c: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let by = self.releaser(c)?;
        let sprite_rng = c.w.streams.get(Stream::Sprite);
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        let sails = self.ai_move_with(
            (actions, c.w.parts.unread.get(self.id)),
            c.w.parts.motion.layer_mut(self.id),
            m,
            (class, c.player),
            (&mut releases, &mut leaves, by),
            (
                MoveSprites {
                    masters: OwnerMasters::new(
                        &c.w.parts.own_master,
                        c.w.model.masters.of(c.player),
                    ),
                    graphics: &c.w.model.graphics,
                    look: c.w.parts.combat.of(self.id).abilities.look.as_ref(),
                    roles,
                    sprite_tuning: c.w.model.tuning.sprite,
                    move_to_tuning: &c.w.model.tuning.move_to,
                },
                sprite_rng,
            ),
        )?;
        torn_down(c, self, (leaves, releases))?;
        if let MoveWith::Sails(t) = sails {
            let sprite_rng = c.w.streams.get(Stream::Sprite);
            self.transport_sail(
                (t, actions),
                c.w.parts.motion.layer_mut(self.id),
                m.point,
                (&c.w.spatial, &mut c.w.zones, &mut c.w.pathing),
                &c.w.model,
                (
                    OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
                    c.w.parts.combat.of(self.id).abilities.look.as_ref(),
                    sprite_rng,
                ),
            )?;
        }
        Ok(())
    }

    /// The unit's slot 0x340 (`FUN_141216c50`) on a move to `m.point` its unit AI gives it (also
    /// its move off a footprint, [`crate::world::World::move_aside`]), the unit of `class` of
    /// player `player`. An unpacked trebuchet (class 54) packs instead (its Pack action,
    /// `FUN_140ad4ff0` through slot 0x808), which is not modelled; a packed one (class 51) whose
    /// head action packs or unpacks (types `0x7c`, `0x7d`) does nothing, and the slice holds no such
    /// action, so it moves as any other: `FUN_1410866e0`. Without `m.force` a head `MoveTo` whose
    /// point (`+0x38`..`+0x40`, [`MoveToAction::point`]) is `m.point` stands: the move is done.
    /// Then the head action's slot 0x40 may take the point ([`Unit::head_takes_point`]; the
    /// target it lets go into `releases`). Otherwise a `MoveTo` to the point within `m.range`
    /// (`FUN_140aa0d30`, the master's walking sprite with [`Pace::Walk`], else its running one; the
    /// sprite is not held) is made, the action list is cleared (`FUN_140a98620`,
    /// [`Unit::clear_own_actions`]: the teardowns let their targets go into `releases` and a
    /// counted builder's foundation into `builder_leaves`, by `by`) and the `MoveTo` installed and
    /// started (slot 0x808, `FUN_14106e990`: its slot 0x70 sees an empty list; `FUN_140a977e0`
    /// starts an action of a unit below state 3, which the unit AI's moves are, and
    /// [`MoveToAction::to_point`]); one that ends at its start shows the standing sprite of the
    /// unit's master in `look` ([`Unit::shows_standing_sprite`]). [`MoveWith::Sails`] when a
    /// Transport on top takes the point, which the caller sails there ([`Unit::transport_sail`]).
    /// `motion` is the unit's motion layer, `actions` its action layer and `unread` why the load
    /// could not read it whole ([`crate::world::Unread`]).
    pub(crate) fn ai_move_with(
        &mut self,
        (actions, unread): (&mut Action, Option<&Unread>),
        mut motion: MotionMut<'_>,
        m: AiMove,
        (class, player): (ClassId, PlayerId),
        (releases, builder_leaves, by): (
            &mut Vec<(ObjectId, Releaser)>,
            &mut Vec<ObjectId>,
            Releaser,
        ),
        (sprites, sprite_rng): (MoveSprites<'_>, &mut Well512),
    ) -> GenieResult<MoveWith> {
        let MoveSprites {
            masters,
            graphics,
            look,
            roles,
            sprite_tuning,
            ..
        } = sprites;
        if roles.of_class(class).contains(ClassRoles::PACKS_TO_MOVE) {
            return Err(GenieError::unsupported(
                "an AI move of an unpacked trebuchet (class 54: its Pack action, FUN_140ad4ff0)",
            ));
        }
        if m.force == OrderForce::Unforced
            && actions.top_action_kind() == Some(MOVE_TO)
            && actions.list.move_to().is_some_and(|mt| mt.point == m.point)
        {
            return Ok(MoveWith::Walks);
        }
        match self.head_takes_point(
            (actions, unread),
            motion.reborrow(),
            m.point,
            player,
            sprites,
        )? {
            PointMove::Taken(released) => {
                if let Some(t) = released {
                    releases.push((t, by));
                }
                return Ok(MoveWith::Walks);
            }
            PointMove::Repairs(released, left) => {
                builder_leaves.extend(left);
                if let Some(t) = released {
                    releases.push((t, by));
                }
                return Ok(MoveWith::Walks);
            }
            PointMove::Sails(t) => return Ok(MoveWith::Sails(t)),
            PointMove::Declined => {}
        }
        self.clear_own_actions(actions, motion.reborrow(), releases, builder_leaves, by)?;
        let master = self.master_in(masters)?;
        let sprite = ai_sprite(master, m.pace)
            .or_else(|| point_sprite(master, look, Cargo::Empty, roles))
            .and_then(|id| graphics.get(id).copied());
        let move_to = MoveToAction::to_point(m.point, m.range, sprite, motion.mover_mut());
        if move_to.shows_standing() {
            self.shows_standing_sprite(motion, masters, graphics, sprite_rng, sprite_tuning)?;
        }
        actions.list.set_move_to(Some(move_to));
        Ok(MoveWith::Walks)
    }

    /// The head action's slot 0x40 on a move to `point` without a target object (its second
    /// argument 0), the unit of player `player`. An empty list takes nothing
    /// (`FUN_140a98730`), nor does the base slot (`FUN_140a78490`, which answers 0) of a `MoveTo`,
    /// an Explore, a Build (`0x140aade60`), an Attack, an Enter, a Fly, a missile, a Make or a
    /// wait for a foundation. The Artifact's (`FUN_140aaca30`) takes it
    /// ([`Artifact::take_point`]; a `MoveTo` it walked goes and stops the unit); the Hunt's
    /// (`FUN_140ac52c0`, [`HuntAction::take_move`]) unless its task has a work range; the
    /// Gather's (`0x140a8b740`, [`crate::gathering::gather::GatherAction::take_move`]) always. The Hunt and
    /// the Gather let their target go (slot 0x68 with none), which [`PointMove::Taken`] names
    /// for the caller to release; their second target stays.
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
            ..
        } = sprites;
        if unread.is_some() {
            return Err(GenieError::unsupported(
                "slot 0x40 of an action the slice does not hold",
            ));
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
                | ActionsAction::WaitSolidify(_),
            ) => Ok(PointMove::Declined),
            Some(ActionsAction::Artifact(_)) => {
                let sprites = self.point_sprites(sprites)?;
                self.artifact_takes_point(
                    actions,
                    motion.mover_mut(),
                    point,
                    &sprites,
                    move_to_tuning,
                )
            }
            Some(ActionsAction::Hunt(_)) => {
                let sprites = self.point_sprites(sprites)?;
                self.hunt_takes_point(
                    actions,
                    motion.mover_mut(),
                    point,
                    player,
                    sprites,
                    move_to_tuning,
                )
            }
            Some(ActionsAction::Gather(_)) => {
                let sprites = self.point_sprites(sprites)?;
                self.gather_takes_point(
                    actions,
                    motion.mover_mut(),
                    point,
                    player,
                    sprites,
                    move_to_tuning,
                )
            }
            // the Transport's (0x140aa3e40): the caller sails it there
            Some(ActionsAction::Transport(t)) => Ok(PointMove::Sails(*t)),
            // the Repair's (FUN_140ad9700): its point, slot 0x68 with none, set_state(11)
            Some(ActionsAction::Repair(repair)) => {
                let move_reads = MoveToReads::of(
                    (self, motion.mover()),
                    self.master_in(masters)?,
                    (graphics, look),
                    roles,
                );
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
    fn point_sprites<'g>(&self, sprites: MoveSprites<'g>) -> GenieResult<SpriteReads<'g>> {
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
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<PointMove> {
        let reads = MoveToReads::with_sprites(*sprites, (self, mover.as_deref()));
        let gone = match actions.list.artifact_mut() {
            Some(a) => a.take_point(point, &reads, move_to_tuning)?,
            None => SubActionsGone::default(),
        };
        if gone.stops {
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
        sprites: SpriteReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<PointMove> {
        let my_pasture = self.my_pasture;
        let walks = Walks::of(sprites, (self, mover.as_deref()));
        let (Some(hunt), Some(mover)) = (actions.list.hunt_mut(), mover) else {
            return Err(GenieError::unsupported(
                "a hunter without its hunt or movement state",
            ));
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
    /// `MoveTo` it makes takes `sprites`' default for a sprite the task lacks.
    fn gather_takes_point(
        &self,
        actions: &mut Action,
        mover: Option<&mut Mover>,
        point: Position,
        player: PlayerId,
        sprites: SpriteReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<PointMove> {
        let walks = Walks::of(sprites, (self, mover.as_deref()));
        let (Some(gather), Some(mover)) = (actions.list.gather_mut(), mover) else {
            return Err(GenieError::unsupported(
                "a gatherer without its gather or movement state",
            ));
        };
        let old = gather.target.map(GatherTargetId::id);
        gather.take_move(None, point, player, mover, &walks, move_to_tuning)?;
        Ok(PointMove::Taken(old))
    }

    /// The unit's slot 0x348 (`FUN_141086440`), forced, on its unit AI's move to an object (in
    /// `state`): a `MoveTo` to the target within `range` (none: slot 0x268's,
    /// [`crate::movement::move_to::object_range`]), showing the master's running sprite (the
    /// walking one without it), takes the target it holds (`FUN_140aa0df0`, quiet 0: slot 0x178,
    /// [`crate::movement::move_to::holds_object`]), then replaces its action list (`FUN_140a98620`,
    /// [`Unit::clear_own_actions`]: the old actions' teardowns let their targets go) and starts
    /// ([`MoveToAction::to_object`]).
    pub(crate) fn move_to_object(
        &mut self,
        actions: &mut Action,
        (target, state): (ObjectId, ObjectState),
        range: Distance,
        c: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        if c.w.parts.unread.contains(self.id) {
            return Err(GenieError::unsupported(
                "an AI move to an object of a unit with an action",
            ));
        }
        if c.w.parts.mover_of(self.id).is_none() {
            return Err(GenieError::unsupported(
                "an AI move to an object of a unit that does not move",
            ));
        }
        let livestock = roles
            .of_class(self.master(c.w.master_tables(), c.player)?.class)
            .contains(ClassRoles::LIVESTOCK);
        let master = c.w.spatial.get_known(target)?.ok_or(TARGET_LACKING)?.master;
        let by = self.releaser(c)?;
        if holds_object(livestock, master, state, roles) {
            take(c, self, target, by, roles)?;
        }
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        self.clear_own_actions(
            actions,
            c.w.parts.motion_mut(self.id),
            &mut releases,
            &mut leaves,
            by,
        )?;
        torn_down(c, self, (leaves, releases))?;
        let own = self.master(c.w.master_tables(), c.player)?;
        let range = object_range(range, Some(own), &c.w.model.tuning.order);
        let sprite = ai_sprite(own, Pace::Run)
            .or_else(|| object_sprite(own))
            .and_then(|id| c.w.model.graphics.get(id).copied());
        let here = self.position();
        let mover = c.w.parts.mover_mut(self.id).ok_or(GenieError::unreachable(
            "a walker tested above without its mover",
        ))?;
        let move_to = MoveToAction::to_object(
            &c.w.spatial,
            here,
            mover,
            (target, state),
            range,
            (sprite, livestock),
            &c.w.model,
        )?;
        if move_to.shows_standing() {
            self.shows_standing_sprite(
                c.w.parts.motion.layer_mut(self.id),
                OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
                &c.w.model.graphics,
                c.w.streams.get(Stream::Sprite),
                c.w.model.tuning.sprite,
            )?;
        }
        actions.list.set_move_to(Some(move_to));
        Ok(())
    }

    /// `LAB_140aa320c`, the motion step gave up on the unit's own `MoveTo`: a human's villager
    /// (`waits`) waits once (`set_state(2)`, 1 s or 2 s by its `stance`, which shows nothing);
    /// any other unit's `MoveTo` fails (`set_state(13)`), which its unit AI's check between turns may
    /// interrupt with its next order the same update. The failure's `set_state` (`0x140aa29e0`,
    /// an own `MoveTo` is never quiet) shows the standing sprite at its speed and clears
    /// `+0x7b8`.
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
        let Some(m) = actions.list.move_to_mut() else {
            return Ok(());
        };
        m.state = m.wait.give_up(patience, stance);
        if m.state == MoveTo::Failed {
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
            .and_then(|s| graphics.get(s))
            .copied();
        if let Some(m) = motion.mover_mut() {
            m.shows(standing.as_ref());
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

/// Whether the unit AI's move goes by sea or on foot ([`Unit::ai_move_with`]).
#[derive(Clone, Copy, Debug, PartialEq)]
#[must_use]
pub(crate) enum MoveWith {
    /// A Transport on top takes the point: the caller sails there with this copy of it
    /// ([`Unit::transport_sail`]).
    Sails(TransportAction),
    /// The unit moves itself, or nothing moves.
    Walks,
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
    /// A Repair answered 1 (`FUN_140ad9700`): it walks to the point; the target it let go, and
    /// the target whose repairers a counted repairer left ([`crate::world::World::leave_builders`]).
    Repairs(Option<ObjectId>, Option<ObjectId>),
    /// The Transport answers 1 (`0x140aa3e40`): it sails to the point, which the caller runs
    /// with the map on this copy of it ([`Unit::transport_sail`]).
    Sails(TransportAction),
}
