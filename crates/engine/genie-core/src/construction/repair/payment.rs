//! What a repair reaches and pays: a building's and a combat unit's slot 0x8c8 by the
//! repairer's player.

use crate::combat::damage::Heal;
use crate::combat::damage::heal_hp;
use crate::construction::MainBuildingId;
use crate::construction::repair::ONE;
use crate::math::fmath::below_or_unordered;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::production::CostCount;
use crate::production::Payment;
use crate::production::pay;
use crate::technology::Attributes;
use crate::world::NO_ATTRIBUTE;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::slots::CheckedIndex;
use crate::{GenieError, GenieResult};
use std::fmt::{Debug, Error, Formatter};

/// What a repair's slot 0x8c8 answers (the game's 0, 1 and 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RepairOutcome {
    /// 0: it healed, the target short of its most hit points.
    Healed,
    /// 1: the target holds its most hit points.
    Full,
    /// 2: it took nothing (the player cannot pay, or a building below 1 hit point).
    Unpaid,
}

/// The owner's attributes of a unit's and a building's repair cost (270, 271: `player+0x100`
/// at `+0x870` and `+0x878`).
pub const UNIT_REPAIR_COST: usize = 270;
/// The building repair cost attribute (271).
pub const BUILDING_REPAIR_COST: usize = 271;

/// One object a repair reaches (the target, an annex's main building, a main building's
/// annexes) as the repair reads and changes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Repaired {
    /// The object.
    pub(crate) id: ObjectId,
    /// Its master's type.
    pub(crate) kind: MasterKind,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// Its master.
    pub(crate) master: MasterId,
    /// Its master's hit points (`+0x530`).
    pub(crate) master_hp: MasterHitPoints,
    /// Its hit points (`+0x278`).
    pub(crate) hp: HitPoints,
    /// Its hit point powerup (`+0xa38`, `+0xa3c`).
    pub(crate) hp_powerup: HpPowerup,
    /// An annex's main building (`+0xbb8`).
    pub(crate) main: Option<MainBuildingId>,
    /// A main building's annexes (`+0xbc0`, its four slots in their order).
    pub(crate) annexes: Vec<ObjectId>,
    /// Its owner's attribute 270 ([`UNIT_REPAIR_COST`]): a unit's repair cost rate.
    pub(crate) unit_repair_cost: Amount,
    /// Its owner's attribute 271 ([`BUILDING_REPAIR_COST`]): a building's repair cost rate.
    pub(crate) building_repair_cost: Amount,
}

/// What a repair pays with: the repairer's player's masters and attributes (`FUN_14162fb60`
/// with the repairer's player).
pub struct Payer<'a> {
    /// The repairer's player's masters.
    pub(crate) masters: &'a Masters,
    /// The repairer's player's attributes.
    pub(crate) attributes: &'a mut dyn Attributes,
}

impl Debug for Payer<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), Error> {
        formatter.debug_struct("Payer").finish_non_exhaustive()
    }
}

/// The object of `set` with id `id`.
pub(super) fn find(set: &[Repaired], id: ObjectId) -> GenieResult<usize> {
    set.iter()
        .position(|repaired| repaired.id == id)
        .ok_or(GenieError::unreachable("a repair's object outside its set"))
}

/// Who asks a building's repair (slot 0x8c8's flag): its repairer (1), or its main building for
/// each of its annexes (0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RepairFrom {
    /// The repairer.
    Repairer,
    /// The annex's main building.
    MainBuilding,
}

/// A building's slot 0x8c8 (`FUN_1411d37d0`) by `amount` for the repairer (`payer`): a trebuchet's
/// class repairs as a unit ([`repair_unit`]); one below 1 hit point (`COMISS`: a NaN is) takes
/// nothing ([`RepairOutcome::Unpaid`]). From its repairer ([`RepairFrom::Repairer`]) an annex's
/// repair is its main building's. Else each of its annexes (a combat unit's or a building's type)
/// is repaired by the amount first ([`RepairFrom::MainBuilding`]); then the amount, at most what
/// its hit points lack of its master's, costs its owner's building repair rate times the amount
/// over its master's hit points of its master's costs ([`crate::production::pay`]); paid, it heals
/// as a unit's does, else [`RepairOutcome::Unpaid`].
///
/// # Errors
///
/// An object outside `set`, and an attribute the player does not have.
pub fn repair_building(
    set: &mut [Repaired],
    idx: usize,
    amount: HitPoints,
    from: RepairFrom,
    payer: &mut Payer<'_>,
    roles: &Roles,
    slots: Slots,
) -> GenieResult<RepairOutcome> {
    let checked = set.checked(idx)?;
    if roles
        .of_class(checked.class)
        .contains(ClassRoles::REPAIRED_AS_UNIT)
    {
        return repair_unit(set, idx, amount, payer, roles, slots);
    }
    if below_or_unordered(checked.hp.get(), ONE) {
        return Ok(RepairOutcome::Unpaid);
    }
    if from == RepairFrom::Repairer
        && let Some(main) = checked.main
    {
        let m = find(set, main.id())?;
        return repair_building(set, m, amount, RepairFrom::Repairer, payer, roles, slots);
    }
    for annex in checked.annexes.clone() {
        let a = find(set, annex)?;
        match set.checked(a)?.kind {
            MasterKind::COMBAT => {
                let _ = repair_unit(set, a, amount, payer, roles, slots)?;
            }
            MasterKind::BUILDING => {
                let _ = repair_building(
                    set,
                    a,
                    amount,
                    RepairFrom::MainBuilding,
                    payer,
                    roles,
                    slots,
                )?;
            }
            _ => {}
        }
    }
    let repaired = set.checked(idx)?;
    let most = HitPoints::of_master(repaired.master_hp);
    let mut amount = amount;
    if most < repaired.hp + amount {
        amount = most - repaired.hp;
    }
    let share = CostCount::new((repaired.building_repair_cost.get() * amount) / most);
    if pay(
        payer.masters,
        repaired.master,
        share,
        &mut *payer.attributes,
        roles,
        slots,
    )? == Payment::Paid
    {
        return repair_unit(set, idx, amount, payer, roles, slots);
    }
    Ok(RepairOutcome::Unpaid)
}

/// A combat unit's slot 0x8c8 (`FUN_1412143b0`, also a building's last step) by `amount`: a class
/// of [`ClassRoles::REPAIR_COSTED`](crate::world::layer::ClassRoles::REPAIR_COSTED) first takes at
/// most what its hit points lack of its most (slots 0xc0 and 200) and pays its owner's unit repair
/// rate times the amount over its most of its master's costs (unpaid: [`RepairOutcome::Unpaid`]);
/// then it heals (`FUN_141193b30`, [`crate::combat::damage::heal_hp`]): [`RepairOutcome::Full`]
/// once full, else [`RepairOutcome::Healed`].
///
/// # Errors
///
/// An attribute the player does not have.
pub fn repair_unit(
    set: &mut [Repaired],
    idx: usize,
    amount: HitPoints,
    payer: &mut Payer<'_>,
    roles: &Roles,
    slots: Slots,
) -> GenieResult<RepairOutcome> {
    let repaired = set.checked_mut(idx)?;
    let mut amount = amount;
    if roles
        .of_class(repaired.class)
        .contains(ClassRoles::REPAIR_COSTED)
    {
        let most = HitPoints::of_master(repaired.master_hp) + repaired.hp_powerup.max;
        let now = repaired.hp_powerup.room() + repaired.hp;
        if most < now + amount {
            amount = most - now;
        }
        let share = CostCount::new((repaired.unit_repair_cost.get() * amount) / most);
        if pay(
            payer.masters,
            repaired.master,
            share,
            &mut *payer.attributes,
            roles,
            slots,
        )? == Payment::Unpaid
        {
            return Ok(RepairOutcome::Unpaid);
        }
    }
    Ok(
        match heal_hp(
            &mut repaired.hp,
            &mut repaired.hp_powerup,
            repaired.master_hp,
            amount,
        ) {
            Heal::Full => RepairOutcome::Full,
            Heal::Partial => RepairOutcome::Healed,
        },
    )
}

/// A repairer's player's attributes as its repair pays from them
/// ([`crate::construction::repair::Payer`]): the attributes as the repair reads them, each payment
/// kept as an add the caller makes the owner's right after the repair's step (a subtraction, so the
/// player's value comes out as the repair saw it).
pub(crate) struct PaidAttributes {
    pub(super) values: Vec<Amount>,
    pub(crate) adds: Vec<(AttributeId, Amount)>,
}

impl PaidAttributes {
    pub(crate) fn new(attributes: &[Amount]) -> Self {
        Self {
            values: attributes.to_vec(),
            adds: Vec::new(),
        }
    }
}

impl Attributes for PaidAttributes {
    fn get(&self, attribute_id: AttributeId) -> GenieResult<Amount> {
        self.values
            .get(attribute_id.index())
            .copied()
            .ok_or(NO_ATTRIBUTE)
    }

    fn subtract(&mut self, attribute_id: AttributeId, amount: Amount) -> GenieResult<()> {
        let value = self
            .values
            .get_mut(attribute_id.index())
            .ok_or(NO_ATTRIBUTE)?;
        *value -= amount;
        self.adds.push((attribute_id, -amount));
        Ok(())
    }
}
