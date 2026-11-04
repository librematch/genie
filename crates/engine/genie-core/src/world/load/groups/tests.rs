use crate::groups::SlotState;
use crate::world::load::groups::formation;
use genie_save::generated::world as saved;

/// A formation slot's saved state reads as its state (3, in place); 4 names none and the
/// formation does not import.
#[test]
fn slot_takes_its_saved_state() {
    let saved = |state: i32| saved::Formation {
        slots: vec![saved::FormationEntry1 {
            slots_state: state,
            ..saved::FormationEntry1::default()
        }],
        ..saved::Formation::default()
    };
    let loaded = formation(&saved(3)).unwrap();
    assert_eq!(loaded.slots[0].state, SlotState::InPlace);
    assert!(formation(&saved(4)).is_err());
}
