//! The powerup update (`FUN_141204a10`, [`World::powerup_update`]): an object's powerups end once
//! their hold has passed, and its auras power up what they find around it.

use crate::GenieResult;
use crate::combat::AbilityTimer;
use crate::combat::ClassOrMaster;
use crate::combat::aura::{AuraCount, AuraEntry, AuraGroup, AuraRole, aura_amount, aura_groups};
use crate::combat::powerup::Powerup;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::TaskFlags;
use crate::objects::sprite::{Cargo, new_sprite, standing};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::enabled;
use crate::vision::registry::VisibleQuery;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlotsExt;

impl AuraEntry {
    /// Whether the entry works for an object whose player has `attributes` and whose timed
    /// ability is `ability` at `now` (`FUN_141204a10`): its enabling attribute is above 0
    /// (`FUN_1410622c0`, [`enabled`]), it is not [`TaskFlags::AURA_OFF`], and with
    /// [`TaskFlags::AURA_WHILE_ABILITY`] the ability began (`FUN_14107d270`) and is not over
    /// (`FUN_14107d200`).
    fn works(&self, attributes: &[Amount], ability: AbilityTimer, now: WorldTime) -> bool {
        enabled(attributes, self.enablement)
            && !self.flags.contains(TaskFlags::AURA_OFF)
            && (!self.flags.contains(TaskFlags::AURA_WHILE_ABILITY)
                || (ability.began() && !ability.over(now)))
    }
}

/// The targets of an aura part's entries that work now, each with the role of its first entry
/// (`try_emplace` into the part's map at its node's `+0x58`).
type PartTargets = Vec<(ClassOrMaster, AuraRole)>;

/// The role `targets` give `target`, none where they do not name it (the map's `find`).
fn role_of(targets: &PartTargets, target: ClassOrMaster) -> Option<AuraRole> {
    targets
        .iter()
        .find(|&&(named, _)| named == target)
        .map(|&(_, role)| role)
}

impl World {
    /// `FUN_141204a10`, the powerup update of the combat object in `slot` (an object of type 70 or
    /// 80, after its passive abilities):
    /// - once the world time is past its powerups' end (`+0xa54`, unsigned), they go
    ///   ([`World::reset_powerups`]);
    /// - with its master's [`CombatAbility::AURA`], an aura group ([`aura_groups`]), outside any
    ///   container (`+0x2f0`, `FUN_141188da0`) and alive: its special sprite ends with its timed
    ///   ability ([`World::end_special_sprite`]), then each group's aura runs in turn
    ///   ([`World::run_aura`]).
    ///
    /// The renderer's part (the aura's sprites `+0x9f0`, `+0x9f8`, `FUN_1408cc810`) and the
    /// spectator's count `+0xa44` (slot 227) are not the simulation's.
    ///
    /// # Errors
    ///
    /// A master its player has not got, or what an aura's query or powerup does not model.
    pub(crate) fn powerup_update(
        &mut self,
        slot: (PlayerId, ListIndex, ObjectId),
    ) -> GenieResult<()> {
        let Some((player, index)) = self.live_slot(slot) else {
            return Ok(());
        };
        let unit = self.players.live(player, index)?;
        let id = unit.id;
        let kind = unit.master(self.master_tables(), player)?.kind;
        if !matches!(kind, MasterKind::COMBAT | MasterKind::BUILDING) {
            return Ok(());
        }
        if self.parts.combat_of(id).powerup_update < self.world_time {
            self.reset_powerups(id)?;
        }
        let unit = self.players.live(player, index)?;
        let master = unit.master(self.master_tables(), player)?;
        if !master
            .combat_part()
            .combat_ability
            .contains(CombatAbility::AURA)
        {
            return Ok(());
        }
        let groups = aura_groups(master, self.model.masters.of(player), &self.model.graphics);
        if groups.is_empty() || unit.life.inside.is_some() || unit.life.state != ObjectState::Alive
        {
            return Ok(());
        }
        self.end_special_sprite(player, index)?;
        for group in &groups {
            self.run_aura((player, index), group)?;
        }
        Ok(())
    }

    /// The aura's special sprite end (`FUN_141204a10`): once the timed ability of the object in
    /// `(player, index)` is over ([`AbilityTimer::over`]), an object that shows its master's
    /// special sprite (`+0x140` the master's `+0x8e8`) whose instance has reached its frame count
    /// (slot 0x38 of its sprite list, [`ActiveSprite::reached`]) shows its standing sprite
    /// (slot 0x68 with slot 0x798's, [`standing`]; for none the master's, `FUN_141158620`; none
    /// at all shows nothing new).
    ///
    /// [`ActiveSprite::reached`]: crate::objects::sprite::ActiveSprite::reached
    ///
    /// # Errors
    ///
    /// A slot the player's list does not hold, or a master the player has not got.
    fn end_special_sprite(&mut self, player: PlayerId, index: ListIndex) -> GenieResult<()> {
        let unit = self.players.live(player, index)?;
        let combat = self.parts.combat_of(unit.id);
        if !combat.ability.over(self.world_time) {
            return Ok(());
        }
        let master = unit.master(self.master_tables(), player)?;
        let graphics = &self.model.graphics;
        let special = master
            .creatable_part()
            .special_sprite
            .and_then(|sprite_id| graphics.get(sprite_id));
        let done = unit.sprite.is_some_and(|sprite| {
            special.is_some_and(|graphic| graphic.id == sprite.graphic.id)
                && sprite.reached(sprite.graphic.frames)
        });
        if !done {
            return Ok(());
        }
        let shown = standing(
            master,
            combat.abilities.look.as_ref(),
            Cargo::Empty,
            &self.model.roles,
        )
        .or(master.standing_sprite)
        .and_then(|sprite_id| graphics.get(sprite_id))
        .copied();
        let Some(graphic) = shown else {
            return Ok(());
        };
        let unit = self.players.live_mut(player, index)?;
        new_sprite(
            &mut unit.sprite,
            self.parts.move_stats_mut(unit.id),
            graphic,
            self.streams.get_mut(Stream::Sprite),
            self.model.tuning.sprite,
        );
        Ok(())
    }

    /// One aura group's aura (`FUN_141204a10`'s loop over `+0xa58`) of the object in
    /// `(player, index)`: each part's entries that work now ([`AuraEntry::works`]) name their
    /// targets, a target the role of its first entry. With any, the group's query
    /// ([`crate::map::spatial::Spatial::aura_lists`] around the object, for its master's radii,
    /// of its player) lists the objects near it by relation, and each live object of type 70 and
    /// up but itself joins each part whose targets name its class (plus 900) or else its master:
    /// a powered one the part's list, a counted one its count. A part with some gives its
    /// amount ([`aura_amount`]) to each listed object in turn, or with
    /// [`CombatAbility::AURA_ON_SELF`] to itself ([`World::apply_powerup`], the source the
    /// object's master).
    ///
    /// # Errors
    ///
    /// A slot the player's list does not hold, a master a player has not got, or what the query
    /// or a powerup does not model.
    fn run_aura(
        &mut self,
        (player, index): (PlayerId, ListIndex),
        group: &AuraGroup,
    ) -> GenieResult<()> {
        let unit = self.players.live(player, index)?;
        let (id, position) = (unit.id, unit.position());
        let master = unit.master(self.master_tables(), player)?;
        let (source, size) = (master.id, master.radius);
        let on_self = master
            .combat_part()
            .combat_ability
            .contains(CombatAbility::AURA_ON_SELF);
        let ability = self.parts.combat_of(id).ability;
        let attributes = &self.players.player(player)?.attributes;
        let targets: Vec<PartTargets> = group
            .parts
            .iter()
            .map(|part| {
                let mut targets = PartTargets::new();
                let working = part
                    .entries
                    .iter()
                    .filter(|entry| entry.works(attributes, ability, self.world_time));
                for entry in working {
                    if role_of(&targets, entry.target).is_none() {
                        targets.push((entry.target, entry.role));
                    }
                }
                targets
            })
            .collect();
        if targets.iter().all(Vec::is_empty) {
            return Ok(());
        }
        let relations = self.spatial.registry_kinds(player)?;
        let query = VisibleQuery {
            player,
            position: position.ground(),
            radius: group.area.radius,
            shape: group.area.shape,
            classes: &group.classes,
            relations: &relations,
            size,
            wanted: group.area.relations,
        };
        let lists = self
            .spatial
            .aura_lists(query, self.model.tuning.registry)?
            .clone();
        let mut powered: Vec<Vec<ObjectId>> = vec![Vec::new(); targets.len()];
        let mut counted: Vec<AuraCount> = vec![AuraCount::ZERO; targets.len()];
        for record in lists.iter().flat_map(|(_, records)| records) {
            let Some((found_player, found)) =
                self.object(record.id).filter(|&(_, found)| found.id != id)
            else {
                continue;
            };
            let found_master = found.master(self.master_tables(), found_player)?;
            if found_master.kind < MasterKind::COMBAT || found.life.state != ObjectState::Alive {
                continue;
            }
            let by_class = ClassOrMaster::of_class(found_master.class);
            let by_master = ClassOrMaster::of_master(found_master.id);
            for ((part_targets, list), count) in targets.iter().zip(&mut powered).zip(&mut counted)
            {
                match role_of(part_targets, by_class).or_else(|| role_of(part_targets, by_master)) {
                    Some(AuraRole::Powered) => list.push(found.id),
                    Some(AuraRole::Counted) => *count = count.counted(),
                    None => {}
                }
            }
        }
        for ((part, list), count) in group.parts.iter().zip(&powered).zip(&counted) {
            let objects = count.with_powered(list.len());
            if objects.is_none() {
                continue;
            }
            let powerup = Powerup {
                attribute: part.powerup.attribute,
                source,
                amount: aura_amount(objects, part.powerup),
                share: part.powerup.share,
            };
            if on_self {
                self.apply_powerup(id, powerup)?;
            } else {
                for &target in list {
                    self.apply_powerup(target, powerup)?;
                }
            }
        }
        Ok(())
    }
}
