//! A made unit's spawn, the command queue it takes, its unit AI and the combat unit's
//! constructor.

use crate::commands::GatherPointState;
use crate::garrison::ContainerId;
use crate::map::Distance;
use crate::map::Position;
use crate::map::spatial::{Obstruction, Placed};
use crate::math::Angle;
use crate::math::rng::Stream;
use crate::movement::MoveState;
use crate::movement::Mover;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::orders::Dispatch;
use crate::orders::OrderSource;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::production::QueuedUnits;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::unit_ai_kind;
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::world::parts::{Object, ObjectParts};
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

use crate::combat::missile::grounded;
use crate::objects::facing::{SpriteFacets, drawn_angle, draws_angle};
use crate::objects::layers::{Action, Combat, Motion, Static, default_start};
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::production::spawn::objects::RECYCLABLE_NEW_OBJECT;
use crate::technology::Attributes;
use crate::unit_ai::AiAction;
use crate::unit_ai::{NewObject, Notify, UnitAi};
use crate::unit_ai::{OrderPriority, QueuedOrder};
use crate::vision::visibility::{Seer, capture_kind};
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;

/// What a make action's spawn did (`FUN_140ad0240`'s answer).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(super) enum SpawnOutcome {
    /// Its unit came beside the building: the game's 1.
    Made,
    /// No spot was free, nor a point inside the footprint: the game's 0, before anything
    /// changed.
    NoSpot,
}

/// A computer player's new unit (its AI's slot 0x398).
const COMPUTER_NEW_UNIT: GenieError =
    GenieError::unsupported("a computer player's new unit (its AI's slot 0x398)");
/// A spawn without the players' sight.
const SPAWN_UNSIGHTED: GenieError = GenieError::unsupported("a spawn without the players' sight");
/// A new unit of a master that does not move.
const NEW_UNIT_UNMOVING: GenieError =
    GenieError::unsupported("a new unit of a master that does not move");

impl World {
    /// `FUN_140ad0240`: the unit of `master` spawns beside the building at `list_index`. The spawn
    /// point ([`spawn_point`], from `FUN_140ace630`), then the master's slot 0x40 at the building's
    /// height (the ground's then, [`World::combat_object`]); without a spot nothing changes
    /// ([`SpawnOutcome::NoSpot`]). Then attribute 81 lowers by its
    /// population, the building notifies its unit AI 506 with 619, and the gather point's work
    /// comes (`FUN_140ace9c0`): `+0x7f9` (waiting to move) set, the work command on the gather
    /// point's target at the point (slot 0x128 with 1). A building without a gather point
    /// (`+0xc30` 0) spawns the unit idle, its spot facing nothing; one whose unit AI holds a
    /// command queue (slot 0x218) is not modelled.
    ///
    /// The player's tracker lets its queue place go ([`crate::players::tracker::Tracker::unplace`], the
    /// counter one less).
    ///
    /// Not held: its facet and angle, its powerup offset, the building's `+0xc1c` group (unsaved), the game's events
    /// (`FUN_140c898a0`), the per-master counts (slot 0x398's `+0x13b0`), the sounds and
    /// particles (`FUN_1411a52e0`).
    ///
    /// [`spawn_point`]: crate::production::spawn::search::spawn_point
    pub(super) fn spawn(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        master: MasterId,
        pop: Amount,
    ) -> GenieResult<SpawnOutcome> {
        if self.players.player(player_id)?.kind.is_computer() {
            return Err(COMPUTER_NEW_UNIT);
        }
        if self.spatial.sight.is_none() {
            return Err(SPAWN_UNSIGHTED);
        }
        let building = &self.players.live(player_id, list_index)?;
        let made = self.master(player_id, master)?.clone();
        if made.recyclable.recycled() {
            return Err(RECYCLABLE_NEW_OBJECT);
        }
        // the building unit AI's command queue (`+0x1f0`, saved as `savedOrder`): without a
        // gather point its first location is the spot's goal (FUN_1411d12d0, slot 0x220)
        let queue = self
            .parts
            .unit_ai
            .get(building.id)
            .map(|unit_ai| unit_ai.building_orders.clone())
            .unwrap_or_default();
        let gather = self.parts.gather_point.get(building.id).copied();
        let position = building.position();
        let toward = match (gather, queue.first()) {
            (None, Some(first)) => Some(first.point.ground()),
            _ => gather
                .filter(|gather_point| gather_point.target != Some(building.id))
                .map(|gather_point| gather_point.point.ground()),
        };
        if made.move_stats.is_none() {
            return Err(NEW_UNIT_UNMOVING);
        }
        let Some(point) = self.spot_beside(player_id, list_index, &made, toward)? else {
            // `FUN_140ace630`'s 0: the make action waits for a spot (its caller's
            // `set_state(10)`); the game's event (`FUN_140c898a0` with 0x68) is the interface's
            return Ok(SpawnOutcome::NoSpot);
        };
        let new = self.combat_object(player_id, master, &made, point.with_height(position.z()))?;
        let class = self.tracker_key(player_id, master).class;
        let owner = self.players.player_mut(player_id)?;
        owner.add_attribute(
            self.model.slots.attribute.in_production,
            -pop,
            &self.model.roles,
        );
        // its queue place goes (FUN_14104fb00 with -1, FUN_14104f330)
        owner.tracker.count_queued(QueuedUnits::ONE.negated());
        owner.tracker.unplace(Some(master), class);
        let building = owner.objects.checked_mut(list_index)?;
        let (building_id, state) = (building.id, building.life.state);
        if let Some(ai) = self.parts.unit_ai.get_mut(building.id) {
            ai.post(
                Notify::own(
                    building_id,
                    NotifyKind::ACTION_COMPLETED,
                    [AiAction::Make.value(), 0, 0],
                ),
                state,
            )?;
        }
        if !queue.is_empty() {
            // FUN_140ace9c0 with a command queue: the building unit AI's slot 0x200 hands it to
            // the new unit (FUN_140c15e50), its gather point's work skipped
            self.hand_queue(player_id, new, &queue, position.z())?;
        } else if let Some(gather) =
            gather.filter(|gather_point| gather_point.state == GatherPointState::Point)
        {
            // only a state of 1 sends it (`FUN_140ace9c0`'s `+0xc30 != 1` return)
            self.gather(player_id, new, gather, building_id, OrderSource::Direct)?;
        }
        // FUN_140ace760: the new unit's tasks that spawn more (action type 0x9c)
        let now = self.players.live(player_id, new)?.life.master_id;
        if self
            .master(player_id, now)?
            .action_part()
            .tasks
            .iter()
            .any(|task| task.action_kind == ActionKind::ACTION_156)
        {
            return Err(GenieError::unsupported(
                "a new unit's task that spawns more units",
            ));
        }
        Ok(SpawnOutcome::Made)
    }

    /// `FUN_140c15e50`: each order of a building's command queue goes into the unit AI's queue of
    /// the new unit at `new` (slot 0x30 into slot 0x28: not immediate, behind, priority 100,
    /// ready; its point the order's x and y at the building's height `z`, range 0). An order on
    /// a monastery (master 104) of a unit whose AI already queues one of target type 42 keeps its
    /// kind; on another object the unit may enter (`FUN_14118cd80`: type 70 and up with a
    /// capacity, `FUN_14120eb10` with its room) it would become an enter order (`0x2cd`), not
    /// modelled. The shown point (the game passes stack bytes there) is not held.
    pub(super) fn hand_queue(
        &mut self,
        player_id: PlayerId,
        new: ListIndex,
        queue: &[QueuedOrder],
        z: f32,
    ) -> GenieResult<()> {
        let master = self.players.live(player_id, new)?.life.master_id;
        for queued_order in queue {
            self.queued_target(player_id, new, master, queued_order)?;
            let unit = self.players.live_mut(player_id, new)?;
            let state = unit.life.state;
            if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
                let order = QueuedOrder {
                    priority: OrderPriority::COMMAND,
                    point: queued_order.point.with_z(z),
                    range: Distance::ZERO,
                    ready: true,
                    ..*queued_order
                };
                let _ = ai.queue_order(state, order, Dispatch::Queued, QueueEnd::Back)?;
            }
        }
        Ok(())
    }

    /// [`World::hand_queue`]'s test of a queued order's target `order.target` for the new unit at
    /// `new` (of master `master`): an order on a monastery (104) of a unit whose AI already queues
    /// one of the relic's target type keeps its kind; on another object the unit may enter it
    /// would become an enter order (`0x2cd`), not modelled. An order without a target, or on one
    /// the world does not hold, passes.
    fn queued_target(
        &self,
        player_id: PlayerId,
        new: ListIndex,
        master: MasterId,
        order: &QueuedOrder,
    ) -> GenieResult<()> {
        let target = order
            .target
            .is_some()
            .then(|| self.object(order.target))
            .flatten()
            .map(|(tp, unit)| (tp, unit.life.master_id));
        let Some((tp, tm)) = target else {
            return Ok(());
        };
        let ai = self
            .parts
            .unit_ai
            .get(self.players.live(player_id, new)?.id);
        // The monastery, which a unit carrying a relic's order keeps (`FUN_141188e10`): an order
        // whose type, read as a class, is the relic's (`FUN_140c417f0` with 0x2a)
        let relic_order = |queued_order: &QueuedOrder| {
            queued_order.target_kind.is_some_and(|order_target_kind| {
                self.model
                    .roles
                    .of_class(order_target_kind.class())
                    .contains(ClassRoles::RELIC)
            })
        };
        let keeps = self
            .model
            .roles
            .of_master(tm)
            .contains(MasterRoles::MONASTERY)
            && ai.is_some_and(|ai| ai.orders.iter().any(relic_order));
        let target_master = self.master(tp, tm)?;
        if !keeps
            && MasterKind::COMBAT <= target_master.kind
            && target_master.garrison.holds()
            // a queued order's target −1 is a point: it takes nothing in
            && order.target
                .map_or(Ok(false), |object_id| self.takes_in(ContainerId::new(object_id), player_id, Some(master)))?
        {
            return Err(GenieError::unsupported(
                "a command queue's order on a building the unit may enter (its room, slot 0x198)",
            ));
        }
        Ok(())
    }

    /// What the unit AI of player `player_id`'s new object of master `made` (id `master`) at
    /// `position` reads ([`NewObject`]): its master's fields and its owner's type, posture
    /// (`+0x71c`) and current age (attribute 6).
    ///
    /// # Errors
    ///
    /// A player without attribute 6.
    pub(crate) fn new_object(
        &self,
        player_id: PlayerId,
        master: MasterId,
        made: &Master,
        position: Position,
    ) -> GenieResult<NewObject> {
        let owner = self.players.player(player_id)?;
        Ok(NewObject {
            player_kind: owner.kind,
            posture: owner.posture,
            // The player's attribute a new soldier's unit AI reads: the current age (`player+0x100` at
            // `+0x30`, `FUN_140c12300`).
            age: Attributes::get(owner, self.model.slots.attribute.current_age)?,
            ..NewObject::of(position, master, made)
        })
    }

    /// The static, animated and moving setups' draws (`FUN_1411a45a0`, `FUN_141072690`,
    /// `FUN_141161450`) of a new object whose current sprite is `standing` (slot 0x798: its
    /// master's standing sprite, a building's too, `FUN_1412029f0`): a sprite with a random first
    /// frame and several facets draws its facet (Object stream), the sprite starts (Sprite stream,
    /// [`ActiveSprite::start`]), and one of several facets with sequence flag 2 draws the angle the
    /// object faces (Object stream, "setup" line 0x2c3). Its sprite, its facets and its angle.
    pub(crate) fn setup_draws(&mut self, standing: Graphic) -> (ActiveSprite, SpriteFacets, Angle) {
        if standing.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) && standing.facets.several() {
            let _ = self.streams.get_mut(Stream::Object).rand15();
        }
        let sprite = ActiveSprite::start(
            standing,
            self.streams.get_mut(Stream::Sprite),
            self.model.tuning.sprite,
        );
        let facets = SpriteFacets {
            count: standing.facets,
            turns: standing.flags.contains(SpriteFlags::TURNS),
        };
        let angle = if draws_angle(Some(facets)) {
            let draw = self.streams.get_mut(Stream::Object).rand15();
            drawn_angle(draw, facets)
        } else {
            Angle::ZERO
        };
        (sprite, facets, angle)
    }

    /// The master's slot 0x40 for a combat unit (type 70, `0x141266120`, the constructor
    /// `FUN_1411eae40` with its setups) of `master` for its player at `position`: the setups in
    /// order (static `FUN_1411a45a0`, animated `FUN_141072690`, moving `FUN_141161450`, combat
    /// `FUN_14121df60`) with their draws: the facet (Object stream, for a standing sprite with a
    /// random first frame and several facets), the standing sprite ([`ActiveSprite::start`],
    /// Sprite stream), the angle (Object stream, sequence flag 2) and the unit AI
    /// ([`UnitAi::constructed`], three AI-stream draws). The object takes its place (its height
    /// the ground's, by its master's fly mode), its record and mesh shape, its sight and its
    /// storage (flags 1, 2 and 32, `FUN_141193860`), then the combat setup's writes and counts
    /// ([`World::combat_setup`], the powerup offset's draw on the Object stream). It joins its
    /// player's list last; its index there.
    ///
    /// # Errors
    ///
    /// A computer player's unit, a world without the players' sight, a recyclable master, one
    /// that does not move or has no unit AI or standing sprite, and what the combat setup
    /// fails on.
    #[expect(
        clippy::too_many_lines,
        reason = "a combat unit's making (`0x141266120`, the constructor `FUN_1411eae40`) ported as one function, its setups and draws in the game's order"
    )]
    pub(crate) fn combat_object(
        &mut self,
        player_id: PlayerId,
        master: MasterId,
        made: &Master,
        position: Position,
    ) -> GenieResult<ListIndex> {
        if self.players.player(player_id)?.kind.is_computer() {
            return Err(COMPUTER_NEW_UNIT);
        }
        if self.spatial.sight.is_none() {
            return Err(SPAWN_UNSIGHTED);
        }
        if made.recyclable.recycled() {
            return Err(RECYCLABLE_NEW_OBJECT);
        }
        let moving = made.move_stats.ok_or(NEW_UNIT_UNMOVING)?;
        // the constructor's new_position: the ground's height by the master's fly mode (slot
        // 0x60, FUN_141193a10)
        let position = self.spatial.terrain.as_ref().map_or(position, |terrain| {
            terrain
                .ground(position.ground())
                .map_or(position, |ground| {
                    Position::new(
                        position.x(),
                        position.y(),
                        grounded(position.z(), ground, made.fly_mode),
                    )
                })
        });
        let ai_kind = unit_ai_kind(made, master, &self.model.roles).ok_or(
            GenieError::unsupported("a new combat unit without a unit AI"),
        )?;
        let new_object = self.new_object(player_id, master, made, position)?;
        let standing = made
            .standing_sprite
            .and_then(|sprite_id| self.model.graphics.get(sprite_id).copied())
            .ok_or(GenieError::unsupported(
                "a new unit's standing sprite the data file does not have",
            ))?;
        let id = self.take_object_id(player_id, master);
        let (sprite, facets, angle) = self.setup_draws(standing);
        let ai = UnitAi::constructed(
            ai_kind,
            &new_object,
            self.streams.get_mut(Stream::Ai),
            &self.model.roles,
        );
        // its place, record, mesh shape and sight (new_position, slot 0x58, and FUN_141054fe0)
        let solid = 0.0 < made.radius.x() && 0.0 < made.radius.y() && made.is_tall();
        let placed = Placed {
            position,
            radius: made.radius,
            class: made.class,
            player: player_id,
            master,
            obstruction_kind: made.obstruction_kind,
            buildable_on: made.buildable_on,
            tall: made.is_tall(),
            moves: made.moves(),
            unknown: None,
            obstruction: (made.obstruction_kind != ObstructionKind::NONE && solid).then_some(
                Obstruction {
                    kind: made.obstruction_kind,
                    class: made.obstruction_class,
                },
            ),
        };
        self.spatial.insert(id, placed);
        self.pathing.moved(id, &placed, MoveState::Standing);
        self.pathing.tiles_changed(self.spatial.take_changes());
        if let Some(visibility) = self.spatial.sight.as_mut() {
            visibility.add_seer(
                id,
                Seer {
                    player: player_id,
                    master,
                    los: made.line_of_sight,
                    los_powerup: Distance::ZERO,
                    state: ObjectState::Alive,
                    explored: false,
                    building: None,
                    umv: 0,
                    capture: capture_kind(player_id, made.class, master, &self.model.roles),
                    kind: made.kind,
                },
            );
            let _ = visibility.sight_on(
                id,
                position,
                made.radius,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles: &self.model.roles,
                    visibility_tuning: self.model.tuning.visibility,
                },
            )?;
        }
        if let Some(error) = self.take_sight_unsupported() {
            return Err(error);
        }
        // the static setup's storage (FUN_1411a45a0's FUN_141193860), before the combat setup
        self.give_static_storage(player_id, &made.storage)?;
        let setup = self.combat_setup(player_id, made)?;
        let combat = Combat {
            firepower: setup.firepower,
            charge: setup.charge,
            decay_timer: setup.decay_timer,
            powerup_update: setup.powerup_update,
            ..Default::default()
        };
        let faith = setup.faith;
        let unit = Unit {
            id,
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Alive,
                hp: HitPoints::of_master(made.hit_points),
                attribute_kind: faith.map(|(kind, _)| kind),
                amount: faith.map_or(Amount::ZERO, |(_, amount)| amount),
                death_facets: None,
                inside: None,
                goto_sleep: false,
                master_id: master,
            },
            place: Static::at(position),
            flags: UnitFlags::ATTACKABLE,
            pasture: None,
            my_pasture: None,
            sprite: Some(sprite),
            ..Unit::default()
        };
        let parts = ObjectParts {
            actions: Action {
                // what its master's default task starts once its action list is empty
                default_task: default_start(made),
                ..Default::default()
            },
            unit_ai: Some(ai),
            combat,
            // FUN_1411580a0: facing x (an immediate 1.0), its right -z, not waiting; the animated
            // object's constructor (FUN_141070cf0): animation speed (+0x7b8) 0
            motion: Some(Motion::Moving(Mover::new_object(
                moving,
                Some(&standing),
                position,
                self.world_time,
                angle,
                Some(facets),
                &self.model.tuning.spawn,
            ))),
            ..ObjectParts::default()
        };
        let idx = self.push_live(player_id, Object { unit, parts })?;
        // the static init's add to its player (slot 0x2d0) files it in the tracker
        self.track_add(player_id, id, master)?;
        Ok(idx)
    }
}
