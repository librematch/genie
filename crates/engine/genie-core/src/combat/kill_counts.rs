//! A kill's counts in its players' attributes ([`World::count_kills`]): slot 22's credits of
//! its burns, the damage's counts for its owner and `FUN_1412005b0`'s for a killer.

use crate::combat::damage::KillCount;
use crate::combat::victims::paid_cost;
use crate::construction::build::NOT_HELD;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

impl World {
    /// The counts of a kill of an object of player `player_id` (its master's class and cost
    /// `killed`), `counts` in the game's order: slot 22's credits of its burns
    /// ([`World::credit_burns`]), the damage's counts for its owner ([`World::count_owner_loss`])
    /// and `FUN_1412005b0`'s for a killer ([`World::count_kill_of`]). Not held: the killer
    /// (`+0x690`, `+0x694`), the statistics of the local player and the particles
    /// (`FUN_141158bb0`).
    ///
    /// # Errors
    ///
    /// A computer player's object ([`World::count_owner_loss`]), and [`World::count_kill_of`]'s.
    pub(crate) fn count_kills(
        &mut self,
        player_id: PlayerId,
        killed: KilledMaster,
        counts: &[KillCount],
    ) -> GenieResult<()> {
        for count in counts {
            match count {
                KillCount::Stingers(attackers) => {
                    self.credit_burns(player_id, killed, attackers)?;
                }
                &KillCount::Owner {
                    attacker_class,
                    attacker_player,
                } => {
                    self.count_owner_loss(
                        player_id,
                        killed.class,
                        attacker_class,
                        attacker_player,
                    )?;
                }
                &KillCount::Killer(killer) => self.count_kill_of(player_id, killed, killer)?,
            }
        }
        Ok(())
    }

    /// The damage's counts of a killed object of player `player_id` and its master's `class` for
    /// its owner (`FUN_141082e80` below 1 hit point), by the attacker's class and player
    /// `attacker`: its owner's computer AI is told (slot 0x2f0, not modelled); a villager first
    /// counts for its owner, an attacker of class 9 or 10 (an animal) in attribute 227, and by the
    /// attacker's player type a human's in 229, Gaia's in 226, a computer's in 228.
    ///
    /// # Errors
    ///
    /// A computer player's object (its AI's slot 0x2f0), and a player the world does not hold.
    pub(crate) fn count_owner_loss(
        &mut self,
        player_id: PlayerId,
        class: ClassId,
        attacker_class: ClassId,
        attacker_player: PlayerId,
    ) -> GenieResult<()> {
        if self.players.player(player_id)?.kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's killed object (its AI's slot 0x2f0)",
            ));
        }
        if !self
            .model
            .roles
            .of_class(class)
            .contains(ClassRoles::VILLAGER)
        {
            return Ok(());
        }
        // the attacker's class as the hit saw it: the attacker may be out for its update
        if self
            .model
            .roles
            .of_class(attacker_class)
            .intersects(ClassRoles::PREY | ClassRoles::PREDATOR)
        {
            self.players.player_mut(player_id)?.add_attribute(
                self.model.slots.attribute.villager_killed_by_animal,
                Amount::new(1.0),
                &self.model.roles,
            );
        }
        let by_kind = match self.players.player(attacker_player)?.kind {
            PlayerKind::Human => Some(self.model.slots.attribute.villager_killed_by_kind_1),
            PlayerKind::Gaia => Some(self.model.slots.attribute.villager_killed_by_kind_2),
            PlayerKind::Computer => Some(self.model.slots.attribute.villager_killed_by_kind_3),
            PlayerKind::Phantom => None,
        };
        if let Some(kind) = by_kind {
            self.players.player_mut(player_id)?.add_attribute(
                kind,
                Amount::new(1.0),
                &self.model.roles,
            );
        }
        Ok(())
    }

    /// `FUN_1412005b0` on the object `id` killed for player `killer` (a movement damage's,
    /// [`crate::combat::abilities`]): [`World::count_kill_of`] by its master (its own one, else
    /// its player's).
    ///
    /// # Errors
    ///
    /// An object the world does not hold or whose master it lacks, and
    /// [`World::count_kill_of`]'s.
    pub(crate) fn count_kill(&mut self, id: ObjectId, killer: PlayerId) -> GenieResult<()> {
        let (player_id, unit) = self.object(id).ok_or(NOT_HELD)?;
        let killed = KilledMaster::of(unit.master(self.master_tables(), player_id)?);
        self.count_kill_of(player_id, killed, killer)
    }

    /// `FUN_1412005b0`'s counts of a kill of an object of player `player_id` (its master's class
    /// and cost `killed`) for player `ap`: unless its class is one of the non-military ones
    /// (`FUN_141188b20`) `ap` counts a kill (attribute 20) and `player_id` a loss (154); `ap`
    /// counts it against `player_id` (300 + `player_id`) and `player_id` against `ap` (325 + `ap`);
    /// both count its paid cost (170 and 152, and 400 + `player_id`).
    ///
    /// # Errors
    ///
    /// A player the world does not hold, and a count by player whose attribute number runs past
    /// the attributes' range ([`AttributeId::offset`]).
    ///
    /// [`AttributeId::offset`]: crate::players::attribute::AttributeId::offset
    pub(crate) fn count_kill_of(
        &mut self,
        player_id: PlayerId,
        killed: KilledMaster,
        ap: PlayerId,
    ) -> GenieResult<()> {
        if !self
            .model
            .roles
            .of_class(killed.class)
            .contains(ClassRoles::PEACEFUL)
        {
            self.players.player_mut(ap)?.add_attribute(
                self.model.slots.attribute.kills,
                Amount::new(1.0),
                &self.model.roles,
            );
            self.players.player_mut(player_id)?.add_attribute(
                self.model.slots.attribute.kills_other,
                Amount::new(1.0),
                &self.model.roles,
            );
        }
        self.players.player_mut(ap)?.add_attribute(
            self.model
                .slots
                .attribute
                .kills_of
                .offset(player_id.index())?,
            Amount::new(1.0),
            &self.model.roles,
        );
        self.players.player_mut(player_id)?.add_attribute(
            self.model.slots.attribute.killed_by.offset(ap.index())?,
            Amount::new(1.0),
            &self.model.roles,
        );
        let cost = killed.cost;
        self.players.player_mut(ap)?.add_attribute(
            self.model.slots.attribute.kills_value,
            cost,
            &self.model.roles,
        );
        self.players.player_mut(player_id)?.add_attribute(
            self.model.slots.attribute.kills_value_other,
            cost,
            &self.model.roles,
        );
        self.players.player_mut(ap)?.add_attribute(
            self.model
                .slots
                .attribute
                .kills_value_of
                .offset(player_id.index())?,
            cost,
            &self.model.roles,
        );
        Ok(())
    }
}

/// What a kill's counts read of the killed object's master ([`World::count_kills`]), read at
/// the kill: its class and its paid cost ([`paid_cost`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct KilledMaster {
    /// Its class.
    pub(crate) class: ClassId,
    /// Its paid cost.
    pub(crate) cost: Amount,
}

impl KilledMaster {
    /// What the counts read of `master`.
    pub(crate) fn of(master: &Master) -> Self {
        Self {
            class: master.class,
            cost: paid_cost(master),
        }
    }
}

#[cfg(test)]
mod tests;
