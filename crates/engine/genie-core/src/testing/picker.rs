//! The attack picker of a scene's unit (FD-043-D47): [`WorldPicker`] over a [`World`] as the
//! unit AI's view builds it for its unit, the world's spatial index lent beside it, as
//! `bestUnitToAttack`'s field holds both.

use crate::map::spatial::Spatial;
use crate::movement::SpeedReads;
use crate::objects::ObjectId;
use crate::objects::masters::MasterTables;
use crate::players::PlayerId;
use crate::unit_ai::world_view::{WorldPicker, owner_view};
use crate::world::World;
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::PlayerSlotsExt;

/// Runs `work` on the picker of object `me` of player `player` over `world` and on `world`'s
/// spatial index: its unit AI's seeker, the attacker its unit, master, mover and seer make, its
/// owner's type, attributes, diplomacy and AI settings, its master's terrain restriction.
///
/// # Panics
///
/// An object `world` does not hold in `player`'s list, or one without a unit AI or a master.
pub(crate) fn with_picker<R>(
    world: &mut World,
    (player, me): (PlayerId, ObjectId),
    work: impl FnOnce(&mut WorldPicker<'_>, &mut Spatial) -> R,
) -> R {
    let World {
        players,
        index,
        model,
        parts,
        groups,
        spatial,
        settings,
        world_time,
        mode_680,
        ..
    } = world;
    let owner = players.player(player).expect("the unit's player");
    let unit = owner
        .objects
        .iter()
        .find(|unit| unit.id == me)
        .expect("the unit in its player's list");
    let every = MasterTables::new(&model.masters, &parts.own_master);
    let master = unit.master(every, player).expect("the unit's master");
    let ai = parts.unit_ai.get(me).expect("the unit's AI");
    let actions = parts.actions.of(me);
    let capture = spatial.capture_of(me);
    let attacker = unit.attacker_of(
        actions,
        ai.ai_kind,
        Some(master),
        (player, owner.kind),
        settings.game_mode,
        (
            capture,
            parts.combat_of(me).firepower,
            SpeedReads::of(players, index, parts, groups, model)
                .top_speed(player, unit)
                .expect("the unit's top speed"),
        ),
    );
    let others = owner_view(players, index, player, None).expect("the owner's view");
    let mut picker = WorldPicker {
        attacker: &attacker,
        seeker: Some(ai.seeker_with(None)),
        me,
        ai_target: ai.current_target,
        player,
        others,
        foreign: others.players(),
        every,
        parts,
        speeds: SpeedReads::of(players, index, parts, groups, model),
        by_id: ObjectsRef::new(
            players,
            index,
            &model.masters,
            parts,
            &model.roles,
            &model.tuning,
        ),
        restriction: master.terrain_restriction,
        unit,
        actions,
        attributes: &owner.attributes,
        relations: &owner.relations,
        ai_settings: owner.ai_settings,
        now: *world_time,
        mode_680: *mode_680,
    };
    work(&mut picker, spatial)
}
