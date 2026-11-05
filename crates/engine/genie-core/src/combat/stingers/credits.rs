//! Slot 22's kill credits (`FUN_141205c80` below 1 hit point): an object that dies with stingers
//! counts its kill for the players of the attackers whose stingers burn it.

use crate::GenieResult;
use crate::combat::KillState;
use crate::combat::damage::{KillCount, Victim};
use crate::combat::kill_counts::KilledMaster;
use crate::combat::powerup::PowerupAttribute;
use crate::combat::stingers::ActiveStinger;
use crate::math::fmath::below_or_unordered;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::layers::Combat;
use crate::players::PlayerId;
use crate::world::World;

/// The kinds of the stingers whose attackers slot 22 credits (`0x6d`, `0x78`): the regeneration
/// rate and the regeneration as a share of the hit points, the burns.
pub(crate) const BURNS: [PowerupAttribute; 2] = [
    PowerupAttribute::REGENERATION_RATE,
    PowerupAttribute::REGENERATION_HP_PERCENT,
];

/// What slot 22's kill credits read of an object's stingers (`+0xb58`): whether it has any, and
/// the attackers (`+0x10`) of its burns ([`BURNS`]) in the list's order. An ended stinger counts
/// while the list holds it: the stinger update erases it later
/// ([`World::object_stingers`](crate::world::World::object_stingers)).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StingerAttackers {
    /// The list holds a stinger of any kind (`+0xb58` to `+0xb60` not empty).
    pub(crate) held: bool,
    /// The attackers of its burns, in the list's order.
    pub(crate) burning: Vec<Option<ObjectId>>,
}

impl StingerAttackers {
    /// What slot 22 reads of the stingers `stingers`.
    pub(crate) fn of(stingers: &[ActiveStinger]) -> Self {
        Self {
            held: !stingers.is_empty(),
            burning: stingers
                .iter()
                .filter(|stinger| BURNS.contains(&stinger.kind))
                .map(|stinger| stinger.attacker)
                .collect(),
        }
    }
}

/// Slot 22's kill credits on a hit's `victim` after its loss (`FUN_141205c80`): one left below 1
/// hit point (`COMISS hp, 1` / `JC`: a NaN too) whose kill is not counted (`+0x954`) and that has
/// stingers (those it had and those the hit handed it, [`Victim::stung`], in the order of the
/// pushes) records its burns' attackers ([`KillCount::Stingers`]) and has its kill counted, with
/// burns or without.
pub(crate) fn credit(victim: &mut Victim) {
    let held = victim.stinger_attackers.held || !victim.stung.is_empty();
    if !below_or_unordered(victim.hp.get(), 1.0) || victim.kill_state.is_counted() || !held {
        return;
    }
    let handed = victim
        .stung
        .iter()
        .filter(|(_, task)| BURNS.contains(&task.kind))
        .map(|&(attacker, _)| Some(attacker));
    let burning = victim
        .stinger_attackers
        .burning
        .iter()
        .copied()
        .chain(handed)
        .collect();
    victim.kill_counts.push(KillCount::Stingers(burning));
    victim.kill_state = KillState::Counted;
}

/// Slot 22's kill credits on an object's combat layer `combat` at a loss of its own update (its
/// regeneration's) that left it at `hp`: [`credit`] on the stingers it holds.
pub(crate) fn credit_combat(combat: &mut Combat, hp: HitPoints) -> Option<KillCount> {
    if !below_or_unordered(hp.get(), 1.0)
        || combat.kill_state.is_counted()
        || combat.active_stingers.is_empty()
    {
        return None;
    }
    combat.kill_state = KillState::Counted;
    Some(KillCount::Stingers(
        StingerAttackers::of(&combat.active_stingers).burning,
    ))
}

impl World {
    /// The players slot 22 credits for the burns' `attackers` (`FUN_141205c80`): the world's object
    /// of each (`FUN_1415417a0`, [`World::object`]; none for one it no longer holds), its player,
    /// each once. The game gathers the objects and their players in two hash sets keyed by their
    /// addresses (`FUN_1411e9f80`, `FUN_1411e9c90`), an order no run repeats; the objects' walk
    /// only tells the local player's event log (`FUN_140a4d670`), and the outcome does not depend
    /// on the players' order (`FUN_1412005b0` adds 1 or the same cost to each attribute two credits
    /// share, and the others are each player's own), so genie walks the players in the order they
    /// first appear (`CODING_STYLE.md` section 12). The write-back resolves them right after the
    /// hit: nothing between changes an attacker's player but a livestock's change to Gaia, and
    /// livestock hands no stinger.
    pub(crate) fn burning_players(&self, attackers: &[Option<ObjectId>]) -> Vec<PlayerId> {
        let mut players: Vec<PlayerId> = Vec::new();
        for &attacker in attackers {
            if let Some((player_id, _)) = self.object(attacker)
                && !players.contains(&player_id)
            {
                players.push(player_id);
            }
        }
        players
    }

    /// Slot 22's credits of a kill of an object of player `player_id` (its master's class and cost
    /// `killed`) for the burns' `attackers`: `FUN_1412005b0` for each player
    /// ([`World::burning_players`], [`World::count_kill_of`]).
    ///
    /// # Errors
    ///
    /// An attribute a player lacks.
    pub(crate) fn credit_burns(
        &mut self,
        player_id: PlayerId,
        killed: KilledMaster,
        attackers: &[Option<ObjectId>],
    ) -> GenieResult<()> {
        for killer in self.burning_players(attackers) {
            self.count_kill_of(player_id, killed, killer)?;
        }
        Ok(())
    }
}
