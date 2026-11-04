//! A unit's AI state ([`UnitAi`]): its orders, notifications, history, retargets and paths.

use crate::element::{ReadableHeaderElement, read_counted};
use crate::errors::not_read;
use crate::fits;
use crate::reader::RecordingHeaderReader;
use crate::{ObjectId, PlayerId};
use byteorder::{LE, ReadBytesExt};
use genie_error::GenieResult;
use genie_support::{ReadSkipExt, read_opt_u32};
use std::io::Read;

/// An order in a unit AI's order queue.
#[allow(dead_code)]
#[derive(Debug, Default, Clone, Copy)]
pub struct UnitAiOrder {
    pub(super) issuer: u32,
    pub(super) order_kind: u32,
    pub(super) priority: u32,
    pub(super) target_id: ObjectId,
    pub(super) target_player: PlayerId,
    pub(super) target_location: (f32, f32, f32),
    pub(super) range: f32,
}

impl ReadableHeaderElement for UnitAiOrder {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            issuer: input.read_u32::<LE>()?,
            order_kind: input.read_u32::<LE>()?,
            priority: input.read_u32::<LE>()?,
            target_id: input.read_u32::<LE>()?.into(),
            target_player: fits(input.read_u32::<LE>()?)?,
            target_location: (
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
            ),
            range: input.read_f32::<LE>()?,
        })
    }
}

/// A notification one unit AI sends another.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnitAiNotification {
    /// The object that sent the notification.
    pub caller: u32,
    /// The object the notification is for.
    pub recipient: u32,
    /// The kind of notification.
    pub notification_kind: u32,
    /// The notification's three parameters.
    pub params: (u32, u32, u32),
}

impl ReadableHeaderElement for UnitAiNotification {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            caller: input.read_u32::<LE>()?,
            recipient: input.read_u32::<LE>()?,
            notification_kind: input.read_u32::<LE>()?,
            params: (
                input.read_u32::<LE>()?,
                input.read_u32::<LE>()?,
                input.read_u32::<LE>()?,
            ),
        })
    }
}

/// An entry of a unit AI's order history.
#[allow(dead_code)]
#[derive(Debug, Default, Clone, Copy)]
pub struct UnitAiOrderHistory {
    pub(super) order: u32,
    pub(super) action: u32,
    pub(super) time: u32,
    pub(super) position: (f32, f32, f32),
    pub(super) target_id: ObjectId,
    pub(super) target_attack_category: Option<u32>,
    pub(super) target_position: (f32, f32, f32),
}

impl ReadableHeaderElement for UnitAiOrderHistory {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Ok(Self {
            order: input.read_u32::<LE>()?,
            action: input.read_u32::<LE>()?,
            time: input.read_u32::<LE>()?,
            position: (
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
            ),
            target_id: input.read_u32::<LE>()?.into(),
            target_attack_category: if input.version() >= 10.50 {
                read_opt_u32(input)?
            } else {
                None
            },
            target_position: (
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
            ),
        })
    }
}

/// A target a unit AI keeps track of for retargeting, with a timeout.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnitAiRetargetEntry {
    /// The target object.
    pub target_id: ObjectId,
    /// When the entry times out.
    pub retarget_timeout: u32,
}

impl ReadableHeaderElement for UnitAiRetargetEntry {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let target_id = input.read_u32::<LE>()?.into();
        let retarget_timeout = input.read_u32::<LE>()?;
        Ok(Self {
            target_id,
            retarget_timeout,
        })
    }
}

/// A waypoint of a path.
#[derive(Debug, Default, Clone, Copy)]
pub struct Waypoint {
    /// The position (x, y, z).
    pub location: (f32, f32, f32),
    /// The facet (direction) towards the next waypoint.
    pub facet_to_next_waypoint: u8,
}

impl ReadableHeaderElement for Waypoint {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let waypoint = Self {
            location: (
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
            ),
            facet_to_next_waypoint: input.read_u8()?,
        };
        let _padding = input.read_u8()?;
        let _padding = input.read_u8()?;
        let _padding = input.read_u8()?;
        Ok(waypoint)
    }
}

/// A patrol path. Placeholder: reading one is not implemented yet and fails.
#[derive(Debug, Default, Clone, Copy)]
pub struct PatrolPath {}

impl ReadableHeaderElement for PatrolPath {
    fn read_from<R: Read>(_: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        Err(not_read("a patrol path"))
    }
}

/// A unit's AI state: orders, targets, timers and notifications.
#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub struct UnitAi {
    pub(super) mood: Option<u32>,
    pub(super) current_order: Option<u32>,
    pub(super) current_order_priority: Option<u32>,
    pub(super) current_action: Option<u32>,
    pub(super) current_target: Option<u32>,
    pub(super) current_target_kind: Option<u32>,
    pub(super) current_target_location: (f32, f32, f32),
    pub(super) desired_target_distance: f32,
    pub(super) last_action: Option<u32>,
    pub(super) last_order: Option<u32>,
    pub(super) last_target: Option<u32>,
    pub(super) last_target_kind: Option<u32>,
    pub(super) last_update_kind: Option<u32>,
    pub(super) idle_timer: u32,
    pub(super) idle_timeout: u32,
    pub(super) adjusted_idle_timeout: u32,
    pub(super) secondary_timer: u32,
    pub(super) lookaround_timer: u32,
    pub(super) lookaround_timeout: u32,
    pub(super) defend_target: Option<ObjectId>,
    pub(super) defense_buffer: f32,
    pub(super) last_world_position: Waypoint,
    pub(super) orders: Vec<UnitAiOrder>,
    pub(super) notifications: Vec<UnitAiNotification>,
    pub(super) attacking_units: Vec<ObjectId>,
    pub(super) stop_after_target_killed: bool,
    pub(super) state: u8,
    pub(super) state_position: (f32, f32),
    pub(super) time_since_enemy_sighting: u32,
    pub(super) alert_mode: u8,
    pub(super) alert_mode_object_id: Option<ObjectId>,
    pub(super) patrol_path: Option<PatrolPath>,
    pub(super) patrol_current_waypoint: u32,
    pub(super) order_history: Vec<UnitAiOrderHistory>,
    pub(super) last_retarget_time: u32,
    pub(super) randomized_retarget_timer: u32,
    pub(super) retarget_entries: Vec<UnitAiRetargetEntry>,
    pub(super) best_unit_to_attack: Option<u32>,
    pub(super) formation_kind: u8,
}

impl ReadableHeaderElement for UnitAi {
    // Reads the unit AI's fields in file order, section by section.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut ai = Self::read_current(input)?;
        ai.read_last_and_timers(input)?;
        ai.read_lists(input)?;
        ai.read_state_and_patrol(input)?;
        ai.read_later_versions(input)?;
        Ok(ai)
    }
}

impl UnitAi {
    /// The mood and the current order, action and target: the AI's first fields.
    fn read_current<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut ai = Self {
            mood: read_opt_u32(input)?,
            current_order: read_opt_u32(input)?,
            current_order_priority: read_opt_u32(input)?,
            current_action: read_opt_u32(input)?,
            current_target: read_opt_u32(input)?,
            current_target_kind: match input.read_u16::<LE>()? {
                0xFFFF => None,
                id => Some(id.into()),
            },
            ..Default::default()
        };
        input.skip(2)?;
        ai.current_target_location = (
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
            input.read_f32::<LE>()?,
        );
        ai.desired_target_distance = input.read_f32::<LE>()?;
        Ok(ai)
    }

    /// The last action, order and target, the timers, the defended target and the last
    /// position.
    fn read_last_and_timers<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        self.last_action = read_opt_u32(input)?;
        self.last_order = read_opt_u32(input)?;
        self.last_target = read_opt_u32(input)?;
        self.last_target_kind = read_opt_u32(input)?;
        self.last_update_kind = read_opt_u32(input)?;
        self.idle_timer = input.read_u32::<LE>()?;
        self.idle_timeout = input.read_u32::<LE>()?;
        self.adjusted_idle_timeout = input.read_u32::<LE>()?;
        self.secondary_timer = input.read_u32::<LE>()?;
        self.lookaround_timer = input.read_u32::<LE>()?;
        self.lookaround_timeout = input.read_u32::<LE>()?;
        self.defend_target = read_opt_u32(input)?;
        self.defense_buffer = input.read_f32::<LE>()?;
        self.last_world_position = Waypoint::read_from(input)?;
        Ok(())
    }

    /// The queued orders, the notifications and the units attacking this one, each behind
    /// its count.
    fn read_lists<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> GenieResult<()> {
        self.orders = read_counted(input)?;
        self.notifications = read_counted(input)?;
        let num_units = input.read_u32::<LE>()?;
        self.attacking_units = (0..num_units)
            .map(|_| Ok(input.read_u32::<LE>()?.into()))
            .collect::<GenieResult<_>>()?;
        Ok(())
    }

    /// The stop flag, the state and its position, the alert mode and the patrol path.
    fn read_state_and_patrol<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        self.stop_after_target_killed = input.read_u8()? != 0;
        self.state = input.read_u8()?;
        self.state_position = (input.read_f32::<LE>()?, input.read_f32::<LE>()?);
        self.time_since_enemy_sighting = input.read_u32::<LE>()?;
        self.alert_mode = input.read_u8()?;
        self.alert_mode_object_id = read_opt_u32(input)?;
        let has_path = input.read_u32::<LE>()? != 0;
        self.patrol_path = if has_path {
            Some(PatrolPath::read_from(input)?)
        } else {
            None
        };
        self.patrol_current_waypoint = input.read_u32::<LE>()?;
        Ok(())
    }

    /// The fields later versions added: the order history (10.48), the retargeting (10.50,
    /// 11.04, 11.05), the best unit to attack (11.14) and the formation (11.44).
    fn read_later_versions<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> GenieResult<()> {
        if input.version() >= 10.48 {
            self.order_history = read_counted(input)?;
        }
        if input.version() >= 10.50 {
            self.last_retarget_time = input.read_u32::<LE>()?;
        }
        if input.version() >= 11.04 {
            self.randomized_retarget_timer = input.read_u32::<LE>()?;
        }
        if input.version() >= 11.05 {
            self.retarget_entries = read_counted(input)?;
        }
        if input.version() >= 11.14 {
            self.best_unit_to_attack = read_opt_u32(input)?;
        }
        if input.version() >= 11.44 {
            self.formation_kind = input.read_u8()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
