//! What `bestUnitToAttack` reads of the world through the unit's picker ([`WorldPicker`]): the
//! searching unit, the found objects, the attack and neutral tests, the statistics' reads and
//! the quick path.

use crate::map::Distance;
use crate::map::spatial::Spatial;
use crate::movement::Speed;
use crate::movement::footprint_gap;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{DamageAmount, DamageClass};
use crate::objects::unit::UnitFlags;
use crate::ownership::CaptureKind;
use crate::pathing::hull_path::{NO_RESTRICTION, NO_TERRAIN, quick_reach};
use crate::players::AllyTest;
use crate::unit_ai::TargetRules;
use crate::unit_ai::best_target::FoundFlags;
use crate::unit_ai::best_target::{
    AttackCategory, Candidate, FoundObject, Searcher, order_weights,
};
use crate::unit_ai::task_finder::allied;
use crate::unit_ai::world_view::NOT_HELD;
use crate::unit_ai::world_view::WorldPicker;
use crate::unit_ai::world_view::attacks;
use crate::unit_ai::{
    Memory, attack_sight, attacks_neutral as unit_ai_attacks_neutral,
    can_attack as unit_ai_can_attack, spares_friends as unit_ai_spares_friends,
};
use crate::world::AiSwitches;
use crate::{GenieError, GenieResult};

/// What `bestUnitToAttack` (`FUN_140c44680`, [`crate::unit_ai::best_target::best_unit_to_attack`])
/// asks of the world while its unit AI runs: the searching unit ([`WorldPicker::searcher`]) and
/// its reads and tests of the objects its search found; the paths (slot 0x3d0) are the search's.
impl WorldPicker<'_> {
    /// The unit, its master and its AI as `bestUnitToAttack` reads them: the AI's targets, order
    /// and action ([`crate::unit_ai::Seeker`]), its may-move test, the sight
    /// ([`crate::unit_ai::attack_sight`]), its ranges and speed, its owner's walls flag and
    /// targeting, the order history's weights and the retarget entries' targets.
    ///
    /// # Errors
    ///
    /// A unit the reads do not model (a computer player's).
    pub(crate) fn searcher(&self, memory: Memory<'_>) -> GenieResult<Searcher> {
        let best_target_tuning = &self.by_id.tuning().best_target;
        let seeker = self.seeker.ok_or(GenieError::unsupported(
            "an attack test outside bestUnitToAttack",
        ))?;
        let attacker = self.attacker;
        Ok(Searcher {
            id: self.me,
            target: seeker.target,
            target_classes: [seeker.target_class, seeker.last_target_class],
            order: seeker.order,
            action: seeker.action,
            master: attacker.master,
            class: attacker.class,
            kind: attacker.kind,
            attack_priority: attacker.attack_priority,
            player_kind: attacker.player_kind,
            walls_allowed: self
                .ai_settings
                .switches
                .contains(AiSwitches::MAY_ATTACK_WALLS),
            can_move: seeker.can_move(),
            sight: attack_sight(attacker)?,
            range: attacker.range_against()?,
            weapon_range: attacker.range,
            min_range: attacker.min_range,
            speed: attacker.speed,
            scenario: attacker.scenario,
            targeting: (
                self.ai_settings.local_targeting_mode,
                self.ai_settings.targeting_factor_scalar,
            ),
            weights: order_weights(memory.history, self.now, best_target_tuning),
            retries: memory.retries.iter().map(|&(id, _)| id).collect(),
            now: self.now,
        })
    }

    /// The object `id` as the world holds it, None for one it does not.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    pub(crate) fn found_object(
        &self,
        spatial: &Spatial,
        id: ObjectId,
    ) -> GenieResult<Option<FoundObject>> {
        let Some((owner, unit, _, master)) = self.held(id)? else {
            return Ok(None);
        };
        let placed = spatial
            .get_known(id)?
            .ok_or(GenieError::unsupported("a found object without a place"))?;
        let capture = spatial
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(id))
            .map_or(CaptureKind::None, |seer| seer.capture);
        let attacker = self.attacker;
        Ok(Some(FoundObject {
            owner,
            flags: {
                let mut flags = FoundFlags::empty();
                flags.set(
                    FoundFlags::ALLIED,
                    allied(self.relations, self.player, owner, AllyTest::WithRelation4),
                );
                flags.set(FoundFlags::HAS_AI, self.parts.unit_ai.contains(unit.id));
                flags.set(FoundFlags::INSIDE, unit.life.inside.is_some());
                flags.set(
                    FoundFlags::TARGETABLE,
                    !unit.flags.contains(UnitFlags::UNTARGETABLE),
                );
                flags.set(
                    FoundFlags::ATTACKABLE,
                    unit.flags.contains(UnitFlags::ATTACKABLE),
                );
                flags
            },
            state: unit.life.state,
            master: unit.life.master_id,
            class: master.class,
            kind: master.kind,
            capture,
            held: unit.life.amount,
            distance: footprint_gap(
                attacker.position,
                attacker.radius,
                placed.position,
                placed.radius,
            ),
            speed: self
                .parts
                .mover_of(unit.id)
                .map_or(Speed::ZERO, |mv| mv.stats.speed),
        }))
    }

    /// The unit AI's attack check (slot 0xa0 with 0) on object `id`.
    ///
    /// # Errors
    ///
    /// What the attack check does not model.
    pub(crate) fn can_attack(&self, spatial: &Spatial, id: ObjectId) -> GenieResult<bool> {
        let roles = self.by_id.roles();
        let unit_ai_tuning = &self.by_id.tuning().unit_ai;
        let seeker = self.seeker.ok_or(GenieError::unsupported(
            "an attack test outside bestUnitToAttack",
        ))?;
        let (target, _) = self.target(spatial, id)?;
        unit_ai_can_attack(self.attacker, &seeker, &target, roles, unit_ai_tuning)
    }

    /// The unit AI's neutral test (slot 0xa8) of an object of `class`.
    pub(crate) fn attacks_neutral(&self, class: ClassId) -> bool {
        let roles = self.by_id.roles();
        unit_ai_attacks_neutral(self.attacker, class, roles)
    }

    /// `FUN_14120e4e0` of a unit of the combat type on object `id`: it spares its friends.
    ///
    /// # Errors
    ///
    /// What the scan does not model.
    pub(crate) fn spares_friends(&self, spatial: &Spatial, id: ObjectId) -> GenieResult<bool> {
        let roles = self.by_id.roles();
        let (target, _) = self.target(spatial, id)?;
        unit_ai_spares_friends(self.attacker, &target, roles)
    }

    /// `FUN_140c41690`: a human player's unit passes nothing over (the computer player's test
    /// never runs: its search fails at the sight).
    pub(crate) const fn shuns(&self, _: ObjectId) -> bool {
        false
    }

    /// What the statistics read of object `id`: its class and speed, whether it is armed (its
    /// attacks' sum, slot 0x228, above 0, or its priest's conversion test on the unit), the unit's
    /// range against it, the unit's damage per hit to it (slot 0x220: the unit's elevation factor,
    /// slot 0x1f0, and its armor, slot 0xf0, [`crate::combat::damage::armor_damage`]), its hit
    /// points, the unit's reload time (slot 0x218) and the unit's own conversion test on it.
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    pub(crate) fn candidate(
        &self,
        spatial: &Spatial,
        id: ObjectId,
        distance: Distance,
        category: Option<AttackCategory>,
    ) -> GenieResult<Candidate> {
        let damage_tuning = &self.by_id.tuning().damage;
        let unit_ai_tuning = &self.by_id.tuning().unit_ai;
        let roles = self.by_id.roles();
        let (_, unit, _, master) = self.held(id)?.ok_or(NOT_HELD)?;
        let (_, me, _, mm) = self.held(self.me)?.ok_or(GenieError::unsupported(
            "a searching unit of a master the slice does not have",
        ))?;
        let damage = self.damage(spatial, self.me, id, roles, damage_tuning)?;
        let armed = 0.0 < attacks(master)
            || self.converts(id, self.me, TargetRules::Any, roles, unit_ai_tuning)?;
        Ok(Candidate {
            id,
            distance,
            category,
            class: master.class,
            speed: self
                .parts
                .mover_of(unit.id)
                .map_or(Speed::ZERO, |mv| mv.stats.speed),
            armed,
            reach: self.attacker.range_against()?,
            damage,
            hp: unit.life.hp,
            reload: self.parts.combat_of(me.id).reload_stinger
                + (mm.combat_part().reload - self.parts.combat_of(me.id).reload_powerup),
            workable: self.converts(self.me, id, TargetRules::Own, roles, unit_ai_tuning)?,
            current: self.seeker.is_some_and(|seeker| seeker.target == Some(id)),
        })
    }

    /// Slot 0x870's armor (`FUN_141213920` -> `FUN_1410857f0`): the highest melee armor (class
    /// 3; else the base armor or the highest armor of any class) plus the pierce armor powerup
    /// (`+0xa4c`, slot 0x710; slot 0x778's stinger `+0xb50` is 0), at least 100.
    ///
    /// # Errors
    ///
    /// What the read does not model.
    pub(crate) fn armored(&self, id: ObjectId) -> GenieResult<bool> {
        let (_, unit, _, master) = self.held(id)?.ok_or(NOT_HELD)?;
        let class3 = master
            .combat_part()
            .armors
            .iter()
            .filter(|a| a.class == DamageClass::PIERCE)
            .map(|a| a.amount)
            .max();
        let armor = class3.unwrap_or_else(|| {
            master.combat_part().armors.iter().fold(
                master.combat_part().base_armor,
                |best, class_amount| {
                    if best < class_amount.amount {
                        class_amount.amount
                    } else {
                        best
                    }
                },
            )
        });
        Ok(DamageAmount::new(100)
            <= armor.plus(self.parts.combat_of(unit.id).armor_powerups.pierce))
    }

    /// Object `id`'s weapon range (slot 0x230: a combat object's master's, else 0).
    ///
    /// # Errors
    ///
    /// What the read does not model.
    pub(crate) fn weapon_range(&self, id: ObjectId) -> GenieResult<Distance> {
        let (_, _, _, master) = self.held(id)?.ok_or(NOT_HELD)?;
        Ok(if MasterKind::RGE_COMBAT <= master.kind {
            master.combat_part().weapon_range
        } else {
            Distance::ZERO
        })
    }

    /// The current target `id` engages: its attacks sum above 0 (slot 0x228) or its unit AI
    /// could work on the unit (slot 0x1b8 with 1).
    ///
    /// # Errors
    ///
    /// What the reads do not model.
    pub(crate) fn engages(&self, id: ObjectId) -> GenieResult<bool> {
        let roles = self.by_id.roles();
        let unit_ai_tuning = &self.by_id.tuning().unit_ai;
        let (_, _, _, master) = self.held(id)?.ok_or(NOT_HELD)?;
        Ok(0.0 < attacks(master)
            || self.converts(id, self.me, TargetRules::Any, roles, unit_ai_tuning)?)
    }

    /// The quick path (slot 0x3e0) within `range`, with its master's terrain mask and radius on
    /// x.
    ///
    /// # Errors
    ///
    /// What the path does not model.
    pub(crate) fn quick_path(
        &self,
        spatial: &mut Spatial,
        id: ObjectId,
        range: Distance,
    ) -> GenieResult<bool> {
        let terrain = spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
        let restriction = self.restriction.ok_or(NO_RESTRICTION)?;
        let mask = terrain.restriction_mask(restriction);
        quick_reach(
            (spatial, self.by_id),
            self.me,
            Some(id),
            mask,
            Distance::new(self.attacker.radius.x()),
            range,
        )
    }
}
