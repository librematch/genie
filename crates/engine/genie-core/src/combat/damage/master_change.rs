//! A hit's victim that changes its master in the hit: slot 22's hit point transforms
//! (`FUN_141204450`) give it another master at once, and the rest of the hit reads that master's
//! values ([`Victim::take_master`]).

use crate::combat::AttackBonus;
use crate::combat::damage::{ArmorBonuses, Armored, Attacker, Building, Victim};
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::orders::hit_point_share;
use std::mem::take;

impl Victim {
    /// `FUN_141204450` on a hit's victim that slot 22 left at 1 hit point or more: the first
    /// transform of its master ([`HitTransforms`](crate::combat::hp_transforms::HitTransforms))
    /// whose threshold holds for its hit points gives it that master at once
    /// ([`Victim::take_master`]); its write-back changes the object's master
    /// ([`Victim::master_changes`]).
    pub(crate) fn transform_in_hit(&mut self) {
        let transforms = take(&mut self.transforms);
        if let Some(into) = transforms.first_holding(self.master_id, self.hp) {
            self.take_master(&into.reads);
            self.master_changes.push(into.id);
        }
        self.transforms = transforms;
    }

    /// Slot 0x90 with 1 (`FUN_141225b70`) on a hit's victim, `reads` what a hit reads of the
    /// object with the new master: below state 3 (`FUN_1411a6a70`) its hit points keep their
    /// share of the new master's ([`hit_point_share`]) and it holds the new master's values (its
    /// type, class, radii, storage, blast defense, decay, weapons, hit points and tasks, its
    /// attack's and its armor's); then its powerups go (`FUN_1412066b0`: the attack, armor and
    /// most hit points powerups a hit reads; the hit points it took, `+0xa38`, stay).
    pub(crate) fn take_master(&mut self, reads: &Self) {
        if self.state < ObjectState::Dying {
            self.hp = hit_point_share(self.hp, self.master_hp, reads.master_hp);
            self.kind = reads.kind;
            self.class = reads.class;
            self.master_id = reads.master_id;
            self.radius = reads.radius;
            self.height = reads.height;
            self.storage = reads.storage;
            self.capacity = reads.capacity;
            self.blast_defense = reads.blast_defense;
            self.fog_visibility = reads.fog_visibility;
            self.decay = reads.decay;
            self.weapons.clone_from(&reads.weapons);
            self.master_hp = reads.master_hp;
            self.bonus_tasks.clone_from(&reads.bonus_tasks);
            self.stinger_tasks.clone_from(&reads.stinger_tasks);
            self.source.take_master(&reads.source);
            match (self.armored.as_mut(), reads.armored.as_ref()) {
                (Some(armored), Some(read)) => armored.take_master(read),
                (_, read) => self.armored = read.cloned(),
            }
            match (self.building.as_mut(), reads.building.as_ref()) {
                (Some(building), Some(read)) => building.take_master(read),
                (_, read) => self.building = read.cloned(),
            }
        }
        self.source.attack_powerup = AttackBonus::ZERO;
        if let Some(armored) = self.armored.as_mut() {
            armored.armor_powerups = ArmorBonuses::ZERO;
            armored.hp_powerup.max = HitPoints::ZERO;
        }
    }
}

impl Attacker {
    /// The new master's part of a hit's source ([`Victim::take_master`]): its type, class, id,
    /// combat ability, missile, ranges, blast, charge's kind, most, target and special attack,
    /// `FUN_1412161b0`'s answer, the infantry its slot 0x1b8 counts and its kill bonus tasks.
    const fn take_master(&mut self, read: &Self) {
        self.kind = read.kind;
        self.class = read.class;
        self.master_id = read.master_id;
        self.ability = read.ability;
        self.missile = read.missile;
        self.weapon_range = read.weapon_range;
        self.area_effect = read.area_effect;
        self.blast_level = read.blast_level;
        self.area_damage = read.area_damage;
        self.charge_kind = read.charge_kind;
        self.max_charge = read.max_charge;
        self.charge_target = read.charge_target;
        self.special_attack = read.special_attack;
        self.special_charge = read.special_charge;
        self.infantry_inside = read.infantry_inside;
        self.kill_bonus_tasks = read.kill_bonus_tasks;
    }
}

impl Armored {
    /// The new master's part of a hit's armor ([`Victim::take_master`]): its armors, base armor,
    /// terrain bonus and its tile's defense, resistance, combat ability, reflection,
    /// invulnerability, special attack, friendly fire, special sprite and kill tasks.
    fn take_master(&mut self, read: &Self) {
        self.armors.clone_from(&read.armors);
        self.base_armor = read.base_armor;
        self.terrain_bonus = read.terrain_bonus;
        self.terrain = read.terrain;
        self.resistance = read.resistance;
        self.ability = read.ability;
        self.reflection = read.reflection;
        self.invulnerability = read.invulnerability;
        self.special_attack = read.special_attack;
        self.friendly_fire = read.friendly_fire;
        self.special_sprite = read.special_sprite;
        self.kill_tasks = read.kill_tasks;
    }
}

impl Building {
    /// The new master's part of a hit on a building ([`Victim::take_master`]): whether it is a
    /// trebuchet and its paid cost.
    const fn take_master(&mut self, read: &Self) {
        self.trebuchet = read.trebuchet;
        self.cost = read.cost;
    }
}

#[cfg(test)]
mod tests;
