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
use crate::objects::ObjectId;
use crate::objects::WorkRate;
use crate::objects::masters::ActionType;
use crate::objects::masters::ClassId;
use crate::objects::masters::TaskId;
use crate::objects::masters::class;
use crate::objects::masters::{MasterId, ids};
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use std::collections::BTreeMap;

use crate::GenieError;
use crate::map::spatial::Obstruction;
use crate::movement::move_to::MoveToAction;
use crate::objects::masters::{Master, Task};
use crate::unit_ai::OrderKind;
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
pub(crate) use self::clearing::{CLEARING_REACH, clearing_objects, footprint_clearing};
pub use self::clearing::{ClearLook, Cleared, ClearingSite};

pub use self::drop_off::DropSearch;
#[cfg(test)]
use self::placing::beside_diagonal;
pub(crate) use crate::construction::build::world_side::{NO_CONSTRUCTION, NOT_HELD, held};
#[cfg(test)]
use crate::map::spatial::Spatial;
#[cfg(test)]
use crate::pathing::Pathing;
#[cfg(test)]
use crate::players::PlayerId;

/// A builder within this of its foundation's footprint has reached it (`DAT_142eb5690`).
const REACH: f32 = crate::math::image::build::REACH;
/// A player's sight of an object it sees (slot 0x1e0).
const SEEN: u8 = 0xf;
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
    pub target: Option<ObjectId>,
    /// The action state (`+0x18`), which word 9 adds.
    pub state: BuildState,
    /// Its sub-action list's `MoveTo` (`+0x50`), None for an empty list: to the foundation
    /// (`set_state(4)`), to the drop site (8), to its point (11) or off the footprint (223).
    pub move_to: Option<MoveToAction>,
    /// `+0x38`..`+0x40`, the action's own point: the foundation's place from the constructor
    /// (`FUN_140aad4e0`) and each `set_state(4)` with a target, −1 each from `set_state(2)`;
    /// `set_state(11)` walks there.
    pub point: Position,
    /// The task it builds with.
    pub task: BuildTask,
    /// `+0x68`, `added_to_num_builders`: the builder counts among the foundation's builders.
    pub added: bool,
    /// `+0x44`, the action's timer: state 222's wait (a second from its `set_state`).
    pub timer: Seconds,
    /// `+0x28` (its id at `+0x34`), the second target: the drop site of its drop-off (state 8,
    /// `set_state(8)`'s search).
    pub site: Option<ObjectId>,
}

/// What a builder's Build task gives its work: the task's work range (`+0x30`; its `MoveTo` to the
/// foundation stops this near) and work value (`+0x28`, `work_val1`), and the builder's work rate:
/// its master's `work_rate` (`+0x798`) plus its own `mWorkRatePowerup` (`+0xa30`, slot 0x6e8).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct BuildTask {
    /// The task's work range.
    pub range: Distance,
    /// The task's work value.
    pub work_value: f32,
    /// The builder's work rate: its master's `work_rate` (`+0x798`) plus its `mWorkRatePowerup`
    /// (`+0xa30`).
    pub work_rate: WorkRate,
    /// The task's resource (`attribute_type`, `+0x20`, none for any): `UNIT_ORDER`'s drop-off
    /// (slot 0x78) asks a builder carrying another one to its unit AI instead.
    pub resource: Option<AttributeId>,
    /// The task's id in its master's list (`+0xa`), which a master's upgrade finds the new
    /// master's task by (−1 for none).
    pub id: TaskId,
    /// The task's moving sprite (`+0x40`), which it gives its `MoveTo`s; None for none.
    pub moving: Option<i16>,
}

impl BuildTask {
    /// The Build task `task` of a builder whose work rate is `work_rate`.
    #[must_use]
    pub const fn of(task: &Task, work_rate: WorkRate) -> Self {
        Self {
            range: task.work_range,
            work_value: task.work_val1,
            work_rate,
            resource: task.attribute_type,
            id: task.id,
            moving: crate::objects::sprite::sprite_id(task.sprites[0]),
        }
    }
}

/// A building under construction (`TRIBE_Building_Object`), with what its master says of it.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Construction {
    /// `+0xc50`, `build_pts`: the seconds of work it has had.
    pub progress: f32,
    /// `+0xc60`, `desolid_flag`: no builder has started it; it has no obstruction record.
    pub desolid: bool,
    /// `+0x9e8`, `num_builders`.
    pub builders: u8,
    /// Its master's hit points (`+0x530`) and build time (`+0x898`, the first train location's
    /// train time; 0 without one).
    pub hit_points: i16,
    /// Its master's build time (the first train location's train time; 0 without one).
    pub build_time: i16,
    /// The record its start adds: its master's obstruction kind and class.
    pub record: Obstruction,
    /// `+0xc61`, `snow_flag`: it was completed on snow (its decal is not held).
    #[diff(
        gap = "the teardowns' clearing (FUN_1411d5600, FUN_1411ad250) is not ported; the game's flag has three values"
    )]
    pub snow: bool,
    /// `+0x788`, `sprite_override_id`, is not −1.
    pub sprite_override: bool,
}

/// What the completion's first part ([`Completion::first_part`]) reads besides the building's
/// construction.
#[derive(Clone, Copy, Debug)]
pub struct Completion<'a> {
    /// The building's master.
    pub master: Option<&'a Master>,
    /// The owner's attributes 237, 238 and 239 (`player+0x768`..`+0x778`): a farm's bonus when
    /// built, its amount, its resource and a master near which it is given
    /// ([`Completion::first_part`]).
    pub farm_bonus: [f32; 3],
    /// The owner's masters, whose table `FUN_141659d20` reads.
    pub masters: Option<&'a crate::objects::masters::Masters>,
    /// The owner is a computer player (`FUN_141659cf0`).
    pub computer: bool,
    /// The building has a unit AI (`+0x6b0`): a mule cart's.
    pub ai: bool,
}

/// What the completion's first part (`FUN_1411c6bd0`) gives its caller: the attributes its owner
/// gains (a farm's bonus) and whether a mule cart's search for resources follows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FirstPart {
    /// The attributes its owner gains, in order.
    pub attributes: Vec<(AttributeId, Amount)>,
    /// The building's master has a task of action type 0x99 and the building a unit AI: its
    /// search for resources follows ([`crate::world::World::cart_search`]).
    pub cart: bool,
    /// A pasture (1893, 1897) whose build points reached its build time: its livestock follow
    /// after the attributes (`FUN_1411c8410`, [`crate::world::World::setup_pasture`]).
    pub pasture: bool,
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
    pub fn first_part(&self, c: &Construction) -> Result<FirstPart, GenieError> {
        use crate::math::conversions::cvttss2si;
        let master = self.master.ok_or(GenieError::unsupported(
            "a completion of a master the slice does not have",
        ))?;
        if self.computer {
            return Err(GenieError::unsupported(
                "a computer player's building part (FUN_1411e7ca0)",
            ));
        }
        if c.sprite_override {
            return Err(GenieError::unsupported(
                "a completion of an overridden sprite",
            ));
        }
        let mut part = FirstPart::default();
        if master.class == class::FARM && f32::from(c.build_time) <= c.progress {
            let [amount, resource, near] = self.farm_bonus;
            let (resource, near) = (cvttss2si(resource), cvttss2si(near));
            if 0 <= resource && 0 <= near {
                let has = i16::try_from(near)
                    .ok()
                    .and_then(MasterId::of_data)
                    .and_then(|m| self.masters?.get(m))
                    .is_some();
                if has {
                    return Err(GenieError::unsupported(
                        "a farm's bonus near a master (FUN_1411d3cd0)",
                    ));
                }
                let attribute = i16::try_from(resource)
                    .ok()
                    .and_then(AttributeId::of_data)
                    .ok_or(crate::world::NO_ATTRIBUTE)?;
                part.attributes.push((attribute, Amount::new(amount)));
            }
            // the game mode is not the editor's (4) in a game
            part.pasture = matches!(master.id, ids::PASTURE_MANGROVE | ids::PASTURE);
        }
        part.cart = self.ai && master.tasks.iter().any(|t| t.action_type == CART_TASK);
        Ok(part)
    }
}

/// The action type of the mule cart's tasks (its moves to resources, `0x99`): the action
/// factory makes no action of it.
pub const CART_TASK: ActionType = ActionType::named(0x99);

/// A completed building's stack unit (`FUN_1411dbb70`'s block for `+0x986`): the master its
/// owner's slot 0x1c0 makes (`FUN_14162ce00` with its flag set), the place, and the hit points
/// the building lacked (its master's less its own, which the new object loses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackUnit {
    /// The master (`+0x986`; master 1889 on mangrove or rice terrain, `+0xcd0`, makes 1893).
    pub master: MasterId,
    /// The building's place.
    pub at: Position,
    /// Its master's hit points (`+0x530`, a short) less its own.
    pub hp_lost: HitPoints,
}

/// The completion's sprite (`FUN_1411dbb70` from state 0 to 2, after its storage): slot 0x68
/// (`FUN_141217000`) with the master's standing sprite (`+0x470`), then its sprite list's slot
/// 0x28 (`FUN_1413ba880`) sets that sprite's node to the frame its BUILD command chose
/// (`+0xc54`, [`crate::objects::unit::Unit::build_frame`]; slot 9 with it, [`ActiveSprite::set_frame`]).
pub(crate) fn show_built(
    sprite: &mut Option<crate::objects::sprite::ActiveSprite>,
    stats: Option<&mut crate::movement::MoveStats>,
    master: &Master,
    frame: u8,
    graphics: &BTreeMap<i16, crate::objects::sprite::Graphic>,
    sprite_rng: &mut crate::math::rng::Well512,
) {
    let standing = (0 <= master.standing_sprite)
        .then(|| graphics.get(&master.standing_sprite).copied())
        .flatten();
    if let Some(g) = standing {
        crate::objects::sprite::new_sprite(sprite, stats, g, sprite_rng);
        if let Some(s) = sprite.as_mut().filter(|s| s.graphic.id == g.id) {
            s.set_frame(i16::from(frame));
        }
    }
}

#[cfg(test)]
mod built_sprite_tests {
    use super::*;

    use crate::objects::sprite::{ActiveSprite, Graphic};

    /// The completion's sprite (`FUN_1411dbb70`): the foundation's construction sprite gives way
    /// to the master's standing one (slot 0x68), whose node takes the BUILD command's frame
    /// (slot 0x28, slot 9 with it); a static standing sprite takes the frame alone.
    #[test]
    fn a_completed_building_shows_its_standing_sprite_at_its_frame() {
        let g = |id, flags| Graphic {
            id,
            frames: 4,
            facets: 1,
            speed_multiplier: 1.0,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags,
        };
        let graphics = BTreeMap::from([(10, g(10, 1)), (11, g(11, 1))]);
        let master = Master {
            standing_sprite: 11,
            ..Master::default()
        };
        let mut rng = crate::math::rng::Well512::default();
        let mut sprite = Some(ActiveSprite::start(g(10, 1), &mut rng));
        show_built(&mut sprite, None, &master, 2, &graphics, &mut rng);
        let s = sprite.unwrap();
        assert_eq!((s.graphic.id, s.frame, s.last_frame), (11, 2, 2));
        let statics = BTreeMap::from([(11, g(11, 0))]);
        let mut none = None;
        show_built(&mut none, None, &master, 3, &statics, &mut rng);
        assert_eq!(none.map(|s| (s.graphic.id, s.frame)), Some((11, 3)));
    }
}

/// What a builder's Build reads as its update starts (`Unit::build_part`'s reads), besides
/// the action and the world it reaches through the step.
#[derive(Debug)]
pub(crate) struct BuildReads {
    /// The foundation (the action's target) when the builder's player holds it with its
    /// construction: the update reads and works it through the world.
    pub(crate) site: Option<ObjectId>,
    /// The foundation's master, when the builder's player holds the foundation.
    pub(crate) built: Option<MasterId>,
    /// The foundation's `+0xcb0`: the object its completion's stack unit made
    /// ([`crate::objects::unit::Unit::stack_made`]).
    pub(crate) stack_made: Option<ObjectId>,
    /// The state of each foundation its unit AI's queued build orders name (None for one that
    /// is gone).
    pub(crate) build_targets: Vec<(Option<ObjectId>, Option<crate::objects::ObjectState>)>,
    /// In state 8, what the drop-off reads of its second target ([`BuildAction::site`]): its
    /// state and [`crate::gathering::hunt::DropOff`]; None when the world does not hold it.
    pub(crate) deposit: Option<(crate::objects::ObjectState, crate::gathering::hunt::DropOff)>,
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
    pub(crate) by: crate::objects::release::Releaser,
}

/// The fishing ships that work the fish traps they build (`FUN_1411945a0`: 13, 2356, 2171).
const FISHING_SHIPS: [MasterId; 3] = [
    MasterId::named(13),
    MasterId::named(2356),
    MasterId::named(2171),
];

/// A second builder works at a third of the rate of the first (`DAT_142ed66b0`).
const EXTRA_BUILDER: f32 = crate::math::image::build::EXTRA_BUILDER;
/// The progress a started foundation holds before its first work (`0x2edbe6ff`, about 1e-10).
const STARTED: u32 = 0x2edb_e6ff;
const WONDER_RATE: f32 = crate::math::image::build::WONDER_RATE;

/// The waits a builder makes before it gives up (`0x15`).
const MAX_WAITS: u8 = 21;
/// The drop-off's wait after a drop site out of reach (`0xc0400000`).
const DROP_WAIT: Seconds = Seconds::new(-3.0);
/// The drop-off reaches its drop site within this squared footprint gap (`DAT_14330eb8c`).
const DROP_REACH: f32 = crate::math::image::build::DROP_REACH;
/// The orders whose targets gone the unit AI drops after a drop-off (`FUN_140c5de60`: the bits
/// of `0x400011` over `order - 0x2c5`).
pub(crate) const DROPPED_ORDERS: [OrderKind; 3] =
    [OrderKind::Gather, OrderKind::Hunt, OrderKind::OnTarget];

#[cfg(test)]
mod stack_tests {
    use super::*;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::GroundOffset;
    use crate::map::Radii;
    use crate::map::spatial::{Placed, Tiles};
    use crate::movement::Speed;
    use crate::movement::TurnRate;
    use crate::objects::ListIndex;
    use crate::objects::ObjectState;
    use crate::objects::lifecycle::{Class, Lifecycle};
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionType;
    use crate::objects::masters::master_type;
    use crate::objects::masters::{BuildingMaster, Masters};
    use crate::objects::unit::Unit;
    use crate::players::attribute::{self, AttributeId};
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::test_ids::opt_master;
    use crate::unit_ai::UnitAiType;
    use crate::world::Millis;
    use crate::world::WorldTime;
    use crate::world::{Player, World};

    /// A building master of `id`: class 3, radius `r`, height `z`, obstruction `kind`, train
    /// time `bt`, hit points `hp`, an annex of `head`, with `annexes` at their offsets.
    fn building(
        id: i16,
        (r, z, kind): (f32, f32, ObstructionType),
        (bt, hp): (i16, i16),
        head: i16,
        annexes: &[(i16, GroundOffset)],
    ) -> Master {
        let mut slots = [None; 4];
        let mut offsets = [GroundOffset::ZERO; 4];
        for (k, &(a, o)) in annexes.iter().enumerate() {
            slots[k] = opt_master(a);
            offsets[k] = o;
        }
        Master {
            id: master_id(id),
            kind: master_type::BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(r, r),
            height: z,
            tall: 0.0 < z,
            obstruction_type: kind,
            obstruction_class: ObstructionClass::UNIT,
            build_time: bt,
            trained_at: vec![master_id(9999)],
            hit_points: hp,
            building: Some(BuildingMaster {
                stack_unit: None,
                foundation_terrain: -1,
                tech: None,
                annexes: slots,
                annex_offsets: offsets,
                head_unit: opt_master(head),
                transform_unit: None,
                ..BuildingMaster::default()
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
            (2.0, 4.0, ObstructionType::TYPE_2),
            (150, 2400),
            -1,
            &[],
        );
        if let Some(b) = foundation.building.as_mut() {
            b.stack_unit = Some(ids::TOWN_CENTRE);
            b.disappears_when_built = true;
        }
        let mut tc = building(
            109,
            (2.0, 0.0, ObstructionType::NONE),
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
            (Some(AttributeId::numbered(506)), Amount::new(1.0), 16),
            (None, Amount::ZERO, 0),
        ];
        tc.volley_missile = Some(master_id(54));
        let mut empty = building(
            890,
            (0.0, 4.0, ObstructionType::CIRCLE),
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
                (1.0, 3.0, ObstructionType::TYPE_2),
                (100, 2400),
                621,
                &[],
            ),
            building(
                619,
                (1.5, 0.0, ObstructionType::NONE),
                (100, 2400),
                621,
                &[
                    (1649, GroundOffset::new(0.75, -0.75)),
                    (2160, GroundOffset::ZERO),
                ],
            ),
            building(
                620,
                (1.0, 0.0, ObstructionType::NONE),
                (100, 2400),
                621,
                &[],
            ),
            building(1649, (0.0, 0.0, ObstructionType::NONE), (0, 1), 619, &[]),
            empty,
        ])
    }

    /// Player 1 with the town-centre foundation 8759 at (54, 147), built to its last 0.01
    /// seconds, its record on the map; the next id 8768.
    fn world() -> World {
        let ms = masters();
        let foundation = Unit {
            id: object_id(8759),
            body: crate::objects::layers::Body::at(Position::new(54.0, 147.0, 0.0)),
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Foundation,
                hp: HitPoints::new(2399.0),
                master_id: ids::TOWN_CENTRE_621,
                ..Lifecycle::default()
            },
            construction: Some(Construction {
                progress: 149.99,
                desolid: false,
                builders: 1,
                hit_points: 2400,
                build_time: 150,
                record: Obstruction {
                    kind: ObstructionType::TYPE_2,
                    class: ObstructionClass::UNIT,
                },
                snow: false,
                sprite_override: false,
            }),
            pasture: None,
            my_pasture: None,
            ..Unit::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            1,
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
        );
        t.spatial = Spatial::with_tiles(
            GridSize::new(200, 200),
            Tiles::new(GridSize::new(200, 200), vec![0; 200 * 200]),
        );
        t.spatial.terrain = Some(crate::map::terrain::Terrain::new(
            GridSize::new(200, 200),
            vec![(0, 255, -1); 200 * 200],
            vec![
                crate::map::terrain::TerrainKind {
                    shown: true,
                    flags: 0,
                    forest: false,
                };
                40
            ],
            vec![vec![0.0; 40]; 5],
        ));
        t.spatial.insert(
            object_id(8759),
            Placed {
                position: Position::new(54.0, 147.0, 0.0),
                radius: Footprint::new(2.0, 2.0),
                class: class::BUILDING,
                player: PlayerId::new(1),
                master: ids::TOWN_CENTRE_621,
                obstruction_type: ObstructionType::TYPE_2,
                buildable_on: 0,
                tall: true,
                speed: Speed::ZERO,
                unknown: None,
                obstruction: Some(Obstruction {
                    kind: ObstructionType::TYPE_2,
                    class: ObstructionClass::UNIT,
                }),
            },
        );
        t.next_object_id = object_id(8768);
        t.masters = vec![ms.clone(), ms];
        t
    }

    /// A castle's completion records its event in its owner's history between its storage and
    /// its counts, the owner's id as `data1` at the world's time (`FUN_1411dbb70`:
    /// `FUN_141560ae0` on `+0xdb8` with `(float)+0x718`, 0 and 0), then counts it.
    #[test]
    fn a_castle_s_completion_records_its_owner_s_id_in_its_history() {
        let mut t = world();
        t.world_time = WorldTime::from_millis(912_000);
        t.players[1].history.add_entry(0, 3.0, 0.0);
        for ms in &mut t.masters {
            let castle = building(
                82,
                (2.0, 4.0, ObstructionType::TYPE_2),
                (150, 2400),
                -1,
                &[],
            );
            *ms = Masters::new(ms.iter().cloned().chain([castle]));
        }
        t.players[1].objects[ListIndex::new(0)].life.master_id = ids::CASTLE;
        assert_eq!(t.work_building(object_id(8759), 1.0), Ok(false));
        assert_eq!(
            t.players[1].history.events,
            [crate::players::history::Event {
                kind: crate::players::history::EventKind::CASTLE_BUILT,
                time_slice: 1,
                world_time: WorldTime::from_millis(912_000),
                data1: 1.0,
                data2: 0.0,
                data3: 0.0,
            }]
        );
        assert_eq!(t.players[1].attributes[134], Amount::new(1.0));
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
    fn a_town_centre_foundation_completes_into_the_town_centre_and_its_annexes() {
        let mut t = world();
        assert_eq!(t.work_building(object_id(8759), 1.0), Ok(false));
        let ids: Vec<(i32, i16, ObjectState)> = t.players[1]
            .objects
            .iter()
            .map(|u| (u.id.get(), u.life.master_id.get(), u.life.state))
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
        let at = |id: ObjectId| t.spatial.get(id).unwrap().position;
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
        let unit = |id: ObjectId| t.players[1].objects.by_id(id).unwrap();
        assert_eq!(
            [8769, 8770, 8771, 8772, 8773].map(|a| unit(object_id(a)).annex_of),
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
            unit(object_id(8768)).ai.as_ref().map(|ai| ai.ai_type),
            Some(UnitAiType::TOWER)
        );
        assert_eq!(
            unit(object_id(8769)).ai.as_ref().map(|ai| ai.ai_type),
            Some(UnitAiType::BUILDING)
        );
        // the foundation lacked 0.84 of its 2400 hit points after its last work: the town
        // centre lacks them too
        let lacked = 2400.0 - (2399.0 + (0.01_f32 * 2400.0) / 150.0);
        assert_eq!(
            (
                unit(object_id(8768)).life.hp,
                unit(object_id(8768)).store.clone()
            ),
            (
                HitPoints::new(2400.0 - lacked),
                vec![(AttributeId::numbered(506), Amount::new(1.0))]
            )
        );
        let attributes = &t.players[1].attributes;
        assert_eq!(
            (attributes[4], attributes[98], attributes[165]),
            (Amount::new(5.0), Amount::ZERO, Amount::ZERO)
        );
        let gone = unit(object_id(8759));
        assert_eq!((gone.life.hp, gone.roo_called), (HitPoints::ZERO, true));
        assert_eq!(t.spatial.get(object_id(8759)).unwrap().obstruction, None);
    }

    /// A new building's record (`FUN_14118c540` from its place): a master without a height (a
    /// pasture's post, master 1888: collision height 0) gets none, though its obstruction kind
    /// and class would give one; with a height it does (431153923's builder, update 412).
    #[test]
    fn a_new_building_without_a_height_gets_no_record() {
        for (height, record) in [
            (0.0, None),
            (1.0, Some((ObstructionType::CIRCLE, ObstructionClass::UNIT))),
        ] {
            let mut t = world();
            for ms in &mut t.masters {
                let m = ms.get_mut(ids::TOWN_CENTRE).unwrap();
                m.build_time = 0;
                m.obstruction_type = ObstructionType::CIRCLE;
                m.obstruction_class = ObstructionClass::UNIT;
                m.height = height;
                m.building.as_mut().unwrap().annexes = [None; 4];
            }
            let (id, _) = t
                .make_building(
                    PlayerId::new(1),
                    ids::TOWN_CENTRE,
                    Ok(None),
                    Position::new(80.0, 80.0, 0.0),
                )
                .unwrap();
            let placed = t.spatial.get(id).unwrap().obstruction;
            assert_eq!(placed.map(|o| (o.kind, o.class)), record, "height {height}");
        }
    }

    /// A gate foundation's stack gate takes the foundation's lock (`FUN_1411dbb70`: the new
    /// gate's slot 0x670 with the old one's `+0xc7c`): locked, the new gate's last move time is
    /// the world time plus 1; unlocked it stays unlocked and keeps its last move time.
    #[test]
    fn a_gate_s_stack_gate_takes_the_foundation_s_lock() {
        for locked in [true, false] {
            let mut t = world();
            t.world_time = WorldTime::from_millis(5000);
            for ms in &mut t.masters {
                for id in [621, 109] {
                    let m = ms.get_mut(master_id(id)).unwrap();
                    m.class = class::GATE;
                    m.storage.clear();
                    m.volley_missile = None;
                    // every building holds its movement state
                    m.moving = Some(crate::movement::MoveStats {
                        fly_mode: 0,
                        speed: Speed::ZERO,
                        base: Speed::ZERO,
                        turn_rate: TurnRate::ZERO,
                        turn_radius: Distance::ZERO,
                        turn_radius_speed: Speed::MAX,
                        radius: Radii::new(2.0, 2.0, 0.0),
                        restriction: crate::map::RestrictionId::named(0),
                        restriction_mask: crate::map::TerrainMask::ALL,
                        turn_time: Seconds::ZERO,
                        move_algorithm: 0,
                        min_collision: 0.0,
                        fast_turn: false,
                        facets: None,
                        weapon_range: Distance::ZERO,
                    });
                    if let Some(b) = m.building.as_mut() {
                        b.annexes = [None; 4];
                    }
                }
            }
            t.players[1].objects[ListIndex::new(0)].gate = Some(crate::world::Gate {
                locked,
                ..crate::world::Gate::default()
            });
            t.stack_unit(
                PlayerId::new(1),
                object_id(8759),
                StackUnit {
                    master: ids::TOWN_CENTRE,
                    at: Position::new(54.0, 147.0, 0.0),
                    hp_lost: HitPoints::ZERO,
                },
            )
            .unwrap();
            let gate = t.players[1].objects.by_id(object_id(8768)).unwrap();
            assert_eq!(gate.gate.map(|g| g.locked), Some(locked));
            let last_move = gate.body.mover().map(|m| m.last_move);
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
    fn a_stack_unit_s_completion_reads_the_foundation_s_attacking_units() {
        let mut gone = world();
        gone.players[1].objects[ListIndex::new(0)].attacking_units = vec![object_id(99_999)];
        assert_eq!(gone.work_building(object_id(8759), 1.0), Ok(false));
        let mut held = world();
        let first = held.players[1].objects[ListIndex::new(0)].id;
        held.players[1].objects[ListIndex::new(0)].attacking_units = vec![first];
        assert_eq!(
            held.work_building(object_id(8759), 1.0),
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
