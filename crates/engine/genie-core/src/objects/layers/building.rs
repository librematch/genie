//! `TRIBE_Building_Object` (type 80, FD-045-D06): the building layer, its fields in the save's
//! order (`TribeBuildingObject`), then the three the save does not hold. It is an object's part
//! (`World::parts.building`, FD-045-D56): an object the table does not hold reads
//! [`Building::NONE`], the game's defaults below type 80.

use crate::construction::build::Construction;
use crate::construction::{Annexes, MainBuildingId};
use crate::garrison::GarrisonCounts;
use crate::objects::Frame;
use crate::objects::ObjectId;
use crate::objects::Relics;
use crate::players::Amount;
use crate::world::Gate;
use crate::world::WorldTime;
use genie_diff::Diff;

/// The building layer of an object: its construction, its garrison's clock, its relics, its
/// main building, its production queue, its gate, its livestock and pasture
/// workers, the units that took it as their target, its rice flag, its stack unit, its frame,
/// its counts of the units inside and the bonus a farm owes its Make.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Building {
    /// A building's construction.
    pub(crate) construction: Option<Construction>,
    /// A building's `last_garrison_time` (`+0xbe0`): the world time of its last whole second,
    /// on which its garrison heals ([`crate::objects::unit::Unit::building_seconds`]).
    pub(crate) garrison_time: WorldTime,
    /// A building's `relic_count` and `specific_relic_count` (`+0xc20`, `+0xc24`).
    pub(crate) relics: Relics,
    /// An annex's main building (`+0xbb8`, the save's linked owner).
    pub(crate) annex_of: Option<MainBuildingId>,
    /// A main building's annexes (`+0xbc0`..`+0xbd8`, the save's linked objects).
    pub(crate) annexes: Annexes,
    /// A building's gate lock ([`Gate`]).
    pub(crate) gate: Option<Gate>,
    /// A building's `mLivestockSummation` (`+0xc28`, saved): the livestock inside it (their
    /// master's `+0x610`, `FUN_1411cb850`), whose food a mill makes ([`crate::players::world_player`]).
    pub(crate) livestock_summation: Amount,
    /// A pasture's livestock (`+0xb78`, saved as `livestockId`): the objects its completion put
    /// on its edges ([`crate::world::World::setup_pasture`]), in the order it made them.
    pub(crate) livestock: Vec<ObjectId>,
    /// A pasture's workers (`+0xcb8`, saved as `pastureWorkerId`): the units that tend it
    /// ([`crate::world::World::tend_pasture`]), in the order they came.
    pub(crate) pasture_workers: Vec<ObjectId>,
    /// `+0xce0`, `attackingUnits` (saved): the units of players its owner is not allied with
    /// that took this foundation (a building below state 2 whose master has a stack unit,
    /// `+0x986`) as their target, once per take, in order
    /// ([`crate::objects::unit::Unit::lists_taker`]); its stack unit's completion tells each of
    /// the new object.
    pub(crate) attacking_units: Vec<ObjectId>,
    /// A building's `+0xcd0`: its constructor found mangrove or rice terrain at its place
    /// (`FUN_141188db0`), which master 1889's completion reads.
    pub(crate) rice: bool,
    /// A building's `+0xcb0` (`FUN_1411c5f70`): the id of the object its completion's stack unit
    /// made ([`crate::world::World::stack_unit`]); None before (the save does not hold it).
    pub(crate) stack_made: Option<ObjectId>,
    /// A building's `+0xc54`: the frame its BUILD command chose (`FUN_140e7a400`, the packet's
    /// `+0x1c`), which its standing sprite takes once built; 0 from its constructor and the load.
    #[diff(unsaved)]
    pub(crate) build_frame: Frame,
    /// A building's counts of its villagers and its raiders inside (`+0xba8`, `+0xc2c`), which
    /// its slots 0x1c0 and 0x1c8 keep ([`GarrisonCounts`]); 0 from its constructors and the
    /// load, whose re-link counts the units inside in again.
    #[diff(unsaved)]
    pub(crate) garrison_counts: GarrisonCounts,
    /// A farm's `+0xcac`: its completion gave its owner the farm bonus (`FUN_1411c66a0`,
    /// [`FirstPart::farm_bonus`]), which its next update in state 2 with an action on top takes
    /// off that action, its Make ([`Unit::take_farm_bonus`]); false from its constructors and
    /// the load.
    ///
    /// [`FirstPart::farm_bonus`]: crate::construction::build::FirstPart::farm_bonus
    /// [`Unit::take_farm_bonus`]: crate::objects::unit::Unit::take_farm_bonus
    #[diff(unsaved)]
    pub(crate) owes_bonus: bool,
}

impl Building {
    /// The layer of an object that has none (below type 80), or of a new building: no
    /// construction, links, gate, livestock or relics.
    pub const NONE: Self = Self {
        construction: None,
        garrison_time: WorldTime::ZERO,
        relics: Relics::NONE,
        annex_of: None,
        annexes: Annexes::NONE,
        gate: None,
        livestock_summation: Amount::ZERO,
        livestock: Vec::new(),
        pasture_workers: Vec::new(),
        attacking_units: Vec::new(),
        rice: false,
        stack_made: None,
        build_frame: Frame::ZERO,
        garrison_counts: GarrisonCounts::NONE,
        owes_bonus: false,
    };
}
