use crate::victory::VictoryConditions;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::{ReadStringsExt, reserved, write_opt_str};
use std::io::{Read, Write};

#[derive(Debug, Default, Clone)]
pub struct PlayerBaseProperties {
    pub(crate) posture: i32,
    pub(crate) player_kind: i32,
    pub(crate) civilization: i32,
    pub(crate) active: i32,
}

#[derive(Debug, Default, Clone)]
pub struct PlayerFiles {
    /// Obsolete.
    pub(crate) build_list: Option<String>,
    /// Obsolete.
    pub(crate) city_plan: Option<String>,
    /// String content of the AI of this player.
    pub(crate) ai_rules: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlayerStartResources {
    pub(crate) gold: i32,
    pub(crate) wood: i32,
    pub(crate) food: i32,
    pub(crate) stone: i32,
    pub(crate) ore: i32,
    pub(crate) goods: i32,
    pub(crate) player_color: Option<i32>,
}

impl Default for PlayerStartResources {
    fn default() -> Self {
        Self {
            gold: 100,
            wood: 200,
            food: 200,
            stone: 200,
            ore: 100,
            goods: 0,
            player_color: None,
        }
    }
}

impl PlayerStartResources {
    pub fn read_from(mut input: impl Read, version: f32) -> GenieResult<Self> {
        Ok(Self {
            gold: input.read_i32::<LE>()?,
            wood: input.read_i32::<LE>()?,
            food: input.read_i32::<LE>()?,
            stone: input.read_i32::<LE>()?,
            ore: if version >= 1.17 {
                input.read_i32::<LE>()?
            } else {
                100
            },
            goods: if version >= 1.17 {
                input.read_i32::<LE>()?
            } else {
                0
            },
            player_color: if version >= 1.24 {
                Some(input.read_i32::<LE>()?)
            } else {
                None
            },
        })
    }

    pub fn write_to(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        output.write_i32::<LE>(self.gold)?;
        output.write_i32::<LE>(self.wood)?;
        output.write_i32::<LE>(self.food)?;
        output.write_i32::<LE>(self.stone)?;
        if version >= 1.17 {
            output.write_i32::<LE>(self.ore)?;
            output.write_i32::<LE>(self.goods)?;
        }
        if version >= 1.24 {
            output.write_i32::<LE>(self.player_color.unwrap_or(0))?;
        }
        Ok(())
    }
}

/// A player's scenario settings: name, camera, diplomacy, colour and victory conditions.
#[derive(Debug, Clone)]
pub struct ScenarioPlayerData {
    /// The player's default name, if the scenario sets one.
    pub name: Option<String>,
    /// The initial camera position, (x, y) in tiles.
    pub view: (f32, f32),
    /// The player's start location, (x, y) in tiles.
    pub location: (i16, i16),
    /// Whether the player can win together with its allies (stored after version 1.00).
    pub allied_victory: bool,
    /// The player's diplomacy byte toward each player, one per entry.
    pub relations: Vec<i8>,
    /// One stance per player slot (Gaia and players 1 to 8), stored from version 1.08;
    /// recorded games use 0 Gaia, 1 self, 2 ally, 3 neutral, 4 enemy, -1 no player.
    pub unit_diplomacy: Vec<i32>,
    /// The player colour, stored from version 1.13.
    pub color: Option<i32>,
    /// The player's own victory conditions.
    pub victory: VictoryConditions,
}

impl ScenarioPlayerData {
    /// Get the default player name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_ref().map(AsRef::as_ref)
    }

    /// Set the default player name.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = Some(name.into());
    }

    /// Read player data from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or the name cannot be decoded.
    // The format stores the diplomacy count as an i16; valid files never make it negative.
    #[allow(clippy::cast_sign_loss)]
    pub fn read_from(mut input: impl Read, version: f32) -> GenieResult<Self> {
        let name = input.read_u16_length_prefixed_str()?;

        let view = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);

        let location = (input.read_i16::<LE>()?, input.read_i16::<LE>()?);

        let allied_victory = if version > 1.0 {
            input.read_u8()? != 0
        } else {
            false
        };

        let diplo_count = input.read_i16::<LE>()?;
        let mut relations = reserved(diplo_count as usize);
        for _ in 0..diplo_count {
            relations.push(input.read_i8()?);
        }

        let unit_diplomacy = if version >= 1.08 {
            vec![
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
            ]
        } else {
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0]
        };

        let color = if version >= 1.13 {
            Some(input.read_i32::<LE>()?)
        } else {
            None
        };

        let victory = VictoryConditions::read_from(&mut input, version >= 1.09)?;

        Ok(Self {
            name,
            view,
            location,
            allied_victory,
            relations,
            unit_diplomacy,
            color,
            victory,
        })
    }

    /// Write player data to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or the name cannot be encoded.
    // The format stores the diplomacy count as an i16.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn write_to(
        &self,
        mut output: impl Write,
        version: f32,
        victory_version: f32,
    ) -> GenieResult<()> {
        write_opt_str(&mut output, &self.name)?;

        output.write_f32::<LE>(self.view.0)?;
        output.write_f32::<LE>(self.view.1)?;

        output.write_i16::<LE>(self.location.0)?;
        output.write_i16::<LE>(self.location.1)?;

        if version > 1.0 {
            output.write_u8(u8::from(self.allied_victory))?;
        }

        output.write_i16::<LE>(self.relations.len() as i16)?;
        for rel in &self.relations {
            output.write_i8(*rel)?;
        }

        if version >= 1.08 {
            output.write_i32::<LE>(self.unit_diplomacy[0])?;
            output.write_i32::<LE>(self.unit_diplomacy[1])?;
            output.write_i32::<LE>(self.unit_diplomacy[2])?;
            output.write_i32::<LE>(self.unit_diplomacy[3])?;
            output.write_i32::<LE>(self.unit_diplomacy[4])?;
            output.write_i32::<LE>(self.unit_diplomacy[5])?;
            output.write_i32::<LE>(self.unit_diplomacy[6])?;
            output.write_i32::<LE>(self.unit_diplomacy[7])?;
            output.write_i32::<LE>(self.unit_diplomacy[8])?;
        }

        if version >= 1.13 {
            output.write_i32::<LE>(self.color.unwrap_or(-1))?;
        }

        self.victory.write_to(
            &mut output,
            if version >= 1.09 {
                Some(victory_version)
            } else {
                None
            },
        )?;

        Ok(())
    }
}

/// Initial player attributes.
#[derive(Debug, Clone, Copy)]
pub struct WorldPlayerData {
    /// Initial food count.
    pub(crate) food: f32,
    /// Initial wood count.
    pub(crate) wood: f32,
    /// Initial gold count.
    pub(crate) gold: f32,
    /// Initial stone count.
    pub(crate) stone: f32,
    /// Initial ore count. (unused, except in some mods)
    pub(crate) ore: f32,
    /// Initial trade goods count. (unused)
    pub(crate) goods: f32,
    /// Max population.
    pub(crate) population: f32,
}

impl Default for WorldPlayerData {
    fn default() -> Self {
        Self {
            food: 200.0,
            wood: 200.0,
            gold: 100.0,
            stone: 200.0,
            ore: 100.0,
            goods: 0.0,
            population: 75.0,
        }
    }
}

impl WorldPlayerData {
    /// Read the initial resources and population limit; fields the version lacks get defaults.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails.
    pub fn read_from(mut input: impl Read, version: f32) -> GenieResult<Self> {
        Ok(Self {
            food: if version > 1.06 {
                input.read_f32::<LE>()?
            } else {
                200.0
            },
            wood: if version > 1.06 {
                input.read_f32::<LE>()?
            } else {
                200.0
            },
            gold: if version > 1.06 {
                input.read_f32::<LE>()?
            } else {
                50.0
            },
            stone: if version > 1.06 {
                input.read_f32::<LE>()?
            } else {
                100.0
            },
            ore: if version > 1.12 {
                input.read_f32::<LE>()?
            } else {
                100.0
            },
            goods: if version > 1.12 {
                input.read_f32::<LE>()?
            } else {
                0.0
            },
            population: if version >= 1.14 {
                input.read_f32::<LE>()?
            } else {
                75.0
            },
        })
    }

    /// Write the initial resources and population limit the version has.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    pub fn write_to(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        if version > 1.06 {
            output.write_f32::<LE>(self.food)?;
            output.write_f32::<LE>(self.wood)?;
            output.write_f32::<LE>(self.gold)?;
            output.write_f32::<LE>(self.stone)?;
        }
        if version > 1.12 {
            output.write_f32::<LE>(self.ore)?;
        }
        if version > 1.12 {
            output.write_f32::<LE>(self.goods)?;
        }
        if version >= 1.14 {
            output.write_f32::<LE>(self.population)?;
        }
        Ok(())
    }
}
