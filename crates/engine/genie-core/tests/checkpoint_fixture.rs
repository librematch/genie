//! Player 2 of the tier-1 recording MarvinBlue/426048731 against its first checkpoint.

use genie_core::map::Position;
use genie_core::objects::ObjectId;
use genie_core::objects::ObjectState;
use genie_core::objects::masters::MasterId;
use genie_core::players::Amount;
use genie_core::world::checkpoint::{CheckpointObject, PlayerState, player_block};

const FIXTURE: &str = include_str!("fixtures/marvinblue-426048731-p2-checkpoint1.txt");

fn load() -> (PlayerState, [Option<u32>; 11]) {
    let mut player = PlayerState::default();
    let mut expected = [None; 11];
    for line in FIXTURE
        .lines()
        .filter(|text| !text.starts_with('#') && !text.is_empty())
    {
        let mut fields = line.split_whitespace();
        let tag = fields.next().unwrap();
        let texts: Vec<&str> = fields.collect();
        let num = |index: usize| texts[index].parse::<f32>().unwrap();
        match tag {
            "player" => {
                player.id = texts[0].parse().unwrap();
                player.resources = [num(1), num(2), num(3), num(4)].map(Amount::new);
                player.sleeping_count = texts[5].parse().unwrap();
            }
            "object" => player.live.push(CheckpointObject {
                id: ObjectId::try_from(texts[0].parse::<i32>().unwrap()).unwrap(),
                main_id: MasterId::of_data(texts[1].parse().unwrap()).unwrap(),
                state: ObjectState::of_byte(texts[2].parse().unwrap()).unwrap(),
                amount_held: Amount::new(num(3)),
                position: Position::new(num(4), num(5), num(6)),
                ..CheckpointObject::default()
            }),
            "expected" => {
                for (index, text) in texts.iter().enumerate() {
                    expected[index] = text.parse().ok();
                }
            }
            other => panic!("unknown fixture line tag {other:?}"),
        }
    }
    (player, expected)
}

#[test]
fn header_state_reproduces_checkpoint_1_for_player_2() {
    let (player, expected) = load();
    let got = player_block(&player);
    for (index, want) in expected.iter().enumerate() {
        if let Some(want) = want {
            assert_eq!(got[index], *want, "word {index}");
        }
    }
}
