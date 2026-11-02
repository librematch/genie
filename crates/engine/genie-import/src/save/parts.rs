//! The parts of a saved master: its combat, moving and animated records, wherever its class's
//! record chain holds them.

use genie_save::generated::world::{
    Master, RgeMasterAnimatedObject, RgeMasterCombatObject, RgeMasterMovingObject,
};

/// The animated part of a player's master, which holds its speed (`mMaxSpeed`, master `+0x740`).
#[must_use]
pub fn animated_master(m: &Master) -> Option<&RgeMasterAnimatedObject> {
    let combat = m.tribe_master_combat_object.as_deref().or_else(|| {
        m.tribe_master_building_object
            .as_deref()
            .map(|b| &b.tribe_master_combat_object)
    });
    if let Some(c) = combat {
        let moving = &c
            .rge_master_combat_object
            .rge_master_action_object
            .rge_master_moving_object;
        return Some(&moving.rge_master_animated_object);
    }
    let r = m.rge_master.as_deref()?;
    None.or(r.rge_master_animated_object.as_deref())
        .or_else(|| {
            r.rge_master_moving_object
                .as_deref()
                .map(|m| &m.rge_master_animated_object)
        })
        .or_else(|| {
            r.rge_master_action_object
                .as_deref()
                .map(|a| &a.rge_master_moving_object.rge_master_animated_object)
        })
        .or_else(|| {
            r.rge_master_combat_object.as_deref().map(|c| {
                &c.rge_master_action_object
                    .rge_master_moving_object
                    .rge_master_animated_object
            })
        })
        .or_else(|| {
            r.rge_master_missile_object.as_deref().map(|m| {
                &m.rge_master_combat_object
                    .rge_master_action_object
                    .rge_master_moving_object
                    .rge_master_animated_object
            })
        })
}

/// The combat part of a saved master, when its class has one.
#[must_use]
pub fn combat_master(m: &Master) -> Option<&RgeMasterCombatObject> {
    None.or_else(|| {
        m.tribe_master_combat_object
            .as_deref()
            .map(|c| &c.rge_master_combat_object)
    })
    .or_else(|| {
        m.tribe_master_building_object
            .as_deref()
            .map(|b| &b.tribe_master_combat_object.rge_master_combat_object)
    })
    .or_else(|| {
        m.rge_master
            .as_deref()
            .and_then(|r| r.rge_master_combat_object.as_deref())
    })
    .or_else(|| {
        m.rge_master.as_deref().and_then(|r| {
            r.rge_master_missile_object
                .as_deref()
                .map(|m| &m.rge_master_combat_object)
        })
    })
}

/// The moving part of a saved master, when its class has one.
#[must_use]
pub fn moving_master(m: &Master) -> Option<&RgeMasterMovingObject> {
    let combat = m.tribe_master_combat_object.as_deref().or_else(|| {
        m.tribe_master_building_object
            .as_deref()
            .map(|b| &b.tribe_master_combat_object)
    });
    if let Some(c) = combat {
        return Some(
            &c.rge_master_combat_object
                .rge_master_action_object
                .rge_master_moving_object,
        );
    }
    let r = m.rge_master.as_deref()?;
    None.or(r.rge_master_moving_object.as_deref())
        .or_else(|| {
            r.rge_master_action_object
                .as_deref()
                .map(|a| &a.rge_master_moving_object)
        })
        .or_else(|| {
            r.rge_master_combat_object
                .as_deref()
                .map(|c| &c.rge_master_action_object.rge_master_moving_object)
        })
        .or_else(|| {
            r.rge_master_missile_object.as_deref().map(|m| {
                &m.rge_master_combat_object
                    .rge_master_action_object
                    .rge_master_moving_object
            })
        })
}
