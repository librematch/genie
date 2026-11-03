use super::*;
use crate::test_ids::object_id;
use crate::test_ids::{VILLAGER_FEMALE, VILLAGER_MALE};
use genie_core::objects::ActionState;
use genie_core::objects::HitPoints;
use genie_core::objects::actions::ActionList;
use genie_core::objects::layers::Action;
use genie_core::objects::masters::MasterId;
use genie_core::ownership::artifact::Artifact;
use genie_core::ownership::artifact::ArtifactState;
use genie_core::production::QueueButton;
use genie_core::production::{MakeObj, MakeObjState, Making, Production};
use genie_core::world::Seconds;
use genie_core::world::WholeSeconds;
use genie_core::world::checkpoint::ActionTerm;
use genie_core::world::parts::Object;

/// Word 9 reads a building's production action's state over its top action's (records
/// MarvinBlue/426048706's town centre 7150 after update 7499: its Attack in state 7 and its
/// make action in state 6 count 9 + 6); its type stays the top action's, −1 without one.
#[test]
fn building_production_state_comes_before_its_top_action_s() {
    let artifact = Artifact {
        state: ArtifactState::Wait,
        timer: Seconds::ZERO,
        gaia: false,
        sub: None,
        goal: None,
        target: None,
        moving: None,
    };
    let making = Production {
        action: Some(Making::Unit(MakeObj {
            state: MakeObjState::Making,
            paid: true,
            master: Some(VILLAGER_MALE),
            progress: Seconds::new(0.5),
            unique_id: -1,
            button: QueueButton::of_data(0),
            train_time: WholeSeconds::of_data(25),
        })),
        ..Production::default()
    };
    let kind = ARTIFACT_KIND;
    let top = Action {
        list: ActionList::default().with_artifact(Some(artifact)),
        ..Default::default()
    };
    assert_eq!(
        checkpoint_action(&top, Some(&making)),
        Some(ActionTerm {
            kind: Some(kind),
            state: ActionState::new(6)
        })
    );
    assert_eq!(
        checkpoint_action(&top, None),
        Some(ActionTerm {
            kind: Some(kind),
            state: ActionState::new(10)
        })
    );
    assert_eq!(
        checkpoint_action(&Action::default(), Some(&making)),
        Some(ActionTerm {
            kind: None,
            state: ActionState::new(6)
        })
    );
    assert_eq!(checkpoint_action(&Action::default(), None), None);
}

/// Each differing field of a matched object is one difference with the game's master; an
/// object in one list alone is one too; the lines are the differences' texts.
#[test]
fn differences_name_the_object_its_master_and_the_field() {
    let unit = |id, master, hp| {
        let mut u = Unit {
            id,
            ..Unit::default()
        };
        u.life.master_id = master;
        u.life.hp = hp;
        u
    };
    let slice = [
        unit(object_id(1), VILLAGER_MALE, HitPoints::new(25.0)),
        unit(object_id(2), VILLAGER_MALE, HitPoints::new(25.0)),
        unit(
            object_id(4),
            MasterId::try_from(4).unwrap(),
            HitPoints::new(1.0),
        ),
    ];
    let game = [
        unit(object_id(1), VILLAGER_MALE, HitPoints::new(25.0)),
        unit(object_id(2), VILLAGER_FEMALE, HitPoints::new(24.0)),
        unit(
            object_id(3),
            MasterId::try_from(59).unwrap(),
            HitPoints::new(1.0),
        ),
    ];
    let (slice, game) = (slice.map(Object::from), game.map(Object::from));
    let d = object_field_differences(&slice, &game);
    let fields: Vec<(i32, i16, &str)> = d
        .iter()
        .map(|d| (d.id.get(), d.master.get(), d.field.as_str()))
        .collect();
    assert_eq!(
        fields,
        [
            (2, 293, "life.hp"),
            (2, 293, "life.master_id"),
            (3, 59, ONLY_IN_THE_GAME),
            (4, 4, ONLY_IN_THE_SLICE)
        ]
    );
    assert_eq!(d[0].to_string(), "2: life.hp slice 25.0 game 24.0");
    assert_eq!(
        d[2].to_string(),
        "3: only in the game (master 59 state 2 hp 1.0 at Position([0.0, 0.0, 0.0]) action None)"
    );
    assert_eq!(
        object_differences(&slice, &game),
        d.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
}

/// Each error says why the recording could not be compared.
#[test]
fn each_error_says_why() {
    let texts = [
        (Error::NoCheckpoint, "the recording has no checkpoint"),
        (
            Error::NoSuchPlayer(9),
            "a checkpoint names player 9, not in the save",
        ),
        (
            Error::CommandBeforeFirstSync,
            "a command comes before the first sync",
        ),
        (Error::BadMeshDump, "the mesh dump has no mesh line"),
        (Error::BadRegistryDump, "the registry dump is out of shape"),
        (Error::BadTileDump, "the tile list dump is out of shape"),
        (
            Error::BadPoolDump,
            "the recycled pools dump is out of shape",
        ),
        (Error::NoChapter, "the recording has no chapter"),
        (Error::NoPlayer(3), "no player 3 in the save"),
    ];
    for (error, text) in texts {
        assert_eq!(error.to_string(), text);
    }
    let unmodelled = Error::Unmodelled(GenieError::unsupported("a thing"));
    assert!(unmodelled.to_string().starts_with("not modelled: "));
    let truncated = de::truncated(4);
    let text = truncated.to_string();
    let rec = Error::from(truncated);
    assert!(matches!(rec, Error::Rec(_)));
    assert_eq!(rec.to_string(), text);
}
