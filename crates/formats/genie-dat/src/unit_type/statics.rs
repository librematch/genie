//! The attributes every unit type has: the resource storage, the damage sprites and the static
//! part ([`StaticUnitTypeAttributes`]).

use crate::conv::{fits, invalid_data, opt_fits};
use crate::sound::SoundId;
use crate::sprite::{GraphicId, SpriteId};
use crate::unit_type::{UnitClass, write_opt_string_key};
use arrayvec::ArrayVec;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{StringKey, UnitTypeId, read_opt_u16};
use std::io::{Read, Result, Write};

/// A resource a unit stores (e.g. the food in a sheep), with how it is stored.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnitAttribute {
    /// Resource type; 0xFFFF marks an unused slot.
    pub attribute_type: u16,
    /// How much of the resource the unit holds.
    pub amount: f32,
    /// When the resource is stored or released (genieutils' storage `Flag`); not decoded here.
    pub flag: u8,
}

impl UnitAttribute {
    /// Read a resource storage slot from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        Ok(Self {
            attribute_type: input.read_u16::<LE>()?,
            amount: input.read_f32::<LE>()?,
            flag: input.read_u8()?,
        })
    }

    /// Write this resource storage slot to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(self, mut output: impl Write) -> Result<()> {
        output.write_u16::<LE>(self.attribute_type)?;
        output.write_f32::<LE>(self.amount)?;
        output.write_u8(self.flag)?;
        Ok(())
    }

    pub(super) fn write_empty(mut output: impl Write) -> Result<()> {
        output.write_u16::<LE>(0xFFFF)?;
        output.write_f32::<LE>(0.0)?;
        output.write_u8(0)?;
        Ok(())
    }
}

/// A sprite drawn on a unit once its damage passes a threshold (e.g. fire on a building).
#[derive(Debug, Default, Clone, Copy)]
pub struct DamageSprite {
    /// The sprite drawn.
    pub sprite: SpriteId,
    /// Damage, in percent of the unit's hit points, from which the sprite is drawn.
    pub damage_percent: u16,
    /// How the sprite is drawn over the unit (genieutils' `ApplyMode`); not decoded here.
    pub flag: u8,
}

impl DamageSprite {
    /// Read a damage sprite from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        Ok(Self {
            sprite: input.read_u16::<LE>()?.into(),
            damage_percent: input.read_u16::<LE>()?,
            flag: input.read_u8()?,
        })
    }

    /// Write this damage sprite to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u16::<LE>(self.sprite.into())?;
        output.write_u16::<LE>(self.damage_percent)?;
        output.write_u8(self.flag)?;
        Ok(())
    }
}

/// The attributes every unit type has (genieutils' `Unit` without its class-specific parts).
// The bools are flags of the file format.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default, Clone)]
pub struct StaticUnitTypeAttributes {
    pub(super) name: String,
    /// ID of this unit type.
    pub id: UnitTypeId,
    /// Language string ID of the unit's name.
    pub string_id: StringKey,
    pub(super) string_id2: Option<StringKey>,
    /// The unit's class (archer, infantry, building, …).
    pub unit_class: UnitClass,
    /// Sprite shown while the unit stands still, or `None`.
    pub standing_sprite_1: Option<SpriteId>,
    /// A second standing sprite (genieutils' second `StandingGraphic`), or `None`.
    pub standing_sprite_2: Option<SpriteId>,
    /// Sprite played when the unit dies, or `None`.
    pub dying_sprite: Option<SpriteId>,
    /// Sprite played after the dying sprite if `undead_flag` is set (genieutils' `UndeadGraphic`),
    /// or `None`.
    pub undead_sprite: Option<SpriteId>,
    /// Whether the unit plays its undead sprite after dying (genieutils' `UndeadMode`).
    pub undead_flag: u8,
    /// Hit points.
    pub hp: u16,
    /// Line of sight, in tiles.
    pub los: f32,
    /// How many units can garrison in this unit.
    pub garrison_capacity: u8,
    /// Collision size (x, y, z), in tiles.
    pub radius: (f32, f32, f32),
    /// Sound played when the unit is created, or `None`.
    pub train_sound: Option<SoundId>,
    /// Sound played when the unit takes damage, or `None`.
    pub damage_sound: Option<SoundId>,
    /// Unit type that replaces this unit when it dies (e.g. its corpse), or `None`.
    pub death_spawn: Option<UnitTypeId>,
    /// genieutils' `SortNumber`; its use is not known yet.
    pub sort_number: u8,
    /// Whether buildings can be placed over this unit.
    pub can_be_built_on: bool,
    /// Icon of the unit's button and selection panel, or `None`.
    pub button_picture: Option<GraphicId>,
    /// Whether the scenario editor hides this unit type.
    pub hide_in_scenario_editor: bool,
    /// Old portrait picture ID (genieutils' `OldPortraitPict`), or `None`.
    pub portrait_picture: Option<GraphicId>,
    /// Whether the unit type is available at the start of a game.
    pub enabled: bool,
    /// genieutils' `Disabled` flag; its effect is not known yet.
    pub disabled: bool,
    /// Two terrains of which one must be next to the unit to place it, -1 for none (genieutils'
    /// `PlacementSideTerrain`).
    pub tile_req: (i16, i16),
    /// Two terrains of which one must be under the unit to place it, -1 for none (genieutils'
    /// `PlacementTerrain`).
    pub center_tile_req: (i16, i16),
    /// Clearance size (x, y): the free area needed to place the unit, in tiles.
    pub construction_radius: (f32, f32),
    /// How the unit can be placed on hills (genieutils' `HillMode`), read as a flag.
    pub elevation_flag: bool,
    /// Whether the unit stays visible under the fog of war (genieutils' `FogVisibility`), read as a
    /// flag.
    pub fog_flag: bool,
    /// The terrain restriction (terrain table) that says where the unit can be.
    pub terrain_restriction_id: u16,
    /// Whether and how the unit flies (genieutils' `FlyMode`).
    pub movement_type: u8,
    /// The most resource the unit can hold (genieutils' `ResourceCapacity`).
    pub attribute_max_amount: u16,
    /// How fast the unit's resource decays (genieutils' `ResourceDecay`).
    pub attribute_rot: f32,
    /// Which area (blast) damage the unit takes (genieutils' `BlastDefenseLevel`).
    pub area_effect_level: u8,
    /// Combat level (genieutils' `CombatLevel`); not decoded here.
    pub combat_level: u8,
    /// How the player can interact with the unit (genieutils' `InteractionMode`).
    pub select_level: u8,
    /// How the unit is shown on the minimap (genieutils' `MinimapMode`).
    pub map_draw_level: u8,
    /// Which interface the unit's selection uses (genieutils' `InterfaceKind`).
    pub unit_level: u8,
    /// genieutils' `MultipleAttributeMode`; its use is not known yet.
    pub multiple_attribute_mod: f32,
    /// Minimap colour (a palette index).
    pub map_color: u8,
    /// Language string ID of the unit's help text.
    pub help_string_id: StringKey,
    /// Language string ID that follows the help text (genieutils' `LanguageDLLHotKeyText`).
    pub help_page_id: u32,
    /// Hotkey ID of the unit's button.
    pub hotkey_id: u32,
    /// genieutils' `Recyclable` flag; its effect is not known yet.
    pub recyclable: bool,
    /// Whether gatherers pick the unit up as a resource on their own (genieutils'
    /// `EnableAutoGather`).
    pub track_as_resource: bool,
    /// Whether a doppelganger (the copy seen under the fog of war) is left when the unit dies.
    pub create_doppleganger: bool,
    /// The group of resources the unit is gathered as (genieutils' `ResourceGatherGroup`).
    pub resource_group: u8,
    /// How the unit is drawn when it is hidden behind others (genieutils' `OcclusionMode`).
    pub occlusion_mask: u8,
    /// Shape of the unit's obstruction (genieutils' `ObstructionType`).
    pub obstruction_type: u8,
    /// The byte genieutils reads as `ObstructionClass`; genie-rs takes it as the selection shape.
    pub selection_shape: u8,
    /// Object flags, stored as a u32 from data version 11.55 on; 0 for older files, which lack
    /// them.
    pub object_flags: u32,
    /// The civilization this unit type belongs to.
    pub civilization: u8,
    /// Not known yet (genieutils' `Nothing`).
    pub attribute_piece: u8,
    /// Size of the selection outline (x, y, z), in tiles.
    pub outline_radius: (f32, f32, f32),
    /// The resources the unit stores, at most 3; unused slots are dropped when reading.
    pub attributes: ArrayVec<UnitAttribute, 3>,
    /// Sprites drawn on the unit as it takes damage.
    pub damage_sprites: Vec<DamageSprite>,
    /// Sound played when the unit is selected, or `None`.
    pub selected_sound: Option<SoundId>,
    /// Sound played when the unit dies, or `None`.
    pub death_sound: Option<SoundId>,
    /// How the unit reacts to attacks (genieutils' `OldAttackReaction`).
    pub attack_reaction: u8,
    /// genieutils' `ConvertTerrain` flag; its effect is not known yet.
    pub convert_terrain_flag: u8,
    /// ID of the unit type this one copies; newer files also use it to pick the unit's task list
    /// (see [`ActionUnitTypeAttributes::tasks`]).
    ///
    /// [`ActionUnitTypeAttributes::tasks`]: crate::unit_type::movers::ActionUnitTypeAttributes::tasks
    pub copy_id: u16,
    /// ID of the unit's base unit type (genieutils' `BaseID`).
    pub unit_group: u16,
}

impl StaticUnitTypeAttributes {
    /// Read the static attributes from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    ///
    /// Also fails if the name is not valid UTF-8.
    pub fn read_from(mut input: impl Read, version: f32) -> Result<Self> {
        let mut unit_type = Self::default();
        let name_len = input.read_u16::<LE>()?;
        unit_type.read_identity(&mut input)?;
        unit_type.read_sounds_and_placement(&mut input)?;
        unit_type.read_levels(&mut input)?;
        unit_type.read_resource_flags(&mut input, version)?;
        unit_type.read_attributes(&mut input)?;
        unit_type.name = {
            // TODO use not-UTF8 for the name
            let mut bytes = vec![0; usize::from(name_len)];
            input.read_exact(&mut bytes)?;
            String::from_utf8(bytes.iter().copied().take_while(|b| *b != 0).collect())
                .map_err(invalid_data)?
        };
        unit_type.copy_id = input.read_u16::<LE>()?;
        unit_type.unit_group = input.read_u16::<LE>()?;
        Ok(unit_type)
    }

    /// The ids, the class, the sprites, the hit points, the line of sight, the garrison and the
    /// radius.
    fn read_identity(&mut self, mut input: impl Read) -> Result<()> {
        self.id = input.read_u16::<LE>()?.into();
        self.string_id = input.read_u16::<LE>()?.into();
        self.string_id2 = read_opt_u16(&mut input)?;
        self.unit_class = input.read_u16::<LE>()?;
        self.standing_sprite_1 = read_opt_u16(&mut input)?;
        self.standing_sprite_2 = read_opt_u16(&mut input)?;
        self.dying_sprite = read_opt_u16(&mut input)?;
        self.undead_sprite = read_opt_u16(&mut input)?;
        self.undead_flag = input.read_u8()?;
        self.hp = input.read_u16::<LE>()?;
        self.los = input.read_f32::<LE>()?;
        self.garrison_capacity = input.read_u8()?;
        self.radius = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        Ok(())
    }

    /// The sounds, the death spawn, the buttons, the editor flags, the tile and construction
    /// sizes and the terrain and movement types.
    fn read_sounds_and_placement(&mut self, mut input: impl Read) -> Result<()> {
        self.train_sound = read_opt_u16(&mut input)?;
        self.damage_sound = read_opt_u16(&mut input)?;
        self.death_spawn = read_opt_u16(&mut input)?;
        self.sort_number = input.read_u8()?;
        self.can_be_built_on = input.read_u8()? != 0;
        self.button_picture = read_opt_u16(&mut input)?;
        self.hide_in_scenario_editor = input.read_u8()? != 0;
        self.portrait_picture = read_opt_u16(&mut input)?;
        self.enabled = input.read_u8()? != 0;
        self.disabled = input.read_u8()? != 0;
        self.tile_req = (input.read_i16::<LE>()?, input.read_i16::<LE>()?);
        self.center_tile_req = (input.read_i16::<LE>()?, input.read_i16::<LE>()?);
        self.construction_radius = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        self.elevation_flag = input.read_u8()? != 0;
        self.fog_flag = input.read_u8()? != 0;
        self.terrain_restriction_id = input.read_u16::<LE>()?;
        self.movement_type = input.read_u8()?;
        Ok(())
    }

    /// The attribute's amount and decay, the levels, the map colour and the help ids.
    fn read_levels(&mut self, mut input: impl Read) -> Result<()> {
        self.attribute_max_amount = input.read_u16::<LE>()?;
        self.attribute_rot = input.read_f32::<LE>()?;
        self.area_effect_level = input.read_u8()?;
        self.combat_level = input.read_u8()?;
        self.select_level = input.read_u8()?;
        self.map_draw_level = input.read_u8()?;
        self.unit_level = input.read_u8()?;
        self.multiple_attribute_mod = input.read_f32::<LE>()?;
        self.map_color = input.read_u8()?;
        self.help_string_id = input.read_u32::<LE>()?.into();
        self.help_page_id = input.read_u32::<LE>()?;
        self.hotkey_id = input.read_u32::<LE>()?;
        Ok(())
    }

    /// The resource flags, the occlusion, obstruction and selection, the object flags from
    /// 11.55 on, the civilization and the outline.
    fn read_resource_flags(&mut self, mut input: impl Read, version: f32) -> Result<()> {
        self.recyclable = input.read_u8()? != 0;
        self.track_as_resource = input.read_u8()? != 0;
        self.create_doppleganger = input.read_u8()? != 0;
        self.resource_group = input.read_u8()?;
        self.occlusion_mask = input.read_u8()?;
        self.obstruction_type = input.read_u8()?;
        self.selection_shape = input.read_u8()?;
        self.object_flags = if version < 11.55 {
            0
        } else {
            input.read_u32::<LE>()?
        };
        self.civilization = input.read_u8()?;
        self.attribute_piece = input.read_u8()?;
        self.outline_radius = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        Ok(())
    }

    /// The three attributes (type 0xFFFF none), the damage sprites, the selected and death
    /// sounds, the attack reaction and the terrain conversion.
    fn read_attributes(&mut self, mut input: impl Read) -> Result<()> {
        for _ in 0..3 {
            let attr = UnitAttribute::read_from(&mut input)?;
            if attr.attribute_type != 0xFFFF {
                self.attributes.push(attr);
            }
        }
        self.damage_sprites = {
            let num_damage_sprites = input.read_u8()?;
            let mut damage_sprites = vec![];
            for _ in 0..num_damage_sprites {
                damage_sprites.push(DamageSprite::read_from(&mut input)?);
            }
            damage_sprites
        };
        self.selected_sound = read_opt_u16(&mut input)?;
        self.death_sound = read_opt_u16(&mut input)?;
        self.attack_reaction = input.read_u8()?;
        self.convert_terrain_flag = input.read_u8()?;
        Ok(())
    }

    /// Write this unit type to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails, or if the second string ID does not fit an
    /// `i16`.
    ///
    /// Also fails if the name or help string key is a name rather than a number or does not fit the
    /// format's field, if a sprite, sound or unit ID does not fit an `i16`, or if there are more
    /// than 255 damage sprites.
    // The format stores the name length as a u16 (truncated, as before).
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write, _version: f32) -> Result<()> {
        // TODO use not-UTF8 for the name
        output.write_u16::<LE>(self.name.len() as u16)?;
        self.write_identity(&mut output)?;
        self.write_sounds_and_placement(&mut output)?;
        self.write_levels(&mut output)?;
        self.write_resource_flags(&mut output)?;
        self.write_attributes(&mut output)?;
        output.write_all(self.name.as_bytes())?;
        output.write_u16::<LE>(self.copy_id)?;
        output.write_u16::<LE>(self.unit_group)?;
        Ok(())
    }

    /// The ids, the class, the sprites, the hit points, the line of sight, the garrison and the
    /// radius.
    fn write_identity(&self, mut output: impl Write) -> Result<()> {
        output.write_u16::<LE>(self.id.into())?;
        output.write_i16::<LE>(fits(&self.string_id)?)?;
        write_opt_string_key(&mut output, self.string_id2.as_ref())?;
        output.write_u16::<LE>(self.unit_class)?;
        output.write_i16::<LE>(opt_fits(self.standing_sprite_1, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.standing_sprite_2, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.dying_sprite, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.undead_sprite, -1)?)?;
        output.write_u8(self.undead_flag)?;
        output.write_u16::<LE>(self.hp)?;
        output.write_f32::<LE>(self.los)?;
        output.write_u8(self.garrison_capacity)?;
        output.write_f32::<LE>(self.radius.0)?;
        output.write_f32::<LE>(self.radius.1)?;
        output.write_f32::<LE>(self.radius.2)?;
        Ok(())
    }

    /// The sounds, the death spawn, the buttons, the editor flags, the tile and construction
    /// sizes and the terrain and movement types.
    fn write_sounds_and_placement(&self, mut output: impl Write) -> Result<()> {
        output.write_i16::<LE>(opt_fits(self.train_sound, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.damage_sound, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.death_spawn, -1)?)?;
        output.write_u8(self.sort_number)?;
        output.write_u8(u8::from(self.can_be_built_on))?;
        output.write_i16::<LE>(opt_fits(self.button_picture, -1)?)?;
        output.write_u8(u8::from(self.hide_in_scenario_editor))?;
        output.write_i16::<LE>(opt_fits(self.portrait_picture, -1)?)?;
        output.write_u8(u8::from(self.enabled))?;
        output.write_u8(u8::from(self.disabled))?;
        output.write_i16::<LE>(self.tile_req.0)?;
        output.write_i16::<LE>(self.tile_req.1)?;
        output.write_i16::<LE>(self.center_tile_req.0)?;
        output.write_i16::<LE>(self.center_tile_req.1)?;
        output.write_f32::<LE>(self.construction_radius.0)?;
        output.write_f32::<LE>(self.construction_radius.1)?;
        output.write_u8(u8::from(self.elevation_flag))?;
        output.write_u8(u8::from(self.fog_flag))?;
        output.write_u16::<LE>(self.terrain_restriction_id)?;
        output.write_u8(self.movement_type)?;
        Ok(())
    }

    /// The attribute's amount and decay, the levels, the map colour and the help ids.
    fn write_levels(&self, mut output: impl Write) -> Result<()> {
        output.write_u16::<LE>(self.attribute_max_amount)?;
        output.write_f32::<LE>(self.attribute_rot)?;
        output.write_u8(self.area_effect_level)?;
        output.write_u8(self.combat_level)?;
        output.write_u8(self.select_level)?;
        output.write_u8(self.map_draw_level)?;
        output.write_u8(self.unit_level)?;
        output.write_f32::<LE>(self.multiple_attribute_mod)?;
        output.write_u8(self.map_color)?;
        output.write_u32::<LE>(fits(&self.help_string_id)?)?;
        output.write_u32::<LE>(self.help_page_id)?;
        output.write_u32::<LE>(self.hotkey_id)?;
        Ok(())
    }

    /// The resource flags, the occlusion, obstruction and selection, the object flags, the
    /// civilization and the outline.
    fn write_resource_flags(&self, mut output: impl Write) -> Result<()> {
        output.write_u8(u8::from(self.recyclable))?;
        output.write_u8(u8::from(self.track_as_resource))?;
        output.write_u8(u8::from(self.create_doppleganger))?;
        output.write_u8(self.resource_group)?;
        output.write_u8(self.occlusion_mask)?;
        output.write_u8(self.obstruction_type)?;
        output.write_u8(self.selection_shape)?;
        output.write_u32::<LE>(self.object_flags)?;
        output.write_u8(self.civilization)?;
        output.write_u8(self.attribute_piece)?;
        output.write_f32::<LE>(self.outline_radius.0)?;
        output.write_f32::<LE>(self.outline_radius.1)?;
        output.write_f32::<LE>(self.outline_radius.2)?;
        Ok(())
    }

    /// The three attribute slots, the damage sprites and their count, the selected and death
    /// sounds, the attack reaction and the terrain conversion.
    fn write_attributes(&self, mut output: impl Write) -> Result<()> {
        for index in 0..self.attributes.capacity() {
            match self.attributes.get(index) {
                Some(attr) => attr.write_to(&mut output)?,
                None => UnitAttribute::write_empty(&mut output)?,
            }
        }
        output.write_u8(fits(self.damage_sprites.len())?)?;
        for sprite in &self.damage_sprites {
            sprite.write_to(&mut output)?;
        }
        output.write_i16::<LE>(opt_fits(self.selected_sound, -1)?)?;
        output.write_i16::<LE>(opt_fits(self.death_sound, -1)?)?;
        output.write_u8(self.attack_reaction)?;
        output.write_u8(self.convert_terrain_flag)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit type with a value in each section: two attributes, a damage sprite, sprites and
    /// sounds, flags and a name.
    fn archer() -> StaticUnitTypeAttributes {
        let mut unit = StaticUnitTypeAttributes {
            name: "Archer".to_owned(),
            id: 4.into(),
            string_id: StringKey::from(5084u32),
            unit_class: 0,
            standing_sprite_1: Some(1u16.into()),
            hp: 30,
            los: 6.0,
            radius: (0.2, 0.2, 2.0),
            train_sound: Some(SoundId::from(8u16)),
            enabled: true,
            terrain_restriction_id: 7,
            attribute_rot: 0.5,
            combat_level: 4,
            help_string_id: StringKey::from(26084u32),
            hotkey_id: 16084,
            recyclable: true,
            object_flags: 0x40,
            outline_radius: (0.2, 0.2, 0.0),
            damage_sprites: vec![DamageSprite {
                sprite: SpriteId::from(3u16),
                damage_percent: 25,
                flag: 1,
            }],
            selected_sound: Some(SoundId::from(9u16)),
            copy_id: 4,
            unit_group: 4,
            ..StaticUnitTypeAttributes::default()
        };
        unit.attributes.push(UnitAttribute {
            attribute_type: 0,
            amount: 1.0,
            flag: 1,
        });
        unit.attributes.push(UnitAttribute {
            attribute_type: 4,
            amount: 0.5,
            flag: 0,
        });
        unit
    }

    /// A unit type's bytes from 11.55 on (with its object flags) read back and written again
    /// give the same bytes; the name ends the record but for the copy id and group.
    #[test]
    fn a_static_unit_type_writes_back_the_bytes_it_read() {
        let mut bytes = vec![];
        archer().write_to(&mut bytes, 12.0).unwrap();
        let unit = StaticUnitTypeAttributes::read_from(bytes.as_slice(), 12.0).unwrap();
        assert_eq!(
            (
                unit.name.as_str(),
                unit.hp,
                unit.object_flags,
                unit.attributes.len()
            ),
            ("Archer", 30, 0x40, 2)
        );
        assert_eq!(unit.damage_sprites.len(), 1);
        let mut again = vec![];
        unit.write_to(&mut again, 12.0).unwrap();
        assert_eq!(again, bytes);
    }

    /// Before 11.55 the record has no object flags: they read as 0.
    #[test]
    fn an_old_unit_type_has_no_object_flags() {
        let unit = archer();
        let mut bytes = vec![];
        unit.write_to(&mut bytes, 12.0).unwrap();
        // the object flags' four bytes follow the levels and seven bytes of flags and shapes
        let mut head = vec![0, 0];
        unit.write_identity(&mut head).unwrap();
        unit.write_sounds_and_placement(&mut head).unwrap();
        unit.write_levels(&mut head).unwrap();
        let flags_at = head.len() + 7;
        let old = [&bytes[..flags_at], &bytes[flags_at + 4..]].concat();
        let read = StaticUnitTypeAttributes::read_from(old.as_slice(), 11.5).unwrap();
        assert_eq!((read.object_flags, read.name.as_str()), (0, "Archer"));
    }
}
