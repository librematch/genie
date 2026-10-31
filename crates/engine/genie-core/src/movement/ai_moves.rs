//! A unit's moves its unit AI asks: to a point (slot 0x340, the head action's slot 0x40 may take
//! the point) and to an object (slot 0x348), a move that gave up, and the standing sprite of a
//! `MoveTo` that ends at its start.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Position;
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::masters::ClassId;
use crate::objects::masters::action_type;
use crate::players::PlayerId;

use crate::gathering::gather::Walks;
use crate::gathering::hunt::HuntMove;
use crate::math::rng::{Stream, Well512};
use crate::movement::move_to::{
    MoveTo, MoveToAction, MoveToReads, SpriteReads, ai_sprite, object_sprite, point_sprite,
};
use crate::objects::masters::Masters;
use crate::objects::masters::class;
use crate::objects::release::Releaser;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;

use crate::unit_ai::AiMove;
use crate::world::step::Step;

/// An unpacked trebuchet's class (`-0xe76` raw), whose slot 0x340 packs instead of moving.
const UNPACKED_TREBUCHET: ClassId = class::CLASS_54;

impl Unit {
    /// The unit's slot 0x340 on its unit AI's move ([`Unit::ai_move_with`]), what it lets go and
    /// the standing sprite of a `MoveTo` that fails at its start into the context `c`.
    pub(crate) fn ai_move(
        &mut self,
        m: AiMove,
        class: ClassId,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let by = self.releaser(c)?;
        let sprite_rng = c.w.streams.get(Stream::Sprite);
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        let sails = self.ai_move_with(
            m,
            (class, c.player),
            (&mut releases, &mut leaves, by),
            (c.w.masters.get(c.player.index()), &c.w.graphics, sprite_rng),
        )?;
        crate::world::lets::torn_down(c, self, (leaves, releases))?;
        if sails {
            let sprite_rng = c.w.streams.get(Stream::Sprite);
            self.transport_sail(
                m.point,
                (&c.w.spatial, &mut c.w.zones, &mut c.w.pathing),
                (c.w.masters.get(c.player.index()), &c.w.graphics, sprite_rng),
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
    /// (`FUN_140aa0d30`, the master's walking sprite with `m.walk`, else its running one; the
    /// sprite is not held) is made, the action list is cleared (`FUN_140a98620`,
    /// [`Unit::clear_own_actions`]: the teardowns let their targets go into `releases` and a
    /// counted builder's foundation into `builder_leaves`, by `by`) and the `MoveTo` installed and
    /// started (slot 0x808, `FUN_14106e990`: its slot 0x70 sees an empty list; `FUN_140a977e0`
    /// starts an action of a unit below state 3, which the unit AI's moves are, and
    /// [`MoveToAction::to_point`]); one that ends at its start shows the standing sprite of the
    /// unit's master in `look` ([`Unit::shows_standing_sprite`]). True when a Transport on top
    /// takes the point, which the caller sails there ([`Unit::transport_sail`]).
    pub(crate) fn ai_move_with(
        &mut self,
        m: AiMove,
        (class, player): (ClassId, PlayerId),
        (releases, builder_leaves, by): (
            &mut Vec<(ObjectId, Releaser)>,
            &mut Vec<ObjectId>,
            Releaser,
        ),
        (masters, graphics, sprite_rng): (
            Option<&Masters>,
            &std::collections::BTreeMap<SpriteId, Graphic>,
            &mut Well512,
        ),
    ) -> Result<bool, GenieError> {
        if class == UNPACKED_TREBUCHET {
            return Err(GenieError::unsupported(
                "an AI move of an unpacked trebuchet (class 54: its Pack action, FUN_140ad4ff0)",
            ));
        }
        if !m.force
            && self.top_action_kind() == Some(crate::movement::move_to::MOVE_TO)
            && self.actions.move_to().is_some_and(|mt| mt.point == m.point)
        {
            return Ok(false);
        }
        match self.head_takes_point(m.point, player, masters, graphics)? {
            PointMove::Taken(released) => {
                if let Some(t) = released {
                    releases.push((t, by));
                }
                return Ok(false);
            }
            PointMove::Repairs(released, left) => {
                builder_leaves.extend(left);
                if let Some(t) = released {
                    releases.push((t, by));
                }
                return Ok(false);
            }
            PointMove::Sails => return Ok(true),
            PointMove::Declined => {}
        }
        self.clear_own_actions(releases, builder_leaves, by)?;
        let master = self.master_in(masters)?;
        let look = self.abilities.look.as_ref();
        let sprite = ai_sprite(master, m.walk)
            .or_else(|| point_sprite(master, look, false))
            .and_then(|id| graphics.get(&id).copied());
        let move_to = MoveToAction::to_point(m.point, m.range, sprite, self.body.mover_mut());
        if move_to.shows_standing() {
            self.shows_standing_sprite(masters, graphics, sprite_rng)?;
        }
        self.actions.set_move_to(Some(move_to));
        Ok(false)
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
        &mut self,
        point: Position,
        player: PlayerId,
        masters: Option<&Masters>,
        graphics: &std::collections::BTreeMap<SpriteId, Graphic>,
    ) -> Result<PointMove, GenieError> {
        if self.unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "slot 0x40 of an action the slice does not hold",
            ));
        }
        match self.top_action_kind() {
            None
            | Some(
                crate::movement::move_to::MOVE_TO
                | crate::movement::explore::EXPLORE
                | action_type::BUILD
                | crate::combat::attack::ATTACK
                | crate::garrison::ENTER
                | crate::movement::fly::FLY
                | crate::combat::missile::MISSILE
                | crate::gathering::make::MAKE
                | crate::construction::wait_solidify::WAIT_SOLIDIFY,
            ) => Ok(PointMove::Declined),
            Some(crate::ownership::artifact::ARTIFACT_TYPE) => {
                let sprites = self.point_sprites(masters, graphics)?;
                self.artifact_takes_point(point, &sprites)
            }
            Some(crate::gathering::hunt::HUNT) => {
                let sprites = self.point_sprites(masters, graphics)?;
                self.hunt_takes_point(point, player, sprites)
            }
            Some(crate::gathering::gather::GATHER) => {
                let sprites = self.point_sprites(masters, graphics)?;
                self.gather_takes_point(point, player, sprites)
            }
            // the Transport's (0x140aa3e40): the caller sails it there
            Some(crate::garrison::transport::TRANSPORT) => Ok(PointMove::Sails),
            Some(crate::construction::repair::REPAIR) => {
                let move_reads = MoveToReads::of(self, self.master_in(masters)?, graphics);
                self.repair_takes_point(point, &move_reads)
            }
            Some(other) => Err(GenieError::unreachable(
                "a top action of a type top_action never holds",
            )
            .with("type", other)),
        }
    }

    /// The sprites a `MoveTo` to a point that the unit's head action makes takes for none
    /// (`FUN_140aa0d30`'s default from the unit's master and terrain look).
    fn point_sprites<'g>(
        &self,
        masters: Option<&Masters>,
        graphics: &'g std::collections::BTreeMap<SpriteId, Graphic>,
    ) -> Result<SpriteReads<'g>, GenieError> {
        Ok(SpriteReads::of(
            self.master_in(masters)?,
            self.abilities.look.as_ref(),
            graphics,
        ))
    }

    /// The Artifact's slot 0x40 (its slot 8 with the point, [`Artifact::take_point`]): its
    /// sub-actions go, a walking one stopping the unit, and the Enter it held lets its building
    /// or transport go.
    ///
    /// [`Artifact::take_point`]: crate::ownership::artifact::Artifact::take_point
    fn artifact_takes_point(
        &mut self,
        point: Position,
        sprites: &SpriteReads<'_>,
    ) -> Result<PointMove, GenieError> {
        let reads = MoveToReads::with_sprites(*sprites, self);
        let gone = match self.actions.artifact_mut() {
            Some(a) => a.take_point(point, &reads)?,
            None => crate::ownership::artifact::SubActionsGone::default(),
        };
        if gone.stops {
            self.body
                .mover_mut()
                .ok_or(crate::world::WALKER_UNMOVING)?
                .stop()?;
        }
        Ok(PointMove::Taken(gone.released))
    }

    /// The Hunt's slot 0x40 ([`HuntAction::take_move`] with the point): the target it had is
    /// let go, unless the hunt declines the move.
    ///
    /// [`HuntAction::take_move`]: crate::gathering::hunt::HuntAction::take_move
    fn hunt_takes_point(
        &mut self,
        point: Position,
        player: PlayerId,
        sprites: SpriteReads<'_>,
    ) -> Result<PointMove, GenieError> {
        let my_pasture = self.my_pasture;
        let walks = Walks::of(sprites, self);
        let (Some(hunt), Some(mover)) = (self.actions.hunt_mut(), self.body.mover_mut()) else {
            return Err(GenieError::unsupported(
                "a hunter without its hunt or movement state",
            ));
        };
        let old = hunt.target;
        if !hunt.take_move(
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
        )? {
            return Ok(PointMove::Declined);
        }
        Ok(PointMove::Taken(old))
    }

    /// The Gather's slot 0x40 (its move to the point): the target it had is let go; the
    /// `MoveTo` it makes takes `sprites`' default for a sprite the task lacks.
    fn gather_takes_point(
        &mut self,
        point: Position,
        player: PlayerId,
        sprites: SpriteReads<'_>,
    ) -> Result<PointMove, GenieError> {
        let walks = Walks::of(sprites, self);
        let (Some(gather), Some(mover)) = (self.actions.gather_mut(), self.body.mover_mut()) else {
            return Err(GenieError::unsupported(
                "a gatherer without its gather or movement state",
            ));
        };
        let old = gather.target;
        gather.take_move(None, point, player, mover, &walks)?;
        Ok(PointMove::Taken(old))
    }

    /// The Repair's slot 0x40 (`FUN_140ad9700`, [`RepairAction::take_point`]): its point, slot
    /// 0x68 with none, `set_state(11)`, whose `MoveTo` reads `move_reads`. Returns
    /// the target let go and the one whose repairers it leaves.
    ///
    /// [`RepairAction::take_point`]: crate::construction::repair::RepairAction::take_point
    fn repair_takes_point(
        &mut self,
        point: Position,
        move_reads: &MoveToReads<'_>,
    ) -> Result<PointMove, GenieError> {
        let repair = self
            .actions
            .repair_mut()
            .ok_or(GenieError::unreachable("a Repair head without its Repair"))?;
        let (old, counted) = repair.take_point(point, self.body.mover_mut(), move_reads)?;
        Ok(PointMove::Repairs(old, old.filter(|_| counted)))
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
        (target, state): (ObjectId, crate::objects::ObjectState),
        range: Distance,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        if self.unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "an AI move to an object of a unit with an action",
            ));
        }
        if self.body.mover().is_none() {
            return Err(GenieError::unsupported(
                "an AI move to an object of a unit that does not move",
            ));
        }
        let livestock = self.master(&c.w.masters, c.player)?.class == class::LIVESTOCK;
        let master =
            c.w.spatial
                .get_known(target)?
                .ok_or(crate::movement::move_to::TARGET_LACKING)?
                .master;
        let by = self.releaser(c)?;
        if crate::movement::move_to::holds_object(livestock, master, state) {
            crate::world::lets::take(c, self, target, by)?;
        }
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        self.clear_own_actions(&mut releases, &mut leaves, by)?;
        crate::world::lets::torn_down(c, self, (leaves, releases))?;
        let own = self.master(&c.w.masters, c.player)?;
        let range = crate::movement::move_to::object_range(range, Some(own));
        let sprite = ai_sprite(own, false)
            .or_else(|| object_sprite(own))
            .and_then(|id| c.w.graphics.get(&id).copied());
        let here = self.position();
        let mover = self.body.mover_mut().expect("tested above");
        let move_to = MoveToAction::to_object(
            &c.w.spatial,
            here,
            mover,
            (target, state),
            Distance::new(range),
            (sprite, livestock),
        )?;
        if move_to.shows_standing() {
            self.shows_standing_sprite(
                c.w.masters.get(c.player.index()),
                &c.w.graphics,
                c.w.streams.get(Stream::Sprite),
            )?;
        }
        self.actions.set_move_to(Some(move_to));
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
        waits: bool,
        stance: crate::unit_ai::UnitStance,
        (masters, graphics, sprite_rng): (
            Option<&Masters>,
            &std::collections::BTreeMap<SpriteId, Graphic>,
            &mut Well512,
        ),
    ) -> Result<(), GenieError> {
        let Some(m) = self.actions.move_to_mut() else {
            return Ok(());
        };
        m.state = m.wait.give_up(waits, stance);
        if m.state == MoveTo::Failed {
            self.shows_standing_sprite(masters, graphics, sprite_rng)?;
        }
        Ok(())
    }

    /// A `MoveTo`'s `set_state` 1, not quiet, on the unit (`0x140aa29e0`): the master's standing
    /// sprite where the slice holds the unit's sprite (slot 0x68 with slot 0x798's, a draw of the
    /// Sprite stream), and its animation speed (`+0x7b8`) cleared where known.
    pub(crate) fn shows_standing_sprite(
        &mut self,
        masters: Option<&Masters>,
        graphics: &std::collections::BTreeMap<SpriteId, Graphic>,
        sprite_rng: &mut Well512,
    ) -> Result<(), GenieError> {
        let standing = self
            .master_in(masters)?
            .standing_sprite
            .and_then(|s| graphics.get(&s))
            .copied();
        if let Some(m) = self.body.mover_mut() {
            m.shows(standing.as_ref());
        }
        if self.sprite.is_some() {
            let standing = standing.ok_or(GenieError::unsupported(
                "an arrived walker without its standing sprite",
            ))?;
            crate::objects::sprite::new_sprite(
                &mut self.sprite,
                self.body.move_stats_mut(),
                standing,
                sprite_rng,
            );
        }
        self.body.set_anim_speed(0.0);
        Ok(())
    }
}

/// What the head action's slot 0x40 did with a move to a point ([`Unit::head_takes_point`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// with the map ([`Unit::transport_sail`]).
    Sails,
}
