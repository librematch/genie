//! The diplomacy, emotional, resource, strategy and tactical AI modules.

use crate::ai::read_id_list;
use crate::element::{ReadableHeaderElement, WritableHeaderElement};
use crate::reader::RecordingHeaderReader;
use crate::unit::Waypoint;
use crate::{ObjectId, Result};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::ReadStringsExt;
use std::io::{Read, Write};

/// The diplomacy AI module: how the AI feels about each player.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiplomacyAi {
    /// How much this AI dislikes each player, 0-100.
    pub dislike: [u32; 10],
    /// How much this AI likes each player, 0-100.
    pub like: [u32; 10],
    /// Whether the AI may change its stance towards each player.
    pub changeable: [u8; 10],
}

impl ReadableHeaderElement for DiplomacyAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut ai = Self::default();
        for i in 0..10 {
            ai.dislike[i] = input.read_u32::<LE>()?;
            ai.like[i] = input.read_u32::<LE>()?;
            ai.changeable[i] = input.read_u8()?;
        }
        Ok(ai)
    }
}

/// The emotional AI module.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmotionalAi {
    /// The AI's six emotional state values.
    pub state: [u32; 6],
}

impl ReadableHeaderElement for EmotionalAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut ai = Self::default();
        input.read_u32_into::<LE>(&mut ai.state)?;
        Ok(ai)
    }
}

/// The resource AI module.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResourceAi {
    /// The number of resources the module tracks.
    pub num_resources: u32,
}

impl ReadableHeaderElement for ResourceAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let num_resources = input.read_u32::<LE>()?;
        Ok(Self { num_resources })
    }
}

impl WritableHeaderElement for ResourceAi {
    fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u32::<LE>(self.num_resources)?;
        Ok(())
    }
}

/// The strategy AI module: the AI's victory condition, targets and rules.
#[derive(Debug, Default, Clone)]
pub struct StrategyAi {
    /// The victory condition the AI pursues now.
    pub current_victory_condition: u32,
    /// The ID of the current target.
    pub target_id: u32,
    /// The ID of a second target.
    pub second_target_id: u32,
    /// The type of the second target.
    pub second_target_kind: u32,
    /// The current target point.
    pub target_point: Waypoint,
    /// A second target point.
    pub target_point_2: Waypoint,
    /// The target attribute, as the reader names it; not known yet.
    pub target_attribute: u32,
    /// The target number, as the reader names it; not known yet.
    pub target_number: u32,
    /// The time until the AI may change its victory condition.
    pub victory_condition_change_timeout: u32,
    /// The name of the AI's rule set.
    pub ruleset_name: Option<String>,
    /// The IDs of the rules for the victory condition.
    pub vc_ruleset: Vec<u32>,
    /// The IDs of the rules being executed.
    pub executing_rules: Vec<u32>,
    /// The IDs of the idle rules.
    pub idle_rules: Vec<u32>,
    /// The expert list ID. Stored from version 9.71.
    pub expert_list_id: Option<u32>,
}

impl ReadableHeaderElement for StrategyAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        Ok(Self {
            current_victory_condition: input.read_u32::<LE>()?,
            target_id: input.read_u32::<LE>()?,
            second_target_id: input.read_u32::<LE>()?,
            second_target_kind: input.read_u32::<LE>()?,
            target_point: Waypoint::read_from(input)?,
            target_point_2: Waypoint::read_from(input)?,
            target_attribute: input.read_u32::<LE>()?,
            target_number: input.read_u32::<LE>()?,
            victory_condition_change_timeout: input.read_u32::<LE>()?,
            ruleset_name: input.read_u32_length_prefixed_str()?,
            vc_ruleset: read_id_list(input)?,
            executing_rules: read_id_list(input)?,
            idle_rules: read_id_list(input)?,
            expert_list_id: if input.version() >= 9.71 {
                Some(input.read_u32::<LE>()?)
            } else {
                None
            },
        })
    }
}

/// The tactical AI module: the AI's units by role, its targets and its groups.
///
/// Reading it is not finished: the reader stops after `trade_carts`, so the later fields are
/// never filled in.
#[derive(Debug, Default, Clone)]
pub struct TacticalAi {
    /// The AI's civilian units.
    pub civilians: Vec<ObjectId>,
    /// The civilians used as explorers.
    pub civilian_explorers: Vec<ObjectId>,
    /// The AI's soldiers.
    pub soldiers: Vec<ObjectId>,
    /// The soldiers that are in no group.
    pub ungrouped_soldiers: Vec<ObjectId>,
    /// The AI's boats.
    pub boats: Vec<ObjectId>,
    /// The AI's war boats.
    pub war_boats: Vec<ObjectId>,
    /// The AI's fishing boats.
    pub fishing_boats: Vec<ObjectId>,
    /// The AI's trade boats.
    pub trade_boats: Vec<ObjectId>,
    /// The AI's transport boats.
    pub transport_boats: Vec<ObjectId>,
    /// The artifacts (such as relics) the AI knows of.
    pub artifacts: Vec<ObjectId>,
    /// The AI's trade carts.
    pub trade_carts: Vec<ObjectId>,
    /// The AI's strategic numbers. Not read yet.
    pub strategic_numbers: Vec<i32>,
    /// The players the AI attacks. Not read yet.
    pub players_to_attack: Vec<u32>,
    /// The players the AI defends. Not read yet.
    pub players_to_defend: Vec<u32>,
    /// The AI's working area. Not read yet.
    pub working_area: Vec<u32>,
    /// The units the AI gave tasks in this update. Not read yet.
    pub units_tasked_this_update: Vec<ObjectId>,
    /// How the AI's gatherers are spread over the four resources. Not read yet.
    pub gatherer_distribution: [u32; 4],
    /// How the AI wants its gatherers spread over the four resources. Not read yet.
    pub desired_gatherer_distribution: [u32; 4],
    /// The amount of each of the four resources the AI needs. Not read yet.
    pub resources_needed: [u32; 4],
    /// The AI's group ID. Not read yet.
    pub group_id: u32,
    /// The AI's unit groups. Not read yet.
    pub groups: Vec<()>,
}

impl ReadableHeaderElement for TacticalAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut ai = Self {
            civilians: read_id_list(input)?,
            civilian_explorers: read_id_list(input)?,
            ..Default::default()
        };

        let _num_gatherers = input.read_u32::<LE>()?;
        let _desired_num_gatherers = input.read_u32::<LE>()?;

        // FIXME: more stuff here

        ai.soldiers = read_id_list(input)?;
        ai.ungrouped_soldiers = read_id_list(input)?;
        ai.boats = read_id_list(input)?;
        ai.war_boats = read_id_list(input)?;
        ai.fishing_boats = read_id_list(input)?;
        ai.trade_boats = read_id_list(input)?;
        ai.transport_boats = read_id_list(input)?;
        ai.artifacts = read_id_list(input)?;
        ai.trade_carts = read_id_list(input)?;

        Err(crate::Error::NotRead("the rest of the strategy AI"))
    }
}
