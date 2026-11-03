//! A death's parts: a farm's bonus, the visible resources forgotten, the combat object's death
//! and refund, a built building's death and storage, and the military and civilian counts.

use crate::GenieError;
use crate::construction::MainBuildingId;
use crate::construction::build::ClearLook;
use crate::map::Distance;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::layers::Building;
use crate::objects::layers::Combat;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::{OwnerMasters, cost_entries};
use crate::objects::unit::Unit;
use crate::pathing::Pathing;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::layer::generated::VisibilityTuning;
use crate::world::{BuildingDeath, RESTORED_FOUNDATION};

/// The owner's attributes 237, 238 and 239: a built farm's bonus ([`farm_bonus`]);
/// −1 for those the player has not got.
pub(crate) fn farm_bonus(attributes: &[Amount]) -> FarmBonus {
    let at = |i: usize| attributes.get(i).copied().unwrap_or(Amount::new(-1.0));
    FarmBonus {
        amount: at(237),
        resource: at(238),
        near: at(239),
    }
}

/// The owner's attributes 237, 238 and 239 (`player+0x768`..`+0x778`): a farm's bonus when built,
/// its amount, the resource it goes to and a master near which it is given
/// ([`Completion::first_part`](crate::construction::build::Completion::first_part)); each −1 when
/// the player has not got it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FarmBonus {
    /// Attribute 237: the amount.
    pub amount: Amount,
    /// Attribute 238: the resource (an attribute id as the player's value holds it).
    pub resource: Amount,
    /// Attribute 239: the master near which it is given (an id as the value holds it).
    pub near: Amount,
}

/// `FUN_1411980b0`'s visible resources, from `set_state` leaving a state below 4 for one above 6
/// (`FUN_14119d710`) and from the object's deletion: an object of id 0 and up whose master
/// gathers by itself (`+0x681`) leaves the lists of the players that explored its tile
/// ([`Visibility::forget_resource`]); the world's state (`+0x88`) is 0 in a game. The deletion's
/// call is the destructors' (`FUN_14116b5e0`). The routine's other parts are the callers': a
/// master of `create_doppelganger_on_death` 2 leaves its owner's watched list
/// (`FUN_141091310`).
///
/// [`Visibility::forget_resource`]: crate::vision::visibility::Visibility::forget_resource
pub(crate) fn forget_resource(
    spatial: &mut Spatial,
    master: Option<&Master>,
    id: ObjectId,
    position: Position,
) -> Result<(), GenieError> {
    // a recyclable object (a negative id) is in no resource list
    if id.is_recyclable() {
        return Ok(());
    }
    if let (Some(m), Some(v)) = (
        master.filter(|m| m.auto_gather.is_on()),
        spatial.sight.as_mut(),
    ) {
        v.forget_resource(id, position, m.gather_group)?;
    }
    Ok(())
}

/// The combat object's death part (`FUN_14121cc40`'s block for a state of 3 or more from below
/// 3), of a unit of type 70 and of a building: its pasture lets it go (`FUN_141189ca0` on its
/// `+0x794`), its hit points go to 0, and unless its record went already (`mROOAlreadyCalled`,
/// `+0x686`): a unit's, or a building's not desolid whose master is tall (`FUN_1411c3f70`:
/// COMISS, a NaN height is not), mesh shape and obstruction record go (`FUN_141055b40`); one of
/// type 70 and up has no height after (its own master copy, `FUN_141186d10`: no longer tall); a
/// building on a tile that is no annex lets its tiles go (`FUN_140f65580`); then `+0x686` is
/// set. Not modelled: a unit in a pasture, the artifact class (1, `FUN_141218020`), class 43
/// (Gaia's object 0x11d at its place) and class 13's units inside. Not held: the damage sprites
/// (`FUN_1413b9e20`) and the hero attachments (`FUN_141201930`).
///
/// # Errors
///
/// What it does not model.
pub(crate) fn combat_dies(
    u: &mut Unit,
    layer: &Building,
    master: &Master,
    spatial: &mut Spatial,
    pathing: &mut Pathing,
    roles: &Roles,
    visibility_tuning: VisibilityTuning,
) -> Result<(), GenieError> {
    if u.pasture.is_some() || u.my_pasture.is_some() {
        return Err(GenieError::unsupported(
            "a dying unit in a pasture (FUN_141189ca0)",
        ));
    }
    if roles
        .of_class(master.class)
        .intersects(ClassRoles::ARTIFACT | ClassRoles::SIEGE | ClassRoles::MONK_WITH_RELIC)
    {
        return Err(GenieError::unsupported("a dying unit of class 1, 13 or 43"));
    }
    u.life.hp = HitPoints::ZERO;
    if u.roo_called {
        return Ok(());
    }
    let desolid = layer.construction.is_some_and(|c| c.desolid);
    if master.kind < MasterKind::BUILDING || (!desolid && Distance::ZERO < master.height) {
        pathing.removed(u.id);
        spatial.drop_record(u.id);
    }
    if MasterKind::COMBAT <= master.kind {
        spatial.set_tall(u.id, false);
    }
    if master.kind == MasterKind::BUILDING
        && layer.annex_of.is_none()
        && u.life.inside.is_none()
        && let Some(v) = spatial.sight.as_mut()
    {
        v.unlink_building(u.position(), master.radius, visibility_tuning)?;
    }
    u.roo_called = true;
    Ok(())
}

/// What the combat object's `set_state` to 3 from state 2 (`FUN_14121cc40`) takes from its
/// owner's attributes: its paid cost (the cost entries of a resource flagged paid, each as a
/// float, summed in order) from 98 and 164 for a unit; from 98 and 165 for a building (98 alone
/// for the castle, 82, and the wonder, 276), nothing for an annex (`+0xbb8`). Each attribute is
/// held at 0 or more after (slot 0xf0 with 0), the caller's.
#[must_use]
pub(crate) fn refund_of(
    master: &Master,
    main: Option<MainBuildingId>,
    roles: &Roles,
    slots: Slots,
) -> Vec<(AttributeId, Amount)> {
    let mut cost = 0.0f32;
    for (_, amount, paid) in cost_entries(&master.creatable_part().cost) {
        if paid {
            cost += amount.as_f32();
        }
    }
    if master.kind != MasterKind::BUILDING {
        return vec![
            (slots.attribute.total_value, Amount::new(cost)),
            (slots.attribute.units_value, Amount::new(cost)),
        ];
    }
    if main.is_some() {
        return Vec::new();
    }
    // The masters whose death lowers a count of their owner's (`FUN_1411dbb70`, raw `0x6104`,
    // `0x61b0`, `0x60ee`): the monastery's ([`MONASTERY`]) in attribute 52, the wonder's in 42,
    // the castle's in 134.
    if roles
        .of_master(master.id)
        .intersects(MasterRoles::CASTLE | MasterRoles::WONDER)
    {
        return vec![(slots.attribute.total_value, Amount::new(cost))];
    }
    vec![
        (slots.attribute.total_value, Amount::new(cost)),
        (slots.attribute.buildings_value, Amount::new(cost)),
    ]
}

/// What a built building's death (`FUN_1411dbb70` to state 3 from 2) does before the combat
/// object's part, for its caller to apply: the attributes its storage gave its owner
/// ([`storage_leaves`]), the place whose tiles it lets go (`FUN_140f65580`, a building on a tile
/// that is no annex), the count it lowers and its master's technology ([`BuildingDeath`]); its
/// owner's watched list lets it go (`FUN_141091310`, the caller's).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuiltDeath {
    pub(crate) adds: Vec<(AttributeId, Amount)>,
    pub(crate) unlink: Option<Position>,
    pub(crate) building: BuildingDeath,
}

/// [`BuiltDeath`] of the building `u` of `master` with its combat layer `combat` (its store)
/// and its main building `annex_of` (an annex's): the monastery (104), the wonder (276) and the
/// castle (82) count one less in attributes 52, 42 and 134 (slot 0xf8 with −1, then slot 0xf0
/// with 0 below 0); its master's technology (`+0x98c`) goes when no other building of the master
/// stands ([`World::withdraw_tech`]). A foundation terrain stays. Not modelled: a computer
/// player's (`FUN_1411e7ca0`), one with a pasture or its workers (`+0xcb8`), units inside, the
/// terrain 1 a foundation terrain of 28 lays at the death (`FUN_1411ce250`). Not held: its
/// linked objects (`+0xb78`, none held), `+0xca4` and the snow decal (`FUN_1413b9e20`).
///
/// # Errors
///
/// What it does not model.
///
/// [`World::withdraw_tech`]: crate::world::World::withdraw_tech
pub(crate) fn built_death(
    (u, combat): (&mut Unit, &mut Combat),
    annex_of: Option<MainBuildingId>,
    master: &Master,
    player_kind: PlayerKind,
    roles: &Roles,
    slots: Slots,
) -> Result<BuiltDeath, GenieError> {
    let building = master.building.ok_or(GenieError::unsupported(
        "a dying building of a master without its building part",
    ))?;
    if player_kind.is_computer()
        || u.pasture.is_some()
        || u.my_pasture.is_some()
        || !u.garrison.is_empty()
    {
        return Err(GenieError::unsupported(
            "a dying building of a computer player, with a pasture or units inside",
        ));
    }
    let adds = storage_leaves((u, combat), master, roles, slots);
    if building.foundation_terrain == Some(RESTORED_FOUNDATION) {
        return Err(GenieError::unsupported(
            "a dying building of foundation terrain 28 (its terrain 1, FUN_1411ce250)",
        ));
    }
    let unlink = (annex_of.is_none() && u.life.inside.is_none()).then_some(u.position());
    let count = {
        let held = roles.of_master(master.id);
        if held.contains(MasterRoles::MONASTERY) {
            Some(slots.attribute.monasteries)
        } else if held.contains(MasterRoles::WONDER) {
            Some(slots.attribute.wonders)
        } else if held.contains(MasterRoles::CASTLE) {
            Some(slots.attribute.castles)
        } else {
            None
        }
    };
    let tech = building.tech.map(|t| (master.id, t));
    Ok(BuiltDeath {
        adds,
        unlink,
        building: BuildingDeath { count, tech },
    })
}

/// What a built building's storage gave its owner leaves (`FUN_1411e63f0`, then
/// `FUN_1411e6580`): each entry of mode 4 with an attribute and an amount above 0 (the
/// population headroom of a house, a town centre or a castle; slot 0xf8 with the amount
/// negated), then the population it holds itself (`+0x308` 4: the amount leaves, and it holds
/// nothing after, `FUN_14119d310` with −1 and 0); then each entry of mode 16 leaves its own store
/// ([`Combat::store`](crate::objects::layers::Combat::store)) when the store holds its attribute.
pub(crate) fn storage_leaves(
    (u, combat): (&mut Unit, &mut Combat),
    master: &Master,
    roles: &Roles,
    slots: Slots,
) -> Vec<(AttributeId, Amount)> {
    let mut adds = Vec::new();
    for &(kind, amount, flag) in &master.storage {
        if let Some(kind) = kind
            && Amount::ZERO < amount
            && flag == 4
        {
            adds.push((kind, -amount));
        }
    }
    if roles
        .of_some_attribute(u.life.attribute_kind)
        .contains(AttributeRoles::HEADROOM)
    {
        adds.push((slots.attribute.headroom, -u.life.amount));
        u.life.attribute_kind = None;
        u.life.amount = Amount::ZERO;
    }
    // FUN_1411e6580: each entry of mode 16 leaves its own store, when the store holds it
    for &(kind, amount, flag) in &master.storage {
        if let Some(kind) = kind
            && Amount::ZERO < amount
            && flag & 0x10 != 0
            && let Some(e) = combat.store.iter_mut().find(|e| e.0 == kind)
        {
            e.1 -= amount;
        }
    }
    adds
}

/// What the footprint clearing reads of the object `u` ([`crate::construction::build::ClearLook`]) with its
/// owner's `masters`.
///
/// # Errors
///
/// An object of a master its owner lacks.
pub(crate) fn clear_look_of(
    u: &Unit,
    layer: &Building,
    masters: OwnerMasters<'_>,
) -> Result<ClearLook, GenieError> {
    let m = u.master_in(masters)?;
    Ok(ClearLook {
        hp: u.life.hp,
        height: m.height,
        roo_called: u.roo_called,
        kind: m.kind,
        state: u.life.state,
        desolid: layer.construction.is_some_and(|c| c.desolid),
    })
}

/// `FUN_141226060`: a combat object (type 70 and up) of `master` counts `by` (1 or -1) in its
/// player's `attributes` by its class: a military class (0, 6, 12, 13, 18, 22 to 24, 26, 28, 35,
/// 36, 43 to 47, 50, 51, 53 to 57) in attribute 40, masters 1663 and 1697 also in 235; a
/// villager (class 4) in 38 when its master's task swap group (`+0x7b8`) is 1 or 2, and in 37
/// as the classes 2, 19, 20 and 21 do. The attributes take it as they are (no player slot).
/// Its callers: the combat setup (`FUN_14121df60`, 1), the destructors (`FUN_141200450`,
/// `FUN_1411ec020`, -1), the ownership change (`FUN_14120f800`, -1 then 1 for the new owner) and
/// the master change (`FUN_141186580` through `FUN_141209d90`, -1 then 1 with the new master).
pub(crate) fn count_class(attributes: &mut [Amount], master: &Master, by: f32, roles: &Roles) {
    let mut add = |i: usize| {
        if let Some(v) = attributes.get_mut(i) {
            *v += Amount::new(by);
        }
    };
    if roles
        .of_class(master.class)
        .contains(ClassRoles::MILITARY_COUNT)
    {
        add(40);
        if roles
            .of_master(master.id)
            .contains(MasterRoles::MILITARY_235)
        {
            add(235);
        }
        return;
    }
    if roles.of_class(master.class).contains(ClassRoles::VILLAGER)
        && matches!(
            master.action_part().task_by_group,
            TaskGroup::MALE_VILLAGERS | TaskGroup::FEMALE_VILLAGERS
        )
    {
        add(38);
    }
    if roles.of_class(master.class).contains(ClassRoles::VILLAGER)
        || roles
            .of_class(master.class)
            .contains(ClassRoles::CIVILIAN_COUNT)
    {
        add(37);
    }
}
