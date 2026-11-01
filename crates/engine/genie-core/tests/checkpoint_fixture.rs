//! Player 2 of the tier-1 recording MarvinBlue/426048731 against its first checkpoint.

use genie_core::map::Position;
use genie_core::objects::ObjectId;
use genie_core::players::Amount;
use genie_core::world::checkpoint::{CheckpointObject, PlayerState, player_block};

const FIXTURE: &str = include_str!("fixtures/marvinblue-426048731-p2-checkpoint1.txt");

fn load() -> (PlayerState, [Option<u32>; 11]) {
    let mut player = PlayerState::default();
    let mut expected = [None; 11];
    for line in FIXTURE
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let mut f = line.split_whitespace();
        let tag = f.next().unwrap();
        let v: Vec<&str> = f.collect();
        let num = |i: usize| v[i].parse::<f32>().unwrap();
        match tag {
            "player" => {
                player.id = v[0].parse().unwrap();
                player.resources = [num(1), num(2), num(3), num(4)].map(Amount::new);
                player.sleeping_count = v[5].parse().unwrap();
            }
            "object" => player.live.push(CheckpointObject {
                id: ObjectId::try_from(v[0].parse::<i32>().unwrap()).unwrap(),
                main_id: genie_core::objects::masters::MasterId::of_data(v[1].parse().unwrap())
                    .unwrap(),
                state: genie_core::objects::ObjectState::of_byte(v[2].parse().unwrap()).unwrap(),
                amount_held: Amount::new(num(3)),
                position: Position::new(num(4), num(5), num(6)),
                ..CheckpointObject::default()
            }),
            "expected" => {
                for (k, w) in v.iter().enumerate() {
                    expected[k] = w.parse().ok();
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
    for (k, want) in expected.iter().enumerate() {
        if let Some(want) = want {
            assert_eq!(got[k], *want, "word {k}");
        }
    }
}
