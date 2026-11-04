//! The per-player 11-word state block that recorded games checkpoint every N syncs.
//!
//! The engine builds it in `WorldPlayerBase::create_checksum` (158041: `0x1416607a0`); the word
//! table and its derivation are in the workspace's `docs/AoE2DE_s_exe_SIMULATION.md`. Every sum is a
//! wrapping 32-bit add, and every float-to-int step is the x86 `CVTTSS2SI` instruction.

use crate::map::Position;
use crate::math::conversions::cvttss2si;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::world::Millis;

/// What the block reads from one object of a player's live list.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CheckpointObject {
    /// The object's id.
    pub id: ObjectId,
    /// Its master.
    pub main_id: MasterId,
    /// Its state.
    pub state: ObjectState,
    /// What it holds.
    pub amount_held: Amount,
    /// Its place.
    pub position: Position,
    /// What word 9 reads of its actions, none without an action.
    pub top_action: Option<ActionTerm>,
    /// Its unit AI's retarget timer, None without a unit AI.
    pub retarget_timer: Option<Millis>,
}

/// The type (virtual slot 166) and state (virtual slot 86) of an object's top action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TopAction {
    /// The type (virtual slot 166).
    pub kind: ActionKind,
    /// The state (virtual slot 86).
    pub state: ActionState,
}

/// What word 9 reads of an object's actions: its top action's type (virtual slot 166), none
/// for −1 (a building that makes without a top action), and a state (virtual slot 86, a
/// building's production action's first).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionTerm {
    /// The type, none for −1.
    pub kind: Option<ActionKind>,
    /// The state.
    pub state: ActionState,
}

impl From<TopAction> for ActionTerm {
    fn from(top: TopAction) -> Self {
        Self {
            kind: Some(top.kind),
            state: top.state,
        }
    }
}

/// What the block reads from a player.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerState {
    /// The player's id.
    pub id: u32,
    /// Food, wood, stone and gold.
    pub resources: [Amount; 4],
    /// How many objects sleep.
    pub sleeping_count: u32,
    /// The live objects, in their list's order.
    pub live: Vec<CheckpointObject>,
}

/// The player's 11-word block, in the order the recording stores it.
#[must_use]
pub fn player_block(player: &PlayerState) -> [u32; 11] {
    let mut words = [0u32; 11];
    words[0] = player.sleeping_count;
    words[1] = player.resources.iter().fold(0u32, |acc, amount| {
        acc.wrapping_add(cvttss2si(amount.get()) as u32)
    });
    for checkpoint_object in &player.live {
        words[2] = words[2].wrapping_add(i32::from(checkpoint_object.main_id.get()) as u32);
        words[3] = words[3].wrapping_add(u32::from(checkpoint_object.state.get()));
        words[4] = words[4].wrapping_add(cvttss2si(checkpoint_object.amount_held.get()) as u32);
        for axis in checkpoint_object.position.get() {
            words[7] = words[7].wrapping_add(cvttss2si(axis * 100.0) as u32);
        }
        words[9] = words[9].wrapping_add(action_checksum(checkpoint_object));
        words[10] = words[10].wrapping_add(checkpoint_object.id.get() as u32);
    }
    // w[5] (ObjectWaypoints) stays 0: the engine zeroes it and never writes it.
    words[6] = player.live.len() as u32;
    words[8] = player.id;
    words
}

/// Word 9's term for one object: action type + action state byte + unit-AI retarget timer. An
/// object without an action list reports type -1 and state 0.
fn action_checksum(checkpoint_object: &CheckpointObject) -> u32 {
    let (kind, state) = checkpoint_object.top_action.map_or((-1, 0), |action_term| {
        (ActionKind::field(action_term.kind), action_term.state.get())
    });
    (i32::from(kind) as u32)
        .wrapping_add(u32::from(state))
        .wrapping_add(checkpoint_object.retarget_timer.map_or(0, Millis::get))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::gathering::gather::GATHER;
    #[cfg(test)]
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;

    fn object() -> CheckpointObject {
        CheckpointObject {
            id: object_id(1),
            main_id: master_id(4),
            state: ObjectState::Alive,
            ..CheckpointObject::default()
        }
    }

    fn player(live: Vec<CheckpointObject>) -> PlayerState {
        PlayerState {
            id: 3,
            live,
            ..PlayerState::default()
        }
    }

    #[test]
    fn empty_player_carries_sleeping_count_resources_and_id() {
        let player_state = PlayerState {
            id: 7,
            resources: [
                Amount::new(200.0),
                Amount::new(200.0),
                Amount::new(200.0),
                Amount::new(100.0),
            ],
            sleeping_count: 5,
            live: vec![],
        };
        assert_eq!(
            player_block(&player_state),
            [5, 700, 0, 0, 0, 0, 0, 0, 7, 0, 0]
        );
    }

    #[test]
    fn resources_are_truncated_one_by_one() {
        let player_state = PlayerState {
            resources: [Amount::new(1.9); 4],
            ..PlayerState::default()
        };
        assert_eq!(player_block(&player_state)[1], 4);
    }

    #[test]
    fn main_ids_add_as_ints() {
        let player_state = player(vec![
            CheckpointObject {
                main_id: master_id(0x7fff),
                ..object()
            },
            CheckpointObject {
                main_id: master_id(3),
                ..object()
            },
        ]);
        assert_eq!(player_block(&player_state)[2], 0x8002);
    }

    #[test]
    fn positions_are_scaled_in_f32_before_truncation() {
        // f32: 0.57 * 100 rounds to 57.0; the same product in f64 is 56.99999928 and truncates to 56
        let player_state = player(vec![CheckpointObject {
            position: Position::new(0.57, 0.0, 0.0),
            ..object()
        }]);
        assert_eq!(player_block(&player_state)[7], 57);
    }

    #[test]
    fn object_without_action_or_unit_ai_contributes_minus_one_to_word_9() {
        let player_state = player(vec![object()]);
        assert_eq!(player_block(&player_state)[9], u32::MAX);
    }

    #[test]
    fn word_9_adds_action_kind_state_byte_and_retarget_timer() {
        let checkpoint_object = CheckpointObject {
            top_action: Some(ActionTerm {
                kind: Some(GATHER),
                state: ActionState::new(3),
            }),
            retarget_timer: Some(Millis::new(100)),
            ..object()
        };
        assert_eq!(player_block(&player(vec![checkpoint_object]))[9], 108);
    }

    #[test]
    fn sums_wrap_in_32_bits() {
        let player_state = player(vec![
            CheckpointObject {
                id: object_id(i32::MAX),
                ..object()
            },
            CheckpointObject {
                id: object_id(i32::MAX),
                ..object()
            },
        ]);
        assert_eq!(player_block(&player_state)[10], 0xFFFF_FFFE);
    }

    #[test]
    fn words_count_state_amount_live_list_and_zero_waypoints() {
        let player_state = player(vec![
            CheckpointObject {
                amount_held: Amount::new(150.7),
                ..object()
            },
            object(),
        ]);
        let player_block = player_block(&player_state);
        assert_eq!(
            (
                player_block[3],
                player_block[4],
                player_block[5],
                player_block[6]
            ),
            (4, 150, 0, 2)
        );
    }
}
