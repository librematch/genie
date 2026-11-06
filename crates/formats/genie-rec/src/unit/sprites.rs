//! The sprites drawn for an object: their nodes and animations.

use crate::element::{ReadHeader, ReadOptionalHeader, WriteHeader};
use crate::header_reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_dat::sprite::SpriteId;
use genie_error::GenieResult;
use std::io::{Read, Write};

/// The animation state of a sprite node.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteNodeAnimation {
    /// The time between two animation frames.
    pub animate_interval: u32,
    /// The time of the last frame change.
    pub animate_last: u32,
    /// The frame shown last.
    pub last_frame: u16,
    /// Whether the frame changed at the last update.
    pub frame_changed: u8,
    /// Whether the animation looped at the last update.
    pub frame_looped: u8,
    /// Whether the sprite animates.
    pub animate_flag: u8,
    /// The animation speed at the last update.
    pub last_speed: f32,
}

impl ReadHeader for SpriteNodeAnimation {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            animate_interval: input.read_u32::<LE>()?,
            animate_last: input.read_u32::<LE>()?,
            last_frame: input.read_u16::<LE>()?,
            frame_changed: input.read_u8()?,
            frame_looped: input.read_u8()?,
            animate_flag: input.read_u8()?,
            last_speed: input.read_f32::<LE>()?,
        })
    }
}

impl WriteHeader for SpriteNodeAnimation {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u32::<LE>(self.animate_interval)?;
        output.write_u32::<LE>(self.animate_last)?;
        output.write_u16::<LE>(self.last_frame)?;
        output.write_u8(self.frame_changed)?;
        output.write_u8(self.frame_looped)?;
        output.write_u8(self.animate_flag)?;
        output.write_f32::<LE>(self.last_speed)?;
        Ok(())
    }
}

/// One sprite drawn for an object; an object can carry several (a building and its damage
/// fires, for example).
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteNode {
    /// The sprite.
    pub id: SpriteId,
    /// The sprite's horizontal offset.
    pub x: u32,
    /// The sprite's vertical offset.
    pub y: u32,
    /// The frame shown.
    pub frame: u16,
    /// Whether the sprite is hidden.
    pub invisible: bool,
    /// The animation state, for an animated node.
    pub animation: Option<SpriteNodeAnimation>,
    /// The drawing order among the object's sprites.
    pub order: u8,
    /// Not known yet.
    pub flag: u8,
    /// Not known yet.
    pub count: u8,
}

impl ReadOptionalHeader for SpriteNode {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Option<Self>> {
        let ty = input.read_u8()?;
        if ty == 0 {
            return Ok(None);
        }

        Ok(Some(Self {
            id: input.read_u16::<LE>()?.into(),
            x: input.read_u32::<LE>()?,
            y: input.read_u32::<LE>()?,
            frame: input.read_u16::<LE>()?,
            invisible: input.read_u8()? != 0,
            animation: if ty == 2 {
                Some(SpriteNodeAnimation::read_from(input)?)
            } else {
                None
            },
            order: input.read_u8()?,
            flag: input.read_u8()?,
            count: input.read_u8()?,
        }))
    }
}

impl WriteHeader for SpriteNode {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        let ty = if self.animation.is_some() { 2 } else { 1 };
        output.write_u8(ty)?;
        output.write_u16::<LE>(self.id.into())?;
        output.write_u32::<LE>(self.x)?;
        output.write_u32::<LE>(self.y)?;
        output.write_u16::<LE>(self.frame)?;
        output.write_u8(u8::from(self.invisible))?;
        if let Some(animation) = &self.animation {
            animation.write_to(output)?;
        }
        output.write_u8(self.order)?;
        output.write_u8(self.flag)?;
        output.write_u8(self.count)?;
        Ok(())
    }
}

/// The sprites drawn for an object.
#[derive(Debug, Default, Clone)]
pub struct SpriteList {
    /// The sprite nodes, in drawing order.
    pub sprites: Vec<SpriteNode>,
}

impl ReadHeader for SpriteList {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut sprites = vec![];
        while let Some(node) = SpriteNode::read_from(input)? {
            sprites.push(node);
        }
        Ok(Self { sprites })
    }
}

impl WriteHeader for SpriteList {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        for sprite in &self.sprites {
            sprite.write_to(output)?;
        }
        output.write_u8(0)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::writes_back;

    /// A sprite list: each node behind its type (2 with an animation, 1 without), then a 0.
    #[test]
    fn sprite_list_writes_back_its_nodes() {
        let animation = SpriteNodeAnimation {
            animate_interval: 2,
            animate_last: 3,
            last_frame: 4,
            frame_changed: 1,
            frame_looped: 0,
            animate_flag: 1,
            last_speed: 0.5,
        };
        let list = SpriteList {
            sprites: vec![
                SpriteNode {
                    id: 12.into(),
                    x: 1,
                    y: 2,
                    frame: 3,
                    invisible: true,
                    animation: Some(animation),
                    order: 1,
                    flag: 2,
                    count: 3,
                },
                SpriteNode {
                    id: 13.into(),
                    animation: None,
                    ..SpriteNode::default()
                },
            ],
        };
        let read = writes_back(&list, 11.97);
        assert_eq!(read.sprites.len(), 2);
        assert_eq!(
            read.sprites[0]
                .animation
                .map(|sprite_node_animation| sprite_node_animation.last_speed),
            Some(0.5)
        );
        assert!(read.sprites[1].animation.is_none());
    }
}
