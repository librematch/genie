//! The player colours, the sounds and the graphics with their deltas and angle sounds.

use crate::de::DebugString;
use binrw::binrw;

/// A player colour: the palette indices of a player's units, outlines, selection markers and
/// minimap dots.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerColour {
    /// The colour's id.
    pub id: i32,
    /// First palette index of the colour's range of player-colour shades.
    pub player_color_base: i32,
    /// Palette index of the outline drawn for the player's units behind other objects.
    pub unit_outline_color: i32,
    /// Palette index of the first selection-marker colour.
    pub unit_selection_color1: i32,
    /// Palette index of the second selection-marker colour.
    pub unit_selection_color2: i32,
    /// Palette index of the player's dots on the minimap.
    pub minimap_colour: i32,
    /// Second minimap palette index; its use is not known yet.
    pub minimap_color2: i32,
    /// Third minimap palette index; its use is not known yet.
    pub minimap_color3: i32,
    /// Palette index of the player's text on the statistics screens.
    pub statistics_text: i32,
}

/// A sound: a set of sample files the game picks one of by probability.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sound {
    /// The sound's id.
    pub id: i16,
    /// Delay before the sound plays; its unit is not known yet.
    pub play_delay: i16,
    /// Number of sample files in `items`.
    pub file_count: i16,
    /// How long a loaded sample stays cached; its unit is not known yet.
    pub cache_time: i32,
    /// The total the files' `probability` values are drawn against.
    pub total_probability: i16,
    /// The sample files.
    #[br(count = file_count)]
    pub items: Vec<SoundItem>,
}

/// One sample file of a [`Sound`].
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundItem {
    /// File name of the sample.
    pub file_name: DebugString,
    /// Id of the sample in the game's resource archives.
    pub resource_id: i32,
    /// Chance of picking this file, out of the sound's `total_probability`.
    pub probability: i16,
    /// Civilization the file is for; -1 for every civilization.
    pub civilization: i16,
    /// Architecture set the file is for; -1 for every set.
    pub icon_set: i16,
}

/// A graphic (sprite): its file, frames, angles, sounds and the deltas drawn with it.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Graphic {
    /// Internal name.
    pub name: DebugString,
    /// File name of the sprite.
    pub file_name: DebugString,
    /// Name of the particle effect played with the graphic.
    pub particle_effect_name: DebugString,
    /// Id of the graphic's SLP in the resource archives; -1 for none.
    pub slp: i32,
    /// A runtime flag saved with the file; not used when reading.
    pub is_loaded: u8,
    /// A colour flag older versions used; not used in DE as far as known.
    pub old_color_flag: u8,
    /// Draw layer: higher layers draw on top, graphics on one layer by map position.
    pub layer: u8,
    /// Player colour forced on the graphic; -1 uses the owner's colour.
    pub player_color: i16,
    /// Whether clicks on transparent pixels select the object; its exact modes are not known yet.
    pub transparent_selection: u8,
    /// Bounding box in pixels: left, top, right, bottom.
    pub coordinates: [i16; 4],
    /// Number of deltas in `deltas`.
    pub delta_count: i16,
    /// Sound played with the graphic; -1 for none.
    pub sound_id: i16,
    /// Wwise event id of the graphic's sound.
    pub wwise_sound_id: u32,
    /// Non-zero if `angle_sounds` holds one entry per angle.
    pub angle_sounds_used: u8,
    /// Number of frames per angle.
    pub frame_count: i16,
    /// Number of facing angles; some of them are drawn mirrored.
    pub angle_count: i16,
    /// Multiplier on the speed of a unit showing the graphic.
    pub speed_multiplier: f32,
    /// Time each frame shows, in seconds.
    pub frame_duration: f32,
    /// Time before the animation starts again, in seconds.
    pub replay_delay: f32,
    /// Bit flags for how the animation plays (e.g. looping); not all known yet.
    pub sequence_kind: u8,
    /// The graphic's id.
    pub id: i16,
    /// How the missing angles are mirrored from the stored ones.
    pub mirroring_mode: u8,
    /// A flag for the sprite editor; not used by the game as far as known.
    pub editor_flag: u8,
    /// Further graphics drawn with this one.
    #[br(count = delta_count)]
    pub deltas: Vec<GraphicDelta>,
    /// Per-angle sounds, present when `angle_sounds_used` is non-zero.
    #[br(count = if angle_sounds_used != 0 {
        usize::try_from(angle_count).unwrap_or(0)
    } else {
        0
    })]
    pub angle_sounds: Vec<GraphicAngleSound>,
}

/// A graphic drawn together with another one, at an offset.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicDelta {
    /// The graphic drawn; -1 for none.
    pub graphic_id: i16,
    /// Padding; kept to write the file back.
    pub padding1: i16,
    /// An in-memory pointer the game saved with the file; meaningless on disk, kept to write the
    /// file back.
    pub sprite_ptr: i32,
    /// Horizontal offset in pixels.
    pub offset_x: i16,
    /// Vertical offset in pixels.
    pub offset_y: i16,
    /// Angle at which the delta shows; -1 for every angle.
    pub display_angle: i16,
    /// Padding; kept to write the file back.
    pub padding2: i16,
}

/// The sounds of one angle of a graphic: up to three, each played at a frame.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicAngleSound {
    /// Frame at which the first sound plays.
    pub frame_num: i16,
    /// First sound; -1 for none.
    pub sound_id: i16,
    /// Wwise event id of the first sound.
    pub wwise_sound_id: u32,
    /// Frame at which the second sound plays.
    pub frame_num2: i16,
    /// Second sound; -1 for none.
    pub sound_id2: i16,
    /// Wwise event id of the second sound.
    pub wwise_sound_id2: u32,
    /// Frame at which the third sound plays.
    pub frame_num3: i16,
    /// Third sound; -1 for none.
    pub sound_id3: i16,
    /// Wwise event id of the third sound.
    pub wwise_sound_id3: u32,
}
