use crate::element::{ReadHeader, ReadOptionalHeader, WriteHeader};
use crate::fits;
use crate::reader::RecordingHeaderReader;
use crate::unit_kind::UnitBaseClass;
use byteorder::ReadBytesExt;
pub use genie_dat::sprite::SpriteId;
pub use genie_dat::terrain::TerrainId;
pub use genie_dat::unit_kind::AttributeCost;
use genie_error::GenieResult;
pub use genie_support::{StringKey, UnitKindId};
use std::io::Read;

mod building;
mod combat;
mod movers;
mod sprites;
mod statics;
mod unit_ai;

pub use self::building::{BuildingUnitAttributes, GatherPoint, ProductionQueueEntry};
pub use self::combat::{BaseCombatUnitAttributes, CombatUnitAttributes, MissileUnitAttributes};
pub use self::movers::{ActionUnitAttributes, MovementData, MovingUnitAttributes, PathData};
pub use self::sprites::{SpriteList, SpriteNode, SpriteNodeAnimation};
pub use self::statics::{AnimatedUnitAttributes, StaticUnitAttributes};
pub use self::unit_ai::{
    PatrolPath, UnitAi, UnitAiNotification, UnitAiOrder, UnitAiOrderHistory, UnitAiRetargetEntry,
    Waypoint,
};

/// A game object (unit, building, resource …) as the recording header stores it, with the
/// attribute groups of its base class.
#[derive(Debug, Clone)]
pub struct Unit {
    /// The object's base class; it decides which of the attribute groups below are present.
    pub unit_base_class: UnitBaseClass,
    /// The attributes every object has.
    pub static_: StaticUnitAttributes,
    /// Present from the animated base class on.
    pub animated: Option<AnimatedUnitAttributes>,
    /// Present from the moving base class on.
    pub moving: Option<MovingUnitAttributes>,
    /// Present from the action base class on.
    pub action: Option<ActionUnitAttributes>,
    /// Present from the base combat base class on.
    pub base_combat: Option<BaseCombatUnitAttributes>,
    /// Present from the missile base class on.
    pub missile: Option<MissileUnitAttributes>,
    /// Present from the combat base class on.
    pub combat: Option<CombatUnitAttributes>,
    /// Present for buildings.
    pub building: Option<BuildingUnitAttributes>,
}

impl ReadOptionalHeader for Unit {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Option<Self>> {
        let raw_class = input.read_u8()?;
        if raw_class == 0 {
            return Ok(None);
        }
        let unit_base_class = fits(raw_class)?;
        let static_ = StaticUnitAttributes::read_from(input)?;
        let mut unit = Self {
            unit_base_class,
            static_,
            animated: None,
            moving: None,
            action: None,
            base_combat: None,
            missile: None,
            combat: None,
            building: None,
        };
        if unit_base_class >= UnitBaseClass::Animated {
            unit.animated = Some(AnimatedUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Moving {
            unit.moving = Some(MovingUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Action {
            unit.action = Some(ActionUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::BaseCombat {
            unit.base_combat = Some(BaseCombatUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Missile {
            unit.missile = Some(MissileUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Combat {
            unit.combat = Some(CombatUnitAttributes::read_from(input)?);
        }
        if unit_base_class >= UnitBaseClass::Building {
            unit.building = Some(BuildingUnitAttributes::read_from(input)?);
        }
        Ok(Some(unit))
    }
}

// The old format's units are read only: their static attributes have no writer (see
// `StaticUnitAttributes`), nor have the moving, action and combat ones.
impl WriteHeader for Unit {}
