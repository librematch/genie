//! A made unit's spawn, the command queue it takes, its unit AI and the combat unit's
//! constructor.

use crate::GenieError;
use crate::commands::GatherPointState;
use crate::garrison::ContainerId;
use crate::map::Distance;
use crate::map::Position;
use crate::map::spatial::{Obstruction, Placed};
use crate::math::Angle;
use crate::math::rng::Stream;
use crate::movement::Mover;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::MasterId;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::class;
use crate::objects::masters::ids::MONASTERY;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::attribute::IN_PRODUCTION;
use crate::production::QueuedUnits;
use crate::unit_ai::NotifyKind;
use crate::world::parts::{Object, ObjectParts};
use crate::world::slots::PlayerSlots;

use crate::unit_ai::ActionKind;
use crate::unit_ai::{NewObject, Notify, UnitAi};
use crate::vision::visibility::{Seer, capture_kind};
use crate::world::World;

impl World {
    /// `FUN_140ad0240`: the unit of `master` spawns beside the building at `i`. The spawn point
    /// ([`spawn_point`], from `FUN_140ace630`), then the master's slot 0x40 at the building's
    /// height (the ground's then, [`World::combat_object`]). Then attribute 81 lowers by its
    /// population, the building notifies its unit AI 506
    /// with 619, and the gather point's work comes (`FUN_140ace9c0`): `+0x7f9` (waiting to move)
    /// set, the work command on the gather point's target at the point (slot 0x128 with 1). A
    /// building without a gather point (`+0xc30` 0) spawns the unit idle, its spot facing nothing;
    /// one whose unit AI holds a command queue (slot 0x218) is not modelled.
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
        p: PlayerId,
        i: ListIndex,
        master: MasterId,
        pop: Amount,
    ) -> Result<(), GenieError> {
        if self.players.player(p)?.computer {
            return Err(GenieError::unsupported(
                "a computer player's new unit (its AI's slot 0x398)",
            ));
        }
        if self.spatial.sight.is_none() {
            return Err(GenieError::unsupported(
                "a spawn without the players' sight",
            ));
        }
        let building = &self.players.live(p, i)?;
        let made = self.master(p, master)?.clone();
        if made.recyclable {
            return Err(GenieError::unsupported(
                "a new object of a recyclable master (a negative id)",
            ));
        }
        // the building unit AI's command queue (`+0x1f0`, saved as `savedOrder`): without a
        // gather point its first location is the spot's goal (FUN_1411d12d0, slot 0x220)
        let queue = self
            .parts
            .unit_ai
            .get(building.id)
            .map(|a| a.building_orders.clone())
            .unwrap_or_default();
        let gather = self.parts.gather_point.get(building.id).copied();
        let position = building.position();
        let toward = match (gather, queue.first()) {
            (None, Some(first)) => Some(first.point.ground()),
            _ => gather
                .filter(|g| g.target != Some(building.id))
                .map(|g| g.point.ground()),
        };
        if made.moving.is_none() {
            return Err(GenieError::unsupported(
                "a new unit of a master that does not move",
            ));
        }
        let point = self.spot_beside(p, i, &made, toward)?;
        let new = self.combat_object(p, master, &made, point.with_height(position.z()))?;
        let class = self.tracker_key(p, master).class;
        let owner = self.players.player_mut(p)?;
        owner.add_attribute(IN_PRODUCTION, -pop)?;
        // its queue place goes (FUN_14104fb00 with -1, FUN_14104f330)
        owner.tracker.count_queued(QueuedUnits::ONE.negated());
        owner.tracker.unplace(Some(master), class);
        let building = owner.objects.checked_mut(i)?;
        let (building_id, state) = (building.id, building.life.state);
        if let Some(ai) = self.parts.unit_ai.get_mut(building.id) {
            ai.post(
                Notify::own(
                    building_id,
                    NotifyKind::ACTION_COMPLETED,
                    [ActionKind::Make.value(), 0, 0],
                ),
                state,
            )?;
        }
        if !queue.is_empty() {
            // FUN_140ace9c0 with a command queue: the building unit AI's slot 0x200 hands it to
            // the new unit (FUN_140c15e50), its gather point's work skipped
            self.hand_queue(p, new, &queue, position.z())?;
        } else if let Some(gather) = gather.filter(|g| g.state == GatherPointState::POINT) {
            // only a state of 1 sends it (`FUN_140ace9c0`'s `+0xc30 != 1` return)
            self.gather(p, new, gather, building_id, true, true)?;
        }
        // FUN_140ace760: the new unit's tasks that spawn more (action type 0x9c)
        let now = self.players.live(p, new)?.life.master_id;
        if self
            .master(p, now)?
            .tasks
            .iter()
            .any(|t| t.action_type == crate::objects::masters::action_type::ACTION_156)
        {
            return Err(GenieError::unsupported(
                "a new unit's task that spawns more units",
            ));
        }
        Ok(())
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
        p: PlayerId,
        new: ListIndex,
        queue: &[crate::unit_ai::QueuedOrder],
        z: f32,
    ) -> Result<(), GenieError> {
        let master = self.players.live(p, new)?.life.master_id;
        for o in queue {
            let target = o
                .target
                .is_some()
                .then(|| self.object(o.target))
                .flatten()
                .map(|(tp, t)| (tp, t.life.master_id));
            if let Some((tp, tm)) = target {
                let ai = self.parts.unit_ai.get(self.players.live(p, new)?.id);
                // The monastery, which a unit carrying a relic's order keeps (`FUN_141188e10`):
                // an order whose type, read as a class, is the relic's (`FUN_140c417f0` with 0x2a)
                let relic_order = |q: &crate::unit_ai::QueuedOrder| {
                    q.target_type.is_some_and(|t| t.class() == class::RELIC)
                };
                let keeps =
                    tm == MONASTERY && ai.is_some_and(|ai| ai.orders.iter().any(relic_order));
                let t_master = self.master(tp, tm)?;
                if !keeps
                    && COMBAT <= t_master.kind
                    && t_master.garrison.holds()
                    // a queued order's target −1 is a point: it takes nothing in
                    && o.target
                        .map_or(Ok(false), |t| self.takes_in(ContainerId::new(t), p, Some(master)))?
                {
                    return Err(GenieError::unsupported(
                        "a command queue's order on a building the unit may enter (its room, slot 0x198)",
                    ));
                }
            }
            let unit = self.players.live_mut(p, new)?;
            let state = unit.life.state;
            if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
                let order = crate::unit_ai::QueuedOrder {
                    priority: crate::unit_ai::OrderPriority::COMMAND,
                    point: o.point.with_z(z),
                    range: Distance::ZERO,
                    ready: true,
                    ..*o
                };
                let _ = ai.queue_order(state, order, false, false)?;
            }
        }
        Ok(())
    }

    /// What the unit AI of player `p`'s new object of master `made` (id `master`) at `position`
    /// reads ([`NewObject`]): its master's fields and its owner's type, posture (`+0x71c`) and
    /// current age (attribute 6).
    ///
    /// # Errors
    ///
    /// A player without attribute 6.
    pub(crate) fn new_object(
        &self,
        p: PlayerId,
        master: MasterId,
        made: &crate::objects::masters::Master,
        position: Position,
    ) -> Result<NewObject, GenieError> {
        let owner = self.players.player(p)?;
        Ok(NewObject {
            computer: owner.computer,
            posture: owner.posture,
            // The player's attribute a new soldier's unit AI reads: the current age (`player+0x100` at
            // `+0x30`, `FUN_140c12300`).
            age: crate::technology::Attributes::get(owner, attribute::CURRENT_AGE)?,
            ..NewObject::of(position, master, made)
        })
    }

    /// The master's slot 0x40 for a combat unit (type 70, `0x141266120`, the constructor
    /// `FUN_1411eae40` with its setups) of `master` for player `p` at `position`: the setups in
    /// order (static `FUN_1411a45a0`, animated `FUN_141072690`, moving `FUN_141161450`, combat
    /// `FUN_14121df60`) with their draws: the facet (Object stream, for a standing sprite with a
    /// random first frame and several facets), the standing sprite ([`ActiveSprite::start`],
    /// Sprite stream), the angle (Object stream, sequence flag 2), the unit AI
    /// ([`UnitAi::constructed`], three AI-stream draws) and the powerup offset (Object stream).
    /// The object takes its place (its height the ground's, by its master's fly mode), its record
    /// and mesh shape, its sight and its storage (flags 1, 2 and 32, `FUN_141193860`); its
    /// player's attributes 98, 164 and 246 grow by its paid cost (246 while the telemetry's
    /// match runs: `FUN_140982ff0()`'s `+0x921`, which every game's start sets, `FUN_140f1fb20`
    /// with `FUN_140980ac0`, and only its teardown `FUN_140c85fb0` clears, `FUN_140980de0`) and
    /// a villager's (class 4) attribute 248 by one, and its class counts
    /// ([`crate::objects::lifecycle::death_parts::count_class`]). It joins its player's list last; its
    /// index there.
    /// The static, animated and moving setups' draws (`FUN_1411a45a0`, `FUN_141072690`,
    /// `FUN_141161450`) of a new object whose current sprite is `standing` (slot 0x798: its
    /// master's standing sprite, a building's too, `FUN_1412029f0`): a sprite with a random first
    /// frame and several facets draws its facet (Object stream), the sprite starts (Sprite stream,
    /// [`ActiveSprite::start`]), and one of several facets with sequence flag 2 draws the angle the
    /// object faces (Object stream, "setup" line 0x2c3). Its sprite, its facets and its angle.
    pub(crate) fn setup_draws(
        &mut self,
        standing: Graphic,
    ) -> (ActiveSprite, crate::objects::facing::SpriteFacets, Angle) {
        if standing.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) && standing.facets.several() {
            let _ = self.streams.get(Stream::Object).rand15();
        }
        let sprite = ActiveSprite::start(standing, self.streams.get(Stream::Sprite));
        let facets = crate::objects::facing::SpriteFacets {
            count: standing.facets,
            turns: standing.flags.contains(SpriteFlags::TURNS),
        };
        let angle = if crate::objects::facing::draws_angle(Some(facets)) {
            let r = self.streams.get(Stream::Object).rand15();
            crate::objects::facing::drawn_angle(r, facets)
        } else {
            Angle::ZERO
        };
        (sprite, facets, angle)
    }

    pub(crate) fn combat_object(
        &mut self,
        p: PlayerId,
        master: MasterId,
        made: &crate::objects::masters::Master,
        position: Position,
    ) -> Result<ListIndex, GenieError> {
        if self.players.player(p)?.computer {
            return Err(GenieError::unsupported(
                "a computer player's new unit (its AI's slot 0x398)",
            ));
        }
        if self.spatial.sight.is_none() {
            return Err(GenieError::unsupported(
                "a spawn without the players' sight",
            ));
        }
        if made.recyclable {
            return Err(GenieError::unsupported(
                "a new object of a recyclable master (a negative id)",
            ));
        }
        let moving = made.moving.ok_or(GenieError::unsupported(
            "a new unit of a master that does not move",
        ))?;
        // the constructor's new_position: the ground's height by the master's fly mode (slot
        // 0x60, FUN_141193a10)
        let position = self.spatial.terrain.as_ref().map_or(position, |t| {
            t.ground(position.ground()).map_or(position, |g| {
                Position::new(
                    position.x(),
                    position.y(),
                    crate::combat::missile::grounded(position.z(), g, false, made.fly_mode),
                )
            })
        });
        let ai_type = made.unit_ai_type(master).ok_or(GenieError::unsupported(
            "a new combat unit without a unit AI",
        ))?;
        let new_object = self.new_object(p, master, made, position)?;
        let standing = made
            .standing_sprite
            .and_then(|s| self.graphics.get(s).copied())
            .ok_or(GenieError::unsupported(
                "a new unit's standing sprite the data file does not have",
            ))?;
        let id = self.take_object_id(p, master);
        let (sprite, facets, angle) = self.setup_draws(standing);
        let ai = UnitAi::constructed(ai_type, &new_object, self.streams.get(Stream::Ai));
        let _ = self.streams.get(Stream::Object).rand15();
        // its place, record, mesh shape and sight (new_position, slot 0x58, and FUN_141054fe0)
        let solid = 0.0 < made.radius.x() && 0.0 < made.radius.y() && made.tall;
        let placed = Placed {
            position,
            radius: made.radius,
            class: made.class,
            player: p,
            master,
            obstruction_type: made.obstruction_type,
            buildable_on: made.buildable_on,
            tall: made.tall,
            moves: made.moves(),
            unknown: None,
            obstruction: (made.obstruction_type != ObstructionType::NONE && solid).then_some(
                Obstruction {
                    kind: made.obstruction_type,
                    class: made.obstruction_class,
                },
            ),
        };
        self.spatial.insert(id, placed);
        self.pathing.moved(id, &placed, false);
        self.pathing.tiles_changed(self.spatial.take_changes());
        if let Some(v) = self.spatial.sight.as_mut() {
            v.add_seer(
                id,
                Seer {
                    player: p,
                    master,
                    los: made.line_of_sight,
                    state: ObjectState::Alive,
                    explored: false,
                    building: None,
                    umv: 0,
                    capture: capture_kind(p, made.class, master),
                    kind: made.kind,
                },
            );
            let _ = v.sight_on(id, position, made.radius, false, false)?;
        }
        if let Some(e) = self.take_sight_unsupported() {
            return Err(e);
        }
        let owner = self.players.player_mut(p)?;
        for &(kind, amount, flag) in &made.storage {
            if let Some(kind) = kind
                && matches!(flag, 1 | 2 | 32)
            {
                owner.add_attribute(kind, amount)?;
            }
        }
        let cost: i32 = made
            .cost
            .iter()
            .filter_map(|slot| slot.entry())
            .filter(|&(_, _, paid)| paid)
            .map(|(_, amount, _)| i32::from(amount.get()))
            .sum();
        owner.add_attribute(attribute::TOTAL_VALUE, Amount::new(cost as f32))?;
        owner.add_attribute(attribute::UNITS_VALUE, Amount::new(cost as f32))?;
        owner.add_attribute(AttributeId::numbered(246), Amount::new(cost as f32))?;
        if made.class == class::VILLAGER {
            owner.add_attribute(AttributeId::numbered(248), Amount::new(1.0))?;
        }
        // the combat setup's class counters (FUN_141226060)
        crate::objects::lifecycle::death_parts::count_class(&mut owner.attributes, made, 1.0);
        let combat = crate::objects::layers::Combat {
            // the TRIBE combat setup (FUN_14121df60): the master's volley fire
            firepower: made.volley_fire,
            ..Default::default()
        };
        let unit = Unit {
            id,
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Alive,
                hp: HitPoints::of_master(made.hit_points),
                attribute_type: None,
                amount: Amount::ZERO,
                dying_sprite: made.death_sprite,
                death_spawn: made.death_spawns[0].is_some(),
                death_facets: None,
                inside: None,
                goto_sleep: false,
                master_id: master,
                master: made.life,
            },
            place: crate::objects::layers::Static::at(position),
            attackable: true,
            pasture: None,
            my_pasture: None,
            sprite: Some(sprite),
            ..Unit::default()
        };
        let parts = ObjectParts {
            actions: crate::objects::layers::Action {
                // livestock's default task, the Artifact, starts once its action list is empty
                default_task: if made
                    .default_task()
                    .is_some_and(|t| t.action_type == crate::ownership::artifact::ARTIFACT_TYPE)
                {
                    crate::objects::layers::DefaultTask::Artifact
                } else {
                    crate::objects::layers::DefaultTask::Nothing
                },
                ..Default::default()
            },
            unit_ai: ai,
            combat,
            // FUN_1411580a0: facing x (an immediate 1.0), its right -z, not waiting; the animated
            // object's constructor (FUN_141070cf0): animation speed (+0x7b8) 0
            motion: Some(crate::objects::layers::Motion::Moving(Mover::new_object(
                moving,
                Some(&standing),
                position,
                self.world_time,
                angle,
                Some(facets),
            ))),
            ..ObjectParts::default()
        };
        let idx = self.push_live(p, Object { unit, parts })?;
        // the static init's add to its player (slot 0x2d0) files it in the tracker
        self.track_add(p, id, master)?;
        Ok(idx)
    }
}
