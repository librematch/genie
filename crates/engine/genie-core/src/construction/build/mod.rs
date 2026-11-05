//! A builder's Build action (`TRIBE_Action_Build`, type 101, vtable `0x142ed6628`) up to the
//! builder's walk: the action starts in state 4 with a `MoveTo` sub-action (`RGE_Action_Move_To`,
//! type 1) toward the foundation; a builder already at the foundation works in its first update
//! (state 6). A walking builder's `MoveTo` runs the motion step ([`crate::movement`]).
//!
//! A builder at a foundation faces it and joins its work (state 7): at a foundation no one started
//! it first checks the footprint, and a builder standing on it walks to a free tile beside it
//! (state 223) and comes back (state 4). The work adds to the foundation's construction each
//! update ([`crate::world::World::work_building`], the builder lent to the world); the first work
//! starts it: the foundation gets its obstruction record. The work that reaches the build time
//! completes the building ([`crate::world::World::complete_building`]) and ends the builder's work
//! (state 224); a builder whose unit AI holds queued orders then lets
//! the Build end (state 1) and the AI runs its queue.

use self::update::drop_done_orders;
use crate::map::Distance;
use crate::map::Position;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::WorkRate;
use crate::objects::lifecycle::death_parts::FarmBonus;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::TaskId;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use std::collections::BTreeMap;

use crate::map::spatial::Obstruction;
use crate::objects::masters::{Master, Task};
use crate::unit_ai::OrderKind;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

mod build_state;
mod clearing;
mod completion;
mod construction;
mod drop_off;
pub(crate) mod more_work;
mod placing;
mod states;
pub(crate) mod unit;
mod unit_ai_side;
mod update;
mod walk_ends;
mod walks;
pub(crate) mod world_side;

pub use self::build_state::BuildState;
pub use self::clearing::{ClearLook, Cleared, Clearer, ClearingSite};
pub(crate) use self::clearing::{clearing_objects, footprint_clearing};

pub use self::drop_off::DropSearch;
#[cfg(test)]
use self::placing::beside_diagonal;
pub(crate) use crate::construction::build::world_side::{NO_CONSTRUCTION, NOT_HELD, held};
use crate::construction::{BuilderCount, FoundationId};
use crate::gathering::DropSiteId;
use crate::gathering::hunt::DropOff;
use crate::map::Snow;
#[cfg(test)]
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::movement::MoveStats;
use crate::objects::masters::{Masters, WorkValue};
use crate::objects::release::Releaser;
use crate::objects::sprite::{ActiveSprite, new_sprite};
use crate::objects::{Frame, Graphics, ObjectState};
#[cfg(test)]
use crate::pathing::Pathing;
#[cfg(test)]
use crate::players::PlayerId;
use crate::world::NO_ATTRIBUTE;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpriteTuning;

/// A foundation of another player than the builder's, which the slice does not hand the Build.
const FOREIGN_SITE: GenieError = GenieError::unsupported("a foundation of another player");
/// The Build's foundation the world no longer holds where its place is read.
const SITE_GONE: GenieError = GenieError::unsupported("a foundation that is gone");
/// A builder without movement state.
pub(crate) const BUILDER_UNMOVING: GenieError =
    GenieError::unsupported("a builder without movement state");
/// A foundation its owner sees only through a ghost of it on its tile (`FUN_1411d3280`'s last
/// test), which the Build's world does not hold.
const UNSEEN_SITE: GenieError =
    GenieError::unsupported("a foundation its owner sees only through its ghosts (slot 0x1e0)");

/// The Build action of a builder (`TRIBE_Action_Build`, type 101).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct BuildAction {
    /// The foundation's id; none once it is gone.
    pub(crate) target: Option<FoundationId>,
    /// The action state (`+0x18`), which word 9 adds, with its sub-action list's `MoveTo`
    /// (`+0x50`): to the foundation (`set_state(4)`), to the drop site (8), to its point (11) or
    /// off the footprint (223).
    pub(crate) state: BuildState,
    /// `+0x38`..`+0x40`, the action's own point: the foundation's place from the constructor
    /// (`FUN_140aad4e0`) and each `set_state(4)` with a target, −1 each from `set_state(2)`;
    /// `set_state(11)` walks there.
    pub(crate) point: Position,
    /// The task it builds with.
    pub(crate) task: BuildTask,
    /// `+0x68`, `added_to_num_builders`: the builder counts among the foundation's builders.
    pub(crate) added: bool,
    /// `+0x44`, the action's timer: state 222's wait (a second from its `set_state`).
    pub(crate) timer: Seconds,
    /// `+0x28` (its id at `+0x34`), the second target: the drop site of its drop-off (state 8,
    /// `set_state(8)`'s search).
    pub(crate) site: Option<DropSiteId>,
}

/// What a builder's Build task gives its work: the task's work range (`+0x30`; its `MoveTo` to the
/// foundation stops this near) and work value (`+0x28`, `work_val1`), and the builder's work rate:
/// its master's `work_rate` (`+0x798`) plus its own `mWorkRatePowerup` (`+0xa30`, slot 0x6e8).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct BuildTask {
    /// The task's work range.
    pub(crate) range: Distance,
    /// The task's work value.
    pub(crate) work_value: WorkValue,
    /// The builder's work rate: its master's `work_rate` (`+0x798`) plus its `mWorkRatePowerup`
    /// (`+0xa30`).
    pub(crate) work_rate: WorkRate,
    /// The task's resource (`attribute_type`, `+0x20`, none for any): `UNIT_ORDER`'s drop-off
    /// (slot 0x78) asks a builder carrying another one to its unit AI instead.
    pub(crate) resource: Option<AttributeId>,
    /// The task's id in its master's list (`+0xa`), which a master's upgrade finds the new
    /// master's task by (−1 for none).
    pub(crate) id: TaskId,
    /// The task's moving sprite (`+0x40`), which it gives its `MoveTo`s; None for none.
    pub(crate) moving: Option<SpriteId>,
}

impl BuildTask {
    /// The Build task `task` of a builder whose work rate is `work_rate`.
    #[must_use]
    pub const fn of(task: &Task, work_rate: WorkRate) -> Self {
        Self {
            range: task.work_range,
            work_value: task.work_val1,
            work_rate,
            resource: task.attribute_kind,
            id: task.id,
            moving: task.sprites.moving,
        }
    }
}

/// A building under construction (`TRIBE_Building_Object`), with what its master says of it.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Construction {
    /// `+0xc50`, `build_pts`: the seconds of work it has had.
    pub(crate) progress: Seconds,
    /// `+0xc60`, `desolid_flag`: no builder has started it; it has no obstruction record.
    pub(crate) desolid: bool,
    /// `+0x9e8`, `num_builders`.
    pub(crate) builders: BuilderCount,
    /// Its master's hit points (`+0x530`) and build time (`+0x898`, the first train location's
    /// train time; 0 without one).
    pub(crate) hit_points: MasterHitPoints,
    /// Its master's build time (the first train location's train time; 0 without one).
    pub(crate) build_time: WholeSeconds,
    /// The record its start adds: its master's obstruction kind and class.
    pub(crate) record: Obstruction,
    /// `+0xc61`, `snow_flag` ([`crate::map::Snow`]): it stands on snow, completed there or kept
    /// as a farm's or a pasture's (its decal is not held).
    pub(crate) snow: Snow,
    /// `+0x788`, `sprite_override_id`, is not −1.
    pub(crate) sprite_override: bool,
}

/// What the completion's first part ([`Completion::first_part`]) reads besides the building's
/// construction.
#[derive(Clone, Copy, Debug)]
pub struct Completion<'masters> {
    /// The building's master.
    pub(crate) master: Option<&'masters Master>,
    /// The owner's farm bonus ([`FarmBonus`], attributes 237 to 239).
    pub(crate) farm_bonus: FarmBonus,
    /// The owner's masters, whose table `FUN_141659d20` reads.
    pub(crate) masters: Option<&'masters Masters>,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub(crate) player_kind: PlayerKind,
    /// The building has a unit AI (`+0x6b0`): a mule cart's.
    pub(crate) ai: bool,
}

/// What the completion's first part (`FUN_1411c6bd0`) gives its caller: the attributes its owner
/// gains (a farm's bonus) and whether a mule cart's search for resources follows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FirstPart {
    /// The attributes its owner gains, in order.
    pub(crate) attributes: Vec<(AttributeId, Amount)>,
    /// The building's master has a task of action type 0x99 and the building a unit AI: its
    /// search for resources follows ([`crate::world::World::cart_search`]).
    pub(crate) cart: bool,
    /// A pasture (1893, 1897) whose build points reached its build time: its livestock follow
    /// after the attributes (`FUN_1411c8410`, [`crate::world::World::setup_pasture`]).
    pub(crate) pasture: bool,
}

impl Completion<'_> {
    /// The completion's first part (`FUN_1411c6bd0`), which the building's `set_state` runs from
    /// state 0 to any state of 2 or more: a farm (class 49) whose build points reached its build
    /// time (COMISS, JC: a NaN does not) gets its owner's bonus when attributes 238 (a resource)
    /// and 239 (a master), cut to integers (CVTTSS2SI), are both 0 or more (`FUN_1411c66a0`): a
    /// master 239 the owner has (`FUN_141659d20`) gives it only near the farm
    /// (`FUN_1411d3cd0`, not modelled); a resource up to the owner's attribute count gets
    /// attribute 237's amount (slot 0xf8 with 1 and 1); a pasture (1893, 1897) then fills
    /// ([`FirstPart::pasture`], the caller's); a master with a task of action type 0x99 and a unit AI
    /// runs its cart's search (the tail's game mode 4 test, `FUN_140c87f20`, holds in a game).
    /// Not modelled: a computer player's (`FUN_1411e7ca0`) and an overridden sprite's master
    /// copy (`+0x788`). Not held: the owner's `+0x137c` (cleared for master 109), the object
    /// tracker (`FUN_14104eac0`, `FUN_14104faf0`) and the farm's `+0xcac`.
    ///
    /// # Errors
    ///
    /// What it does not model.
    pub fn first_part(&self, construction: &Construction, roles: &Roles) -> GenieResult<FirstPart> {
        use crate::math::conversions::cvttss2si;
        let master = self.master.ok_or(GenieError::unsupported(
            "a completion of a master the slice does not have",
        ))?;
        if self.player_kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's building part (FUN_1411e7ca0)",
            ));
        }
        if construction.sprite_override {
            return Err(GenieError::unsupported(
                "a completion of an overridden sprite",
            ));
        }
        let mut part = FirstPart::default();
        if roles.of_class(master.class).contains(ClassRoles::FARM)
            && Seconds::of_master(construction.build_time) <= construction.progress
        {
            let FarmBonus {
                amount,
                resource,
                near,
            } = self.farm_bonus;
            let (resource, near) = (cvttss2si(resource.get()), cvttss2si(near.get()));
            if 0 <= resource && 0 <= near {
                let has = i16::try_from(near)
                    .ok()
                    .and_then(MasterId::of_data)
                    .and_then(|master_id| self.masters?.get(master_id))
                    .is_some();
                if has {
                    return Err(GenieError::unsupported(
                        "a farm's bonus near a master (FUN_1411d3cd0)",
                    ));
                }
                let attribute = i16::try_from(resource)
                    .ok()
                    .and_then(AttributeId::of_data)
                    .ok_or(NO_ATTRIBUTE)?;
                part.attributes.push((attribute, amount));
            }
            // the game mode is not the editor's (4) in a game
            part.pasture = roles
                .of_master(master.id)
                .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE);
        }
        part.cart = self.ai
            && master
                .action_part()
                .tasks
                .iter()
                .any(|task| task.action_kind == CART_TASK);
        Ok(part)
    }
}

/// The action type of the mule cart's tasks (its moves to resources, `0x99`): the action
/// factory makes no action of it.
pub const CART_TASK: ActionKind = ActionKind::named(0x99);

/// A completed building's stack unit (`FUN_1411dbb70`'s block for `+0x986`): the master its
/// owner's slot 0x1c0 makes (`FUN_14162ce00` with its flag set), the place, and the hit points
/// the building lacked (its master's less its own, which the new object loses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackUnit {
    /// The master (`+0x986`; master 1889 on mangrove or rice terrain, `+0xcd0`, makes 1893).
    pub(crate) master: MasterId,
    /// The building's place.
    pub(crate) at: Position,
    /// Its master's hit points (`+0x530`, a short) less its own.
    pub(crate) hp_lost: HitPoints,
}

/// The completion's sprite (`FUN_1411dbb70` from state 0 to 2, after its storage): slot 0x68
/// (`FUN_141217000`) with the master's standing sprite (`+0x470`), then its sprite list's slot
/// 0x28 (`FUN_1413ba880`) sets that sprite's node to the frame its BUILD command chose
/// (`+0xc54`, [`crate::objects::layers::Building::build_frame`]; slot 9 with it, [`ActiveSprite::set_frame`]).
pub(crate) fn show_built(
    sprite: &mut Option<ActiveSprite>,
    stats: Option<&mut MoveStats>,
    master: &Master,
    frame: Frame,
    graphics: &Graphics,
    sprite_rng: &mut Well512,
    sprite_tuning: SpriteTuning,
) {
    let standing = master
        .standing_sprite
        .and_then(|sprite_id| graphics.get(sprite_id).copied());
    if let Some(graphic) = standing {
        new_sprite(sprite, stats, graphic, sprite_rng, sprite_tuning);
        if let Some(active_sprite) = sprite
            .as_mut()
            .filter(|active_sprite| active_sprite.graphic.id == graphic.id)
        {
            active_sprite.set_frame(frame);
        }
    }
}

#[cfg(test)]
mod built_sprite_tests {
    use super::*;
    use crate::objects::SpeedMultiplier;
    use crate::objects::SpriteFlags;
    use crate::testing::tuning;

    #[cfg(test)]
    use crate::objects::FacetCount;
    use crate::objects::sprite::{ActiveSprite, Graphic};

    /// The completion's sprite (`FUN_1411dbb70`): the foundation's construction sprite gives way
    /// to the master's standing one (slot 0x68), whose node takes the BUILD command's frame
    /// (slot 0x28, slot 9 with it); a static standing sprite takes the frame alone.
    #[test]
    fn completed_building_shows_its_standing_sprite_at_its_frame() {
        let graphic_of = |id, flags| Graphic {
            id,
            frames: Frame::of_data(4),
            facets: FacetCount::of_data(1),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags,
        };
        let graphics = Graphics::from([
            (
                SpriteId::named(10),
                graphic_of(SpriteId::named(10), SpriteFlags::ANIMATED),
            ),
            (
                SpriteId::named(11),
                graphic_of(SpriteId::named(11), SpriteFlags::ANIMATED),
            ),
        ]);
        let master = Master {
            standing_sprite: Some(SpriteId::named(11)),
            ..Master::default()
        };
        let mut rng = Well512::default();
        let mut sprite = Some(ActiveSprite::start(
            graphic_of(SpriteId::named(10), SpriteFlags::ANIMATED),
            &mut rng,
            tuning().sprite,
        ));
        show_built(
            &mut sprite,
            None,
            &master,
            Frame::of_data(2),
            &graphics,
            &mut rng,
            tuning().sprite,
        );
        let active = sprite.unwrap();
        assert_eq!(
            (active.graphic.id, active.frame, active.last_frame),
            (SpriteId::named(11), Frame::of_data(2), Frame::of_data(2))
        );
        let statics = Graphics::from([(
            SpriteId::named(11),
            graphic_of(SpriteId::named(11), SpriteFlags::NONE),
        )]);
        let mut none = None;
        show_built(
            &mut none,
            None,
            &master,
            Frame::of_data(3),
            &statics,
            &mut rng,
            tuning().sprite,
        );
        assert_eq!(
            none.map(|active_sprite| (active_sprite.graphic.id, active_sprite.frame)),
            Some((SpriteId::named(11), Frame::of_data(3)))
        );
    }
}

/// What a builder's Build reads as its update starts (`Unit::build_part`'s reads), besides
/// the action and the world it reaches through the step.
#[derive(Debug)]
pub(crate) struct BuildReads {
    /// The foundation (the action's target) when the builder's player holds it with its
    /// construction: the update reads and works it through the world.
    pub(crate) site: Option<FoundationId>,
    /// The foundation's `+0xcb0`: the object its completion's stack unit made
    /// ([`crate::objects::layers::Building::stack_made`]).
    pub(crate) stack_made: Option<ObjectId>,
    /// The state of each foundation its unit AI's queued build orders name (None for one that
    /// is gone).
    pub(crate) build_targets: Vec<(Option<ObjectId>, Option<ObjectState>)>,
    /// In state 8, what the drop-off reads of its second target ([`BuildAction::site`]): its
    /// state and [`crate::gathering::hunt::DropOff`]; None when the world does not hold it.
    pub(crate) deposit: Option<(ObjectState, DropOff)>,
    /// In state 8, the targets of its unit AI's queued orders 0x2c5, 0x2c9 and 0x2db the world
    /// no longer holds or whose seen state is above 5 (`FUN_140c5de60`).
    pub(crate) gone_orders: Vec<Option<ObjectId>>,
    /// What the footprint clearing of its `set_state(7)` reads of the objects on the tiles
    /// ([`ClearLook`]) by id; none for one the world does not hold.
    pub(crate) looks: BTreeMap<ObjectId, ClearLook>,
    /// The builder's class (`+0x0a`, decoded).
    pub(crate) class: Option<ClassId>,
    /// Its foundation's master is a drop site for its civ (`FUN_140fde180`) and it carries a
    /// kind its player has: the end of the work gives its load to its player.
    pub(crate) drop_site: bool,
    /// The builder as the releaser of its targets (slot 0x180's argument).
    pub(crate) by: Releaser,
}

/// The progress a started foundation holds before its first work (`0x2edbe6ff`, about 1e-10).
const STARTED: u32 = 0x2edb_e6ff;

/// The drop-off's wait after a drop site out of reach (`0xc0400000`).
const DROP_WAIT: Seconds = Seconds::new(-3.0);
/// The orders whose targets gone the unit AI drops after a drop-off (`FUN_140c5de60`: the bits
/// of `0x400011` over `order - 0x2c5`).
pub(crate) const DROPPED_ORDERS: [OrderKind; 3] =
    [OrderKind::Gather, OrderKind::Hunt, OrderKind::OnTarget];

#[cfg(test)]
mod stack_tests {
    use super::*;
    use crate::construction::BuildProgress;
    #[cfg(test)]
    use crate::construction::MainBuildingId;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::GroundOffset;
    use crate::map::Radii;
    use crate::map::Restriction;
    use crate::map::TerrainFlags;
    use crate::map::spatial::{Placed, Tiles};
    use crate::map::terrain::TileTerrain;
    #[cfg(test)]
    use crate::map::terrain::{Terrain, TerrainKind};
    #[cfg(test)]
    use crate::map::{RestrictionId, TerrainMask};
    use crate::movement::MoveAlgorithm;
    use crate::movement::Speed;
    use crate::movement::TurnRate;
    use crate::objects::ListIndex;
    use crate::objects::ObjectState;
    #[cfg(test)]
    use crate::objects::layers::{Building, Static};
    use crate::objects::lifecycle::{Class, Lifecycle};
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::BuiltMode;
    use crate::objects::masters::CreatablePart;
    use crate::objects::masters::FlyMode;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionKind;
    use crate::objects::masters::{BuildingMaster, Masters};
    use crate::objects::unit::Unit;
    use crate::objects::unit::UnitFlags;
    use crate::players::attribute::AttributeId;
    #[cfg(test)]
    use crate::players::history::{EntryKind, Event, EventKind, Population, TimeSlice};
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::test_ids::opt_master;
    use crate::testing::attribute;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::unit_ai::UnitAiKind;
    use crate::world::Millis;
    use crate::world::WorldTime;
    #[cfg(test)]
    use crate::world::{FrameSeed, Gate};
    use crate::world::{Player, World};
    use std::sync::Arc;

    /// A building master of `id`: class 3, radius `radius`, height `z`, obstruction `kind`, train
    /// time `bt`, hit points `hp`, an annex of `head`, with `annexes` at their offsets.
    fn building(
        id: i16,
        (radius, z, kind): (f32, f32, ObstructionKind),
        (bt, hp): (i16, i16),
        head: i16,
        annexes: &[(i16, GroundOffset)],
    ) -> Master {
        let mut slots = [None; 4];
        let mut offsets = [GroundOffset::ZERO; 4];
        for (slot, &(index, ground_offset)) in annexes.iter().enumerate() {
            slots[slot] = opt_master(index);
            offsets[slot] = ground_offset;
        }
        Master {
            id: master_id(id),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(radius, radius),
            height: Distance::new(z),
            obstruction_kind: kind,
            obstruction_class: ObstructionClass::UNIT,
            hit_points: MasterHitPoints::of_data(hp),
            building: Some(BuildingMaster {
                stack_unit: None,
                foundation_terrain: None,
                tech: None,
                annexes: slots,
                annex_offsets: offsets,
                head_unit: opt_master(head),
                transform_unit: None,
                ..BuildingMaster::default()
            }),
            creatable: Some(CreatablePart {
                build_time: WholeSeconds::of_data(bt),
                trained_at: vec![master_id(9999)],
                ..CreatablePart::default()
            }),
            ..Master::default()
        }
    }

    /// The town centre's masters as build 158041's `.dat` holds them for a civ without 2160: the
    /// foundation 621 (stack unit 109, disappears when built), the town centre 109 (annexes 618,
    /// 619, 620 and 890; its storage of population, 5 of mode 4, and of attribute 506, 1 of mode
    /// 16; a volley missile: the tower's unit AI), 618 (tall), 619 (its own annexes 1649 and
    /// 2160), 620, 1649 and 890 (class 27, hit points −1, tall).
    fn masters() -> Masters {
        let mut foundation = building(
            621,
            (2.0, 4.0, ObstructionKind::KIND_2),
            (150, 2400),
            -1,
            &[],
        );
        if let Some(building_master) = foundation.building.as_mut() {
            building_master.stack_unit = Some(ids::TOWN_CENTRE);
            building_master.disappears_when_built = BuiltMode::DISAPPEARS;
        }
        let mut tc = building(
            109,
            (2.0, 0.0, ObstructionKind::NONE),
            (100, 2400),
            621,
            &[
                (618, GroundOffset::new(1.0, -1.0)),
                (619, GroundOffset::new(-0.5, 0.5)),
                (620, GroundOffset::new(-1.0, 1.0)),
                (890, GroundOffset::new(-1.0, 1.0)),
            ],
        );
        tc.storage = vec![
            (Some(attribute::HEADROOM), Amount::new(5.0), 4),
            (Some(AttributeId::named(506)), Amount::new(1.0), 16),
            (None, Amount::ZERO, 0),
        ];
        if let Some(creatable_part) = tc.creatable.as_mut() {
            creatable_part.volley_missile = Some(master_id(54));
        }
        let mut empty = building(
            890,
            (0.0, 4.0, ObstructionKind::CIRCLE),
            (-10, -1),
            109,
            &[],
        );
        empty.class = class::WALL;
        Masters::new([
            foundation,
            tc,
            building(
                618,
                (1.0, 3.0, ObstructionKind::KIND_2),
                (100, 2400),
                621,
                &[],
            ),
            building(
                619,
                (1.5, 0.0, ObstructionKind::NONE),
                (100, 2400),
                621,
                &[
                    (1649, GroundOffset::new(0.75, -0.75)),
                    (2160, GroundOffset::ZERO),
                ],
            ),
            building(
                620,
                (1.0, 0.0, ObstructionKind::NONE),
                (100, 2400),
                621,
                &[],
            ),
            building(1649, (0.0, 0.0, ObstructionKind::NONE), (0, 1), 619, &[]),
            empty,
        ])
    }

    /// Player 1 with the town-centre foundation 8759 at (54, 147), built to its last 0.01
    /// seconds, its record on the map; the next id 8768.
    fn world() -> World {
        let ms = masters();
        let foundation = Unit {
            id: object_id(8759),
            place: Static::at(Position::new(54.0, 147.0, 0.0)),
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Foundation,
                hp: HitPoints::new(2399.0),
                master_id: ids::TOWN_CENTRE_621,
                ..Lifecycle::default()
            },
            pasture: None,
            my_pasture: None,
            ..Unit::default()
        };
        let layer = Building {
            construction: Some(Construction {
                progress: Seconds::new(149.99),
                desolid: false,
                builders: BuilderCount::of_data(1),
                hit_points: MasterHitPoints::of_data(2400),
                build_time: WholeSeconds::of_data(150),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::UNIT,
                },
                snow: Snow::None,
                sprite_override: false,
            }),
            ..Default::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 2],
            vec![
                Player::default(),
                Player {
                    objects: vec![foundation].into(),
                    attributes: vec![Amount::ZERO; 600],
                    ..Player::default()
                },
            ],
        )
        .with_buildings([(object_id(8759), layer)]);
        world.spatial = Spatial::with_tiles(
            GridSize::new(200, 200),
            Tiles::new(GridSize::new(200, 200), vec![0; 200 * 200]),
        );
        world.spatial.terrain = Some(Terrain::new(
            GridSize::new(200, 200),
            vec![TileTerrain::of_save(0, 255, -1); 200 * 200],
            vec![
                TerrainKind {
                    shown: true,
                    flags: TerrainFlags::NONE,
                    forest: false,
                };
                40
            ],
            vec![Restriction::new(vec![0.0; 40]); 5],
        ));
        world.spatial.insert(
            object_id(8759),
            Placed {
                position: Position::new(54.0, 147.0, 0.0),
                radius: Footprint::new(2.0, 2.0),
                class: class::BUILDING,
                player: PlayerId::new(1),
                master: ids::TOWN_CENTRE_621,
                obstruction_kind: ObstructionKind::KIND_2,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: Some(Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::UNIT,
                }),
            },
        );
        world.next_object_id = object_id(8768);
        world.model.masters = vec![Arc::new(ms.clone()), Arc::new(ms)];
        world
    }

    /// A castle's completion records its event in its owner's history between its storage and
    /// its counts, the owner's id as `data1` at the world's time (`FUN_1411dbb70`:
    /// `FUN_141560ae0` on `+0xdb8` with `(float)+0x718`, 0 and 0), then counts it.
    #[test]
    fn castle_completion_records_its_owner_id_in_its_history() {
        let mut world = world();
        world.world_time = WorldTime::from_millis(912_000);
        world.players[1].history.add_entry(
            EntryKind::POPULATION,
            Population::of_data(3),
            Population::ZERO,
        );
        for ms in &mut world.model.masters {
            let castle = building(
                82,
                (2.0, 4.0, ObstructionKind::KIND_2),
                (150, 2400),
                -1,
                &[],
            );
            *ms = Arc::new(Masters::new(ms.iter().cloned().chain([castle])));
        }
        world.players[1].objects[ListIndex::new(0)].life.master_id = ids::CASTLE;
        assert_eq!(
            world.work_building(FoundationId::new(object_id(8759)), Seconds::new(1.0)),
            Ok(BuildProgress::Ended)
        );
        assert_eq!(
            world.players[1].history.events,
            [Event {
                kind: EventKind::CastleBuilt,
                time_slice: TimeSlice::of_data(1),
                world_time: WorldTime::from_millis(912_000),
                data1: 1.0,
                data2: 0.0,
                data3: 0.0,
            }]
        );
        assert_eq!(world.players[1].attributes[134], Amount::new(1.0));
    }

    /// census v5's 273 stops "a completion of a stack unit or a vanishing building" (records/
    /// 428759682 update 2019): the work that completes the town-centre foundation 621 drops its
    /// record, its owner makes the stack unit 109 at its place with its flag (`FUN_14162ce00`):
    /// the town centre built already (its train time as build points, its hit points), its
    /// annexes depth first (618, 619 with its 1649, 620, 890; 2160 the player lacks), each at
    /// its offset and linked, the tall ones desolid; worked 10000 seconds at once it completes,
    /// its annexes first: its population (attribute 4) and its own store (506), less the hit
    /// points the foundation lacked; then 621 disappears (state 7, hit points 0) without paying
    /// attributes 98 and 165.
    #[test]
    fn town_centre_foundation_completes_into_the_town_centre_and_its_annexes() {
        let mut world = world();
        assert_eq!(
            world.work_building(FoundationId::new(object_id(8759)), Seconds::new(1.0)),
            Ok(BuildProgress::Ended)
        );
        let ids: Vec<(i32, i16, ObjectState)> = world.players[1]
            .objects
            .iter()
            .map(|unit| (unit.id.get(), unit.life.master_id.get(), unit.life.state))
            .collect();
        assert_eq!(
            ids,
            [
                (8759, 621, ObjectState::Removed),
                (8768, 109, ObjectState::Alive),
                (8769, 618, ObjectState::Alive),
                (8770, 619, ObjectState::Alive),
                (8771, 1649, ObjectState::Alive),
                (8772, 620, ObjectState::Alive),
                (8773, 890, ObjectState::Alive),
            ]
        );
        let at = |id: ObjectId| world.spatial.get(id).unwrap().position;
        assert_eq!(
            [
                at(object_id(8769)),
                at(object_id(8770)),
                at(object_id(8771)),
                at(object_id(8772))
            ],
            [
                Position::new(55.0, 146.0, 0.0),
                Position::new(53.5, 147.5, 0.0),
                Position::new(54.25, 146.75, 0.0),
                Position::new(53.0, 148.0, 0.0)
            ]
        );
        assert_eq!(
            [8769, 8770, 8771, 8772, 8773].map(|id| world
                .parts
                .building_of(object_id(id))
                .annex_of
                .map(MainBuildingId::id)),
            [
                Some(object_id(8768)),
                Some(object_id(8768)),
                Some(object_id(8770)),
                Some(object_id(8768)),
                Some(object_id(8768))
            ]
        );
        // the tower's unit AI for the town centre, the building's for the annexes
        assert_eq!(
            world
                .parts
                .unit_ai
                .get(object_id(8768))
                .map(|ai| ai.ai_kind),
            Some(UnitAiKind::Tower)
        );
        assert_eq!(
            world
                .parts
                .unit_ai
                .get(object_id(8769))
                .map(|ai| ai.ai_kind),
            Some(UnitAiKind::Building)
        );
        // the foundation lacked 0.84 of its 2400 hit points after its last work: the town
        // centre lacks them too
        let lacked = 2400.0 - (2399.0 + (0.01_f32 * 2400.0) / 150.0);
        let unit = |id: ObjectId| world.players[1].objects.by_id(id).unwrap();
        assert_eq!(
            (
                unit(object_id(8768)).life.hp,
                world.parts.combat_of(object_id(8768)).store.clone()
            ),
            (
                HitPoints::new(2400.0 - lacked),
                vec![(AttributeId::named(506), Amount::new(1.0))]
            )
        );
        let attributes = &world.players[1].attributes;
        assert_eq!(
            (attributes[4], attributes[98], attributes[165]),
            (Amount::new(5.0), Amount::ZERO, Amount::ZERO)
        );
        let gone = unit(object_id(8759));
        assert_eq!(
            (gone.life.hp, gone.flags.contains(UnitFlags::ROO_CALLED)),
            (HitPoints::ZERO, true)
        );
        assert_eq!(
            world.spatial.get(object_id(8759)).unwrap().obstruction,
            None
        );
    }

    /// A new building's record (`FUN_14118c540` from its place): a master without a height (a
    /// pasture's post, master 1888: collision height 0) gets none, though its obstruction kind
    /// and class would give one; with a height it does (431153923's builder, update 412).
    #[test]
    fn new_building_without_height_gets_no_record() {
        for (height, record) in [
            (0.0, None),
            (1.0, Some((ObstructionKind::CIRCLE, ObstructionClass::UNIT))),
        ] {
            let mut world = world();
            for ms in &mut world.model.masters {
                let master = Arc::make_mut(ms).get_mut(ids::TOWN_CENTRE).unwrap();
                if let Some(creatable_part) = master.creatable.as_mut() {
                    creatable_part.build_time = WholeSeconds::ZERO;
                }
                master.obstruction_kind = ObstructionKind::CIRCLE;
                master.obstruction_class = ObstructionClass::UNIT;
                master.height = Distance::new(height);
                master.building.as_mut().unwrap().annexes = [None; 4];
            }
            let (id, _) = world
                .make_building(
                    PlayerId::new(1),
                    ids::TOWN_CENTRE,
                    Ok(None),
                    Position::new(80.0, 80.0, 0.0),
                )
                .unwrap();
            let placed = world.spatial.get(id).unwrap().obstruction;
            assert_eq!(
                placed.map(|obstruction| (obstruction.kind, obstruction.class)),
                record,
                "height {height}"
            );
        }
    }

    /// A gate foundation's stack gate takes the foundation's lock (`FUN_1411dbb70`: the new
    /// gate's slot 0x670 with the old one's `+0xc7c`): locked, the new gate's last move time is
    /// the world time plus 1; unlocked it stays unlocked and keeps its last move time.
    #[test]
    fn gate_stack_gate_takes_the_foundation_lock() {
        for locked in [true, false] {
            let mut world = world();
            world.world_time = WorldTime::from_millis(5000);
            for ms in &mut world.model.masters {
                for id in [621, 109] {
                    let master = Arc::make_mut(ms).get_mut(master_id(id)).unwrap();
                    master.class = class::GATE;
                    master.storage.clear();
                    if let Some(creatable_part) = master.creatable.as_mut() {
                        creatable_part.volley_missile = None;
                    }
                    // every building holds its movement state
                    master.move_stats = Some(MoveStats {
                        fly_mode: FlyMode::GROUND,
                        speed: Speed::ZERO,
                        base: Speed::ZERO,
                        turn_rate: TurnRate::ZERO,
                        turn_radius: Distance::ZERO,
                        turn_radius_speed: Speed::MAX,
                        radius: Radii::new(2.0, 2.0, 0.0),
                        restriction: RestrictionId::named(0),
                        restriction_mask: TerrainMask::ALL,
                        turn_time: Seconds::ZERO,
                        move_algorithm: MoveAlgorithm::NONE,
                        min_collision: 0.0,
                        fast_turn: false,
                        facets: None,
                        weapon_range: Distance::ZERO,
                    });
                    if let Some(building_master) = master.building.as_mut() {
                        building_master.annexes = [None; 4];
                    }
                }
            }
            world
                .parts
                .building_mut(world.players[1].objects[ListIndex::new(0)].id)
                .unwrap()
                .gate = Some(Gate {
                locked,
                ..Gate::default()
            });
            world
                .stack_unit(
                    PlayerId::new(1),
                    object_id(8759),
                    StackUnit {
                        master: ids::TOWN_CENTRE,
                        at: Position::new(54.0, 147.0, 0.0),
                        hp_lost: HitPoints::ZERO,
                    },
                )
                .unwrap();
            let gate = world.players[1].objects.by_id(object_id(8768)).unwrap();
            assert_eq!(
                world
                    .parts
                    .building_of(gate.id)
                    .gate
                    .map(|gate| gate.locked),
                Some(locked)
            );
            let last_move = world.parts.mover_of(gate.id).map(|mover| mover.last_move);
            assert_eq!(
                last_move,
                Some(WorldTime::from_millis(if locked { 5001 } else { 5000 })),
                "{locked}"
            );
        }
    }

    /// The stack unit's completion tells each attacking unit of the foundation (`+0xce0`) the
    /// world still holds of the new object (slot 0x140, not modelled); one it no longer holds
    /// is passed over.
    #[test]
    fn stack_unit_completion_reads_the_foundation_attacking_units() {
        let mut gone = world();
        gone.parts
            .building_mut(gone.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .attacking_units = vec![object_id(99_999)];
        assert_eq!(
            gone.work_building(FoundationId::new(object_id(8759)), Seconds::new(1.0)),
            Ok(BuildProgress::Ended)
        );
        let mut held = world();
        let first = held.players[1].objects[ListIndex::new(0)].id;
        held.parts
            .building_mut(held.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .attacking_units = vec![first];
        assert_eq!(
            held.work_building(FoundationId::new(object_id(8759)), Seconds::new(1.0)),
            Err(GenieError::unsupported(
                "a stack unit's completion with other players' units on it (+0xce0)"
            ))
        );
    }
}

#[cfg(test)]
mod step_tests;
#[cfg(test)]
pub(crate) mod tests;
