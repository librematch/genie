//! The parts of a saved master: its combat, moving and animated records, wherever its class's
//! record chain holds them.

use genie_save::generated::world::{
    Master, RgeMasterAnimatedObject, RgeMasterCombatObject, RgeMasterMovingObject,
};

/// The animated part of a player's master, which holds its speed (`mMaxSpeed`, master `+0x740`):
/// an animated or doppelganger master's own record (both `RGE_Master_Animated_Object`), else the
/// one its moving, action, combat or missile record holds.
#[must_use]
pub fn animated_master(master: &Master) -> Option<&RgeMasterAnimatedObject> {
    let combat = master.tribe_master_combat_object.as_deref().or_else(|| {
        master
            .tribe_master_building_object
            .as_deref()
            .map(|tribe_master_building_object| {
                &tribe_master_building_object.tribe_master_combat_object
            })
    });
    if let Some(tribe_master_combat_object) = combat {
        let moving = &tribe_master_combat_object
            .rge_master_combat_object
            .rge_master_action_object
            .rge_master_moving_object;
        return Some(&moving.rge_master_animated_object);
    }
    let rge_master = master.rge_master.as_deref()?;
    None.or(rge_master.rge_master_animated_object.as_deref())
        .or(rge_master.rge_master_doppleganger_object.as_deref())
        .or_else(|| {
            rge_master
                .rge_master_moving_object
                .as_deref()
                .map(|rge_master_moving_object| {
                    &rge_master_moving_object.rge_master_animated_object
                })
        })
        .or_else(|| {
            rge_master
                .rge_master_action_object
                .as_deref()
                .map(|rge_master_action_object| {
                    &rge_master_action_object
                        .rge_master_moving_object
                        .rge_master_animated_object
                })
        })
        .or_else(|| {
            rge_master
                .rge_master_combat_object
                .as_deref()
                .map(|rge_master_combat_object| {
                    &rge_master_combat_object
                        .rge_master_action_object
                        .rge_master_moving_object
                        .rge_master_animated_object
                })
        })
        .or_else(|| {
            rge_master
                .rge_master_missile_object
                .as_deref()
                .map(|rge_master_missile_object| {
                    &rge_master_missile_object
                        .rge_master_combat_object
                        .rge_master_action_object
                        .rge_master_moving_object
                        .rge_master_animated_object
                })
        })
}

/// The combat part of a saved master, when its class has one.
#[must_use]
pub fn combat_master(master: &Master) -> Option<&RgeMasterCombatObject> {
    None.or_else(|| {
        master
            .tribe_master_combat_object
            .as_deref()
            .map(|tribe_master_combat_object| &tribe_master_combat_object.rge_master_combat_object)
    })
    .or_else(|| {
        master
            .tribe_master_building_object
            .as_deref()
            .map(|tribe_master_building_object| {
                &tribe_master_building_object
                    .tribe_master_combat_object
                    .rge_master_combat_object
            })
    })
    .or_else(|| {
        master
            .rge_master
            .as_deref()
            .and_then(|rge_master| rge_master.rge_master_combat_object.as_deref())
    })
    .or_else(|| {
        master.rge_master.as_deref().and_then(|rge_master| {
            rge_master
                .rge_master_missile_object
                .as_deref()
                .map(|rge_master_missile_object| {
                    &rge_master_missile_object.rge_master_combat_object
                })
        })
    })
}

/// The moving part of a saved master, when its class has one.
#[must_use]
pub fn moving_master(master: &Master) -> Option<&RgeMasterMovingObject> {
    let combat = master.tribe_master_combat_object.as_deref().or_else(|| {
        master
            .tribe_master_building_object
            .as_deref()
            .map(|tribe_master_building_object| {
                &tribe_master_building_object.tribe_master_combat_object
            })
    });
    if let Some(tribe_master_combat_object) = combat {
        return Some(
            &tribe_master_combat_object
                .rge_master_combat_object
                .rge_master_action_object
                .rge_master_moving_object,
        );
    }
    let rge_master = master.rge_master.as_deref()?;
    None.or(rge_master.rge_master_moving_object.as_deref())
        .or_else(|| {
            rge_master
                .rge_master_action_object
                .as_deref()
                .map(|rge_master_action_object| &rge_master_action_object.rge_master_moving_object)
        })
        .or_else(|| {
            rge_master
                .rge_master_combat_object
                .as_deref()
                .map(|rge_master_combat_object| {
                    &rge_master_combat_object
                        .rge_master_action_object
                        .rge_master_moving_object
                })
        })
        .or_else(|| {
            rge_master
                .rge_master_missile_object
                .as_deref()
                .map(|rge_master_missile_object| {
                    &rge_master_missile_object
                        .rge_master_combat_object
                        .rge_master_action_object
                        .rge_master_moving_object
                })
        })
}
