//! The attributes every object has, and the animated base class's.

use crate::element::{ReadHeader, WriteHeader, read_xyz};
use crate::fits;
use crate::reader::RecordingHeaderReader;
use crate::unit::SpriteList;
use crate::{ObjectId, PlayerId};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_dat::sprite::SpriteId;
use genie_error::GenieResult;
use genie_support::{UnitKindId, read_opt_u32};
use std::io::{Read, Write};

/// The attributes every object has.
// The bools are the format's on/off flags.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default, Clone)]
pub struct StaticUnitAttributes {
    /// The player who owns the object.
    pub owner_id: PlayerId,
    /// The object's unit type.
    pub unit_kind_id: UnitKindId,
    /// The sprite the object shows.
    pub sprite_id: SpriteId,
    /// The object it is garrisoned in, if any.
    pub garrisoned_in_id: Option<ObjectId>,
    /// The hit points.
    pub hit_points: f32,
    /// The object's life state (alive, dying, dead …).
    pub object_state: u8,
    /// Whether the object is asleep (not updated).
    pub sleep_flag: bool,
    /// Whether the object has a doppelganger, a stand-in shown under the fog of war.
    pub dopple_flag: bool,
    /// Whether the object falls asleep at its next update.
    pub go_to_sleep_flag: bool,
    /// The object's id.
    pub id: ObjectId,
    /// The facet (direction) the object faces.
    pub facet: u8,
    /// The position (x, y, z).
    pub position: (f32, f32, f32),
    /// The offset at which the object is drawn on screen.
    pub screen_offset: (u16, u16),
    /// The offset of the object's shadow.
    pub shadow_offset: (u16, u16),
    /// The control group the object is in; stored before save version 11.58 only.
    pub selected_group: Option<u8>,
    /// The attribute (resource) the object holds: the food of a sheep, the gold a villager
    /// carries.
    pub attribute_kind_held: u16,
    /// The amount of that attribute.
    pub attribute_amount_held: f32,
    /// The number of units working on the object (gatherers, builders …).
    pub worker_count: u8,
    /// The damage stage the object shows.
    pub current_damage: u8,
    /// The timer since the object was last damaged.
    pub damaged_lately_timer: u8,
    /// Whether the object is under attack.
    pub under_attack: bool,
    /// The objects in the object's pathing group (units that move together).
    pub pathing_group_members: Vec<ObjectId>,
    /// The object's group id, if any.
    pub group_id: Option<u32>,
    /// Not known yet.
    pub roo_already_called: u8,
    /// The sprites drawn for the object, if it has a sprite list.
    pub sprite_list: Option<SpriteList>,
}

impl ReadHeader for StaticUnitAttributes {
    // Reads the object's fields in file order, section by section.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut attrs = Self::read_identity(input)?;
        attrs.read_held_and_damage(input)?;
        attrs.read_group_and_sprites(input)?;
        Ok(attrs)
    }
}

impl StaticUnitAttributes {
    /// The owner, type, sprite and container, the hit points, the state and its flags, the
    /// id, facet, position and offsets: the first fields.
    fn read_identity<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            owner_id: input.read_u8()?.into(),
            unit_kind_id: input.read_u16::<LE>()?.into(),
            sprite_id: input.read_u16::<LE>()?.into(),
            garrisoned_in_id: read_opt_u32(input)?,
            hit_points: input.read_f32::<LE>()?,
            object_state: input.read_u8()?,
            sleep_flag: input.read_u8()? != 0,
            dopple_flag: input.read_u8()? != 0,
            go_to_sleep_flag: input.read_u8()? != 0,
            id: input.read_u32::<LE>()?.into(),
            facet: input.read_u8()?,
            position: read_xyz(input)?,
            screen_offset: (input.read_u16::<LE>()?, input.read_u16::<LE>()?),
            shadow_offset: (input.read_u16::<LE>()?, input.read_u16::<LE>()?),
            ..Default::default()
        })
    }

    /// The selected group (before 11.58; -1 is none), the attribute held and its amount, the
    /// workers, the damage and its timer and the under-attack flag.
    fn read_held_and_damage<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        if input.version() < 11.58 {
            self.selected_group = match input.read_i8()? {
                -1 => None,
                id => Some(fits(id)?),
            };
        }
        self.attribute_kind_held = input.read_u16::<LE>()?;
        self.attribute_amount_held = input.read_f32::<LE>()?;
        self.worker_count = input.read_u8()?;
        self.current_damage = input.read_u8()?;
        self.damaged_lately_timer = input.read_u8()?;
        self.under_attack = input.read_u8()? != 0;
        Ok(())
    }

    /// The pathing group's members behind their count, the group id, the
    /// `roo_already_called` flag and the sprite list behind a flag.
    fn read_group_and_sprites<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        let num_members = input.read_u32::<LE>()?;
        self.pathing_group_members = (0..num_members)
            .map(|_| Ok(input.read_u32::<LE>()?.into()))
            .collect::<GenieResult<_>>()?;
        self.group_id = read_opt_u32(input)?;
        self.roo_already_called = input.read_u8()?;
        if input.read_u8()? != 0 {
            self.sprite_list = Some(SpriteList::read_from(input)?);
        }
        Ok(())
    }
}

// The old format's objects are read only: the writer has no save version for the selected
// group, which only versions before 11.58 have.
impl WriteHeader for StaticUnitAttributes {}

/// The attributes the animated base class adds.
#[derive(Debug, Default, Clone, Copy)]
pub struct AnimatedUnitAttributes {
    /// The current movement speed.
    pub speed: f32,
}

impl ReadHeader for AnimatedUnitAttributes {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let speed = input.read_f32::<LE>()?;
        Ok(Self { speed })
    }
}

impl WriteHeader for AnimatedUnitAttributes {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_f32::<LE>(self.speed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
