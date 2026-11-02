//! An effect's commands by kind: the `.dat`'s 12 bytes each (a kind and four arguments) as
//! `TRIBE_Effects::do_effect` (`FUN_141462440`) and its base (`FUN_140ec2ac0`) read them, parsed
//! once where the `.dat` is read.

use crate::GenieError;
use crate::math::conversions::cvttss2si;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::players::attribute::AttributeId;
use crate::technology::UpgradeMode;
use crate::technology::effect::Change;
use crate::technology::{ChangeMode, DisabledTech, ModifyLocation, ModifyMode, TechId};
use std::ops::{Range, RangeInclusive};

/// The `TRIBE_Effects` command kinds that change technologies (`'e'`, `'f'`, `'g'`): a cost, the
/// disabling and a research time.
const TECH_COST: u8 = b'e';
const TECH_DISABLE: u8 = b'f';
const TECH_TIME: u8 = b'g';

/// The `RGE_Effects` command kinds the base (`FUN_140ec2ac0`) switches on: the player's own (0 to
/// 8), the same for its mutual allies (10 to 18), enemies (20 to 28), neutrals (30 to 38) and Gaia
/// (40 to 48), and the objects' (200 to 206).
mod kind {
    pub const SET: u8 = 0;
    pub const RESOURCE: u8 = 1;
    pub const ENABLE: u8 = 2;
    pub const UPGRADE: u8 = 3;
    pub const ADD: u8 = 4;
    pub const MULTIPLY: u8 = 5;
    pub const RESOURCE_MULTIPLY: u8 = 6;
    pub const SPAWN: u8 = 7;
    pub const TECH: u8 = 8;
    /// A set, add and multiply on the objects with masters of their own of a master or class
    /// (200 to 202), on the effect's object (203 to 205), and its master's change (206).
    pub const OBJECTS_SET: u8 = 200;
    pub const OBJECT_SET: u8 = 203;
    pub const OBJECT_UPGRADE: u8 = 206;
}

/// The cost attributes a multiply by a negative amount turns into a cost entry's removal or
/// addition (`FUN_1415c1f70`): 103 to 106.
const COST_ATTRIBUTES: RangeInclusive<i16> = 103..=106;

/// A cost command's master argument from which it names a class (`FUN_1415c1f70`: 900 to 999,
/// the class + 900).
const CLASS_UNITS: Range<i16> = 900..1000;

/// Which players a block of the base's kinds reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Party {
    /// The player itself (0 to 8).
    Own,
    /// Its mutual allies (10 to 18, both relations 0).
    Allies,
    /// Its enemies (20 to 28, both relations 3 or both 4).
    Enemies,
    /// Its neutrals (30 to 38, both relations 1).
    Neutrals,
    /// Gaia (40 to 48).
    Gaia,
}

impl Party {
    /// The party of a command kind's block (its tens: 0 the player, 1 its allies, 2 its enemies,
    /// 3 its neutrals, 4 Gaia).
    const fn of_block(block: u8) -> Self {
        match block {
            0 => Self::Own,
            1 => Self::Allies,
            2 => Self::Enemies,
            3 => Self::Neutrals,
            _ => Self::Gaia,
        }
    }
}

/// The masters a cost command changes (`FUN_1415c1f70`'s `unit`): a master by its id, or every
/// master of a class (a `unit` of 900 to 999 names class `unit − 900`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostUnits {
    /// The master of this id.
    Master(MasterId),
    /// Every master of this class, by id.
    Class(ClassId),
}

impl CostUnits {
    /// The masters `unit` names; a negative one names none.
    fn of_unit(unit: i16) -> Option<Self> {
        if CLASS_UNITS.contains(&unit)
            && let Ok(class) = ClassId::try_from(unit - CLASS_UNITS.start)
        {
            return Some(Self::Class(class));
        }
        MasterId::of_data(unit).map(Self::Master)
    }
}

/// What a cost command does with the cost slot of its resource (`FUN_1415c1f70`'s `how`, the
/// amount as CVTTSS2SI gives it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostEdit {
    /// −2: the first slot that takes the resource empties in its place.
    Remove,
    /// −1: the first empty slot takes the resource, amount −1 and paid.
    Add,
}

impl CostEdit {
    /// The edit of `how`; any other number does nothing.
    const fn of_how(how: i32) -> Option<Self> {
        match how {
            -2 => Some(Self::Remove),
            -1 => Some(Self::Add),
            _ => None,
        }
    }
}

/// How a resource command changes the resource (its second argument's low byte: 0 sets, any
/// other adds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceChange {
    /// The resource takes the amount.
    Set,
    /// The amount adds to it.
    Add,
}

/// The objects' kinds (200 to 206), which the slice does not run but for the masters of their
/// own it holds none of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectsKind {
    /// 200 to 202: the objects with masters of their own of a master or class.
    OwnMasters(Change),
    /// 203 to 205: the effect's object, which first takes a master of its own.
    Object(Change),
    /// 206: the effect's object's master change.
    ObjectMaster,
}

/// One command of an effect, by its kind. The base's kinds name the [`Party`] they reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectCommand {
    /// Kinds 0, 4 and 5 of a block: the masters' attribute `attribute` (the third argument's low
    /// byte) set, added to or multiplied by `amount`, on master `master` or class `class` (each
    /// none for −1) through slot 0x148.
    Attribute {
        /// The block's players.
        party: Party,
        /// Kind 0 sets, 4 adds, 5 multiplies.
        change: Change,
        /// The first argument (a negative one is none).
        master: Option<MasterId>,
        /// The second argument.
        class: Option<ClassId>,
        /// The third argument's low byte.
        attribute: MasterAttribute,
        /// The fourth.
        amount: f32,
    },
    /// Kind 5 outside Gaia's block on a cost attribute (103 to 106) by a negative amount
    /// (`COMISS` 0, d; `JBE`: not a NaN): the cost slots of `units` (the second argument, or
    /// the first for a negative one) change instead (`FUN_1415c1f70`).
    Costs {
        /// The block's players.
        party: Party,
        /// The masters it names (none for a negative argument).
        units: Option<CostUnits>,
        /// The cost attribute.
        attribute: MasterAttribute,
        /// The amount as CVTTSS2SI gives it (none for any but −2 and −1).
        edit: Option<CostEdit>,
    },
    /// Kind 1 of a block: resource `attribute` changes by `amount` times the effect's player's
    /// attribute `scale`.
    Resource {
        /// The block's players.
        party: Party,
        /// The first argument (a negative one is none: nothing changes).
        attribute: Option<AttributeId>,
        /// The second argument's low byte.
        change: ResourceChange,
        /// The third argument (a negative one is none: the amount as it is).
        scale: Option<AttributeId>,
        /// The fourth.
        amount: f32,
    },
    /// Kind 6 of a block: resource `attribute` multiplied by `amount`.
    ResourceMultiply {
        /// The block's players.
        party: Party,
        /// The first argument (a negative one is none: nothing changes).
        attribute: Option<AttributeId>,
        /// The fourth.
        amount: f32,
    },
    /// Kind 2 of a block: master `master` enabled or disabled.
    Enable {
        /// The block's players.
        party: Party,
        /// The first argument (a negative one is none: nothing changes).
        master: Option<MasterId>,
        /// The second argument's low byte (1 enables, 0 disables).
        enable: u8,
        /// The third argument is 1: the command is for one building, the effect's object.
        one: bool,
    },
    /// Kind 3 of a block: master `from`'s objects upgraded to `to`.
    Upgrade {
        /// The block's players.
        party: Party,
        /// The first argument.
        from: Option<MasterId>,
        /// The second.
        to: Option<MasterId>,
        /// The third: the upgrade's mode.
        mode: UpgradeMode,
    },
    /// Kind 7 of a block: `count` objects of `master` spawned at the player's buildings of `at`.
    Spawn {
        /// The block's players.
        party: Party,
        /// The first argument.
        master: Option<MasterId>,
        /// The second.
        at: Option<MasterId>,
        /// The third.
        count: i16,
    },
    /// Kind 8 of a block: technology `tech`'s change by `mode` and `amount` at `location`
    /// (`FUN_1415c3f20`).
    Tech {
        /// The block's players.
        party: Party,
        /// The first argument (a negative one is none).
        tech: Option<TechId>,
        /// The second.
        mode: ModifyMode,
        /// The fourth.
        amount: f32,
        /// The third.
        location: ModifyLocation,
    },
    /// The objects' kinds (200 to 206).
    Objects(ObjectsKind),
    /// `'e'`: technology `tech`'s cost of `resource` (any for none) by `amount` in `mode` (the
    /// third argument's low byte).
    TechCost {
        /// The first argument (a negative one is none).
        tech: Option<TechId>,
        /// The second (−1 for any).
        resource: Option<AttributeId>,
        /// The fourth.
        amount: f32,
        /// The third argument's low byte.
        mode: ChangeMode,
    },
    /// `'f'`: the technology the amount names (CVTTSS2SI) disabled; −1 disables none. The
    /// shared kind's test reads the whole number, the disabling its low 16 bits.
    TechDisable {
        /// The fourth argument as CVTTSS2SI gives it.
        tech: DisabledTech,
    },
    /// `'g'`: technology `tech`'s research time by `amount` in `mode` (the third argument's low
    /// byte).
    TechTime {
        /// The first argument (a negative one is none).
        tech: Option<TechId>,
        /// The fourth.
        amount: f32,
        /// The third argument's low byte.
        mode: ChangeMode,
    },
    /// A kind neither switch knows (9, 19, 29, 39, 49 and up but the ones above): nothing.
    Inert(u8),
}

impl EffectCommand {
    /// The command of kind `kind` with the `.dat`'s arguments `a`, `b`, `c` and `d`, as the two
    /// switches read them.
    ///
    /// # Errors
    ///
    /// A class (an attribute command's second argument) or a cost's resource (`'e'`'s second)
    /// below −1.
    pub fn of_data(kind: u8, a: i16, b: i16, c: i16, d: f32) -> Result<Self, GenieError> {
        // the third argument's low byte, the mode of the technology and resource commands
        let low = |v: i16| v.to_le_bytes()[0];
        Ok(match kind {
            TECH_COST => Self::TechCost {
                tech: TechId::of_data(a),
                resource: AttributeId::of_field(b)?,
                amount: d,
                mode: ChangeMode::of_data(low(c)),
            },
            TECH_DISABLE => Self::TechDisable {
                tech: DisabledTech::of_data(cvttss2si(d)),
            },
            TECH_TIME => Self::TechTime {
                tech: TechId::of_data(a),
                amount: d,
                mode: ChangeMode::of_data(low(c)),
            },
            kind::OBJECTS_SET..kind::OBJECT_SET => {
                Self::Objects(ObjectsKind::OwnMasters(change_of(kind - kind::OBJECTS_SET)))
            }
            kind::OBJECT_SET..kind::OBJECT_UPGRADE => {
                Self::Objects(ObjectsKind::Object(change_of(kind - kind::OBJECT_SET)))
            }
            kind::OBJECT_UPGRADE => Self::Objects(ObjectsKind::ObjectMaster),
            _ if 4 < kind / 10 || 8 < kind % 10 => Self::Inert(kind),
            _ => Self::of_block(kind, a, b, c, d)?,
        })
    }

    /// The base's kinds of a block (0 to 48, the units 0 to 8).
    fn of_block(kind: u8, a: i16, b: i16, c: i16, d: f32) -> Result<Self, GenieError> {
        let party = Party::of_block(kind / 10);
        Ok(match kind % 10 {
            kind::SET | kind::ADD | kind::MULTIPLY => {
                let change = match kind % 10 {
                    kind::SET => Change::Set,
                    kind::ADD => Change::Add,
                    _ => Change::Multiply,
                };
                let attribute = MasterAttribute::of_effect(c);
                if change == Change::Multiply
                    && COST_ATTRIBUTES.contains(&c)
                    && 0.0 > d
                    && party != Party::Gaia
                {
                    Self::Costs {
                        party,
                        units: CostUnits::of_unit(if b < 0 { a } else { b }),
                        attribute,
                        edit: CostEdit::of_how(cvttss2si(d)),
                    }
                } else {
                    Self::Attribute {
                        party,
                        change,
                        master: MasterId::of_data(a),
                        class: ClassId::of_field(b)?,
                        attribute,
                        amount: d,
                    }
                }
            }
            kind::RESOURCE => Self::Resource {
                party,
                attribute: AttributeId::of_data(a),
                change: if b.to_le_bytes()[0] == 0 {
                    ResourceChange::Set
                } else {
                    ResourceChange::Add
                },
                scale: AttributeId::of_data(c),
                amount: d,
            },
            kind::RESOURCE_MULTIPLY => Self::ResourceMultiply {
                party,
                attribute: AttributeId::of_data(a),
                amount: d,
            },
            kind::ENABLE => Self::Enable {
                party,
                master: MasterId::of_data(a),
                enable: b.to_le_bytes()[0],
                one: c == 1,
            },
            kind::UPGRADE => Self::Upgrade {
                party,
                from: MasterId::of_data(a),
                to: MasterId::of_data(b),
                mode: UpgradeMode::of_data(c),
            },
            kind::SPAWN => Self::Spawn {
                party,
                master: MasterId::of_data(a),
                at: MasterId::of_data(b),
                count: c,
            },
            kind::TECH => Self::Tech {
                party,
                tech: TechId::of_data(a),
                mode: ModifyMode::of_data(b),
                amount: d,
                location: ModifyLocation::of_data(c),
            },
            _ => Self::Inert(kind),
        })
    }
}

/// The change of a kind's offset in its run: 0 sets, 1 adds, 2 multiplies.
const fn change_of(offset: u8) -> Change {
    match offset {
        0 => Change::Set,
        1 => Change::Add,
        _ => Change::Multiply,
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{ClassId, MasterAttribute};
    use crate::players::attribute::AttributeId;
    use crate::technology::ChangeMode;
    use crate::technology::DisabledTech;
    use crate::technology::TechId;
    use crate::technology::UpgradeMode;
    use crate::technology::effect::Change;
    use crate::technology::{
        CostEdit, CostUnits, EffectCommand, ObjectsKind, Party, ResourceChange,
    };
    use crate::test_ids::master_id;

    fn parse(kind: u8, a: i16, b: i16, c: i16, d: f32) -> EffectCommand {
        EffectCommand::of_data(kind, a, b, c, d).unwrap()
    }

    /// A kind's tens name its block's players and its units the command; 9 and 49 up are none.
    #[test]
    fn kinds_tens_are_its_party_and_its_units_the_command() {
        assert_eq!(
            parse(24, 4, -1, 0x0108, 2.0),
            EffectCommand::Attribute {
                party: Party::Enemies,
                change: Change::Add,
                master: Some(master_id(4)),
                class: None,
                attribute: MasterAttribute::numbered(8),
                amount: 2.0,
            }
        );
        assert_eq!(
            [0, 10, 20, 30, 40].map(|k| match parse(k, -1, -1, 0, 1.0) {
                EffectCommand::Attribute { party, change, .. } => Some((party, change)),
                _ => None,
            }),
            [
                Some((Party::Own, Change::Set)),
                Some((Party::Allies, Change::Set)),
                Some((Party::Enemies, Change::Set)),
                Some((Party::Neutrals, Change::Set)),
                Some((Party::Gaia, Change::Set)),
            ]
        );
        for k in [9, 19, 49, 50, 100, 104, 199, 207, 255] {
            assert_eq!(parse(k, 1, 2, 3, 4.0), EffectCommand::Inert(k));
        }
    }

    /// A multiply of a cost attribute (103 to 106) by a negative amount outside Gaia's block
    /// edits the cost slots: of the class named by 900 and up, else the master; NaN and Gaia's
    /// block multiply the attribute.
    #[test]
    fn negative_cost_multiply_edits_the_cost_slots() {
        assert_eq!(
            parse(5, 4, -1, 104, -2.0),
            EffectCommand::Costs {
                party: Party::Own,
                units: Some(CostUnits::Master(master_id(4))),
                attribute: MasterAttribute::numbered(104),
                edit: Some(CostEdit::Remove),
            }
        );
        assert_eq!(
            parse(15, -1, 903, 103, -1.0),
            EffectCommand::Costs {
                party: Party::Allies,
                units: Some(CostUnits::Class(ClassId::of_master(3))),
                attribute: MasterAttribute::numbered(103),
                edit: Some(CostEdit::Add),
            }
        );
        assert!(matches!(
            parse(5, i16::MIN, -1, 105, -3.0),
            EffectCommand::Costs {
                units: None,
                edit: None,
                ..
            }
        ));
        assert!(matches!(
            parse(45, 4, -1, 104, -2.0),
            EffectCommand::Attribute { .. }
        ));
        assert!(matches!(
            parse(5, 4, -1, 104, f32::NAN),
            EffectCommand::Attribute { .. }
        ));
    }

    /// The resource, master and technology kinds read their arguments by kind.
    #[test]
    fn each_kind_reads_its_arguments() {
        assert_eq!(
            parse(1, 3, 0x0100, -1, 20.0),
            EffectCommand::Resource {
                party: Party::Own,
                attribute: Some(AttributeId::numbered(3)),
                change: ResourceChange::Set,
                scale: None,
                amount: 20.0,
            }
        );
        assert!(matches!(
            parse(1, -1, 1, 269, 1.0),
            EffectCommand::Resource {
                attribute: None,
                change: ResourceChange::Add,
                ..
            }
        ));
        assert_eq!(
            parse(2, 83, 0x0201, 1, 0.0),
            EffectCommand::Enable {
                party: Party::Own,
                master: Some(master_id(83)),
                enable: 1,
                one: true,
            }
        );
        assert_eq!(
            parse(33, 4, 24, 2, 0.0),
            EffectCommand::Upgrade {
                party: Party::Neutrals,
                from: Some(master_id(4)),
                to: Some(master_id(24)),
                mode: UpgradeMode::ONE_OBJECT,
            }
        );
        assert_eq!(
            parse(b'e', 7, -1, 0x0102, 3.0),
            EffectCommand::TechCost {
                tech: TechId::of_data(7),
                resource: None,
                amount: 3.0,
                mode: ChangeMode::SCALE,
            }
        );
        assert_eq!(
            parse(b'f', -1, -1, -1, 65538.0),
            EffectCommand::TechDisable {
                tech: DisabledTech::of_data(65538)
            }
        );
        assert_eq!(
            [200, 202, 203, 205, 206].map(|k| parse(k, -1, -1, 0, 1.0)),
            [
                EffectCommand::Objects(ObjectsKind::OwnMasters(Change::Set)),
                EffectCommand::Objects(ObjectsKind::OwnMasters(Change::Multiply)),
                EffectCommand::Objects(ObjectsKind::Object(Change::Set)),
                EffectCommand::Objects(ObjectsKind::Object(Change::Multiply)),
                EffectCommand::Objects(ObjectsKind::ObjectMaster),
            ]
        );
    }

    /// A class below −1, or a cost's resource below −1, is no value of the data.
    #[test]
    fn class_or_resource_below_minus_one_fails() {
        assert!(EffectCommand::of_data(0, -1, -2, 0, 1.0).is_err());
        assert!(EffectCommand::of_data(b'e', 0, -2, 0, 1.0).is_err());
    }
}
