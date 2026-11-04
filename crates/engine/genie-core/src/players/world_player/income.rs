//! The income: the trickles, the mills' food, the attribute adds and their sharing, and the
//! update's tail.

use crate::GenieResult;
use crate::map::Tile;
use crate::map::spatial::TileSight;
use crate::math::fmath::at_most_or_unordered;
use crate::math::fmath::log;
use crate::objects::masters::MasterKind;
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::ResourceStat;
use crate::players::attribute::AttributeId;
use crate::players::tracker::ListKey;
use crate::players::world_player::{EARLY_SCOUTING_MS, MINUTE, SECOND, get, set};
use crate::world::AllySharing;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};

/// Whether a resource add (slot 0xf8's fifth argument) goes on to the players its player
/// shares its gathered resources with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceShare {
    /// It does (1).
    Shared,
    /// It does not (0): their own shares, a market's trade.
    Unshared,
}

impl World {
    /// The trickles of player `player_id`: each of attributes 230 to 233 above 0 (`COMISS` with 0,
    /// `JBE` skips a NaN) adds its sixtieth to food, wood, stone and gold in turn, through the
    /// attribute add with its statistic and its sharing ([`World::add_resource`] with 1 and 1).
    pub fn trickles(&mut self, player_id: PlayerId) -> GenieResult<()> {
        for kind in AttributeId::resources() {
            let rate = get(
                &self.players.player(player_id)?.attributes,
                self.model.slots.attribute.trickles.offset(kind.index())?,
            );
            if at_most_or_unordered(rate.get(), 0.0) {
                continue;
            }
            self.add_resource(
                player_id,
                kind,
                rate / MINUTE,
                ResourceStat::Gathered,
                ResourceShare::Shared,
            )?;
        }
        Ok(())
    }

    /// The food of player `player_id`'s mills' livestock: over the ids its tracker files under the
    /// mill (master 68, [`crate::players::tracker::Tracker::list`]), each object the world holds
    /// whose type is a building's sums its livestock (`mLivestockSummation`, `+0xc28`, in the
    /// list's order from 0); food grows by attribute 254 over 60 times (`time` times logf(sum /
    /// 200 + 1)) ([`crate::math::fmath::log`]), with its statistic and its sharing
    /// ([`World::add_resource`]), also when that is 0. `time` is the update time before the loop's
    /// step.
    pub fn mill_food(&mut self, player_id: PlayerId, time: Seconds) -> GenieResult<()> {
        let mut sum = 0.0;
        for &id in self
            .players
            .player(player_id)?
            .tracker
            .list(ListKey::master(self.model.slots.master.mill))
        {
            let Some((owner, unit)) = self.object(id) else {
                continue;
            };
            let building = unit
                .master(self.master_tables(), owner)
                // The buildings' type (the object's `+0xf`, `'P'`).
                .is_ok_and(|master| master.kind == MasterKind::BUILDING);
            if building {
                sum += self.parts.building_of(unit.id).livestock_summation.get();
            }
        }
        let grows = log(sum / self.model.tuning.world_player.livestock_scale + SECOND);
        let rate = get(
            &self.players.player(player_id)?.attributes,
            self.model.slots.attribute.mill_food,
        )
        .get()
            / MINUTE;
        self.add_resource(
            player_id,
            self.model.slots.attribute.food,
            Amount::new(rate * (time * grows)),
            ResourceStat::Gathered,
            ResourceShare::Shared,
        )?;
        Ok(())
    }

    /// The world player's attribute add (slot 0xf8, `FUN_141616820`) of `amount` to resource `kind`
    /// (food, wood, stone, gold: 0 to 3) of player `player_id`, with `stat` (its fourth argument)
    /// and `share` (its fifth, [`ResourceShare`]): with `stat` 1 the gathered total grows first
    /// (166 to 168; gold: any `stat` but 0 adds to attribute 49 unchecked, then 1 adds to 169),
    /// then the resource ([`World::add_attribute_base`]). The resource panel's event when a
    /// resource crosses 100 (`FUN_140c898a0` 0x80) is the interface's. The kinds 50, 183 and 196
    /// and the others are not resources: their branches are
    /// [`crate::world::Player::add_attribute`]'s.
    pub fn add_resource(
        &mut self,
        player_id: PlayerId,
        kind: AttributeId,
        amount: Amount,
        stat: ResourceStat,
        share: ResourceShare,
    ) -> GenieResult<()> {
        if self
            .model
            .roles
            .of_attribute(kind)
            .contains(AttributeRoles::GOLD)
        {
            if stat.counts_gold() {
                let amounts = &mut self.players.player_mut(player_id)?.attributes;
                let counter = get(amounts, self.model.slots.attribute.gold_counter);
                set(
                    amounts,
                    self.model.slots.attribute.gold_counter,
                    amount + counter,
                );
                if stat.gathered() {
                    self.add_attribute_base(
                        player_id,
                        self.model.slots.attribute.gold_gathered,
                        amount,
                        ResourceStat::Gathered,
                        share,
                    )?;
                }
            }
        } else if stat.gathered() {
            self.add_attribute_base(
                player_id,
                self.model.slots.attribute.gathered.offset(kind.index())?,
                amount,
                ResourceStat::Gathered,
                share,
            )?;
        }
        self.add_attribute_base(player_id, kind, amount, stat, share)?;
        Ok(())
    }

    /// The base player's attribute add (`FUN_14165d040`) of `amount` to attribute `kind` of player
    /// `owner`, for a kind out of 0 to the attribute count nothing: the attribute takes `amount`
    /// plus its value. A computer player's resource fills its escrow: an `amount` above 0 with a
    /// share above 0 (`COMISS`, `JBE`) adds the share times it; an escrow above the resource (`JBE`
    /// skips a NaN) falls to it, or to 0 when the resource is below 0. [`ResourceShare::Shared`], a
    /// player who shares its gathered resources (`+0x1426`,
    /// [`crate::world::AllySharing::RESOURCES`]) gives each resource to the players, in order, it
    /// counts as allies and who count it as theirs ([`World::shares_with`]): their slot 0xf8 with
    /// `stat` and no sharing. Attribute 11's peak (`+0x6fc`, unsaved) is not held; no resource add
    /// reaches it.
    pub(super) fn add_attribute_base(
        &mut self,
        owner: PlayerId,
        kind: AttributeId,
        amount: Amount,
        stat: ResourceStat,
        share: ResourceShare,
    ) -> GenieResult<()> {
        let player = self.players.player_mut(owner)?;
        let index = kind.index();
        if player.attributes.len() <= index {
            return Ok(());
        }
        *player.attributes.checked_mut(index)? += amount;
        if player.kind == PlayerKind::Computer && kind.is_resource() {
            let pct = *player.escrow_percentage.checked(index)?;
            if !at_most_or_unordered(amount.get(), 0.0) && !at_most_or_unordered(pct.get(), 0.0) {
                *player.escrow.checked_mut(index)? += pct.of(amount);
            }
            let held = *player.attributes.checked(index)?;
            if !at_most_or_unordered(player.escrow.checked(index)?.get(), held.get()) {
                *player.escrow.checked_mut(index)? = if at_most_or_unordered(0.0, held.get()) {
                    held
                } else {
                    Amount::ZERO
                };
            }
        }
        if share == ResourceShare::Shared
            && player.ally_sharing.contains(AllySharing::RESOURCES)
            && kind.is_resource()
        {
            for player_id in PlayerId::each(self.players.len()) {
                if player_id != owner && self.shares_with(owner, player_id) {
                    self.add_resource(player_id, kind, amount, stat, ResourceShare::Unshared)?;
                }
            }
        }
        Ok(())
    }

    /// `FUN_14165d040`'s test of the players a sharing player `own` gives to: player `other` is not
    /// Gaia and one of the world's, `own` counts it as an ally (relation 0 or 4), and `other`
    /// counts `own` as one (its relation to `own` 0 or 4, `& 0xfb`). A relation 1 would count both
    /// ways while the world's treaty holds (`FUN_1415e8e10`: `world+0x820`'s `+0x10` is 1), which
    /// the slice does not model.
    #[must_use]
    pub fn shares_with(&self, own: PlayerId, other: PlayerId) -> bool {
        let count = self.players.len();
        if other == PlayerId::GAIA || count <= other.index() || own == other {
            return false;
        }
        let mine = self
            .players
            .player(own)
            .ok()
            .and_then(|pl| pl.relations.get(other).copied());
        if !mine.is_some_and(|relation| relation.allied(AllyTest::WithRelation4)) {
            return false;
        }
        own != PlayerId::GAIA
            && own.index() < count
            && self
                .players
                .player(other)
                .ok()
                .and_then(|pl| pl.relations.get(own))
                .is_some_and(|relation| relation.allied(AllyTest::WithRelation4))
    }

    /// `WorldPlayer::update` after `WorldPlayerBase::update`: player `player_id`'s scout map
    /// (`+0x1370`, once made) takes 1 on each tile its tile list (`+0x180`) holds, in order: until
    /// the world time is 5000 ms only on the tiles the player sees
    /// ([`crate::map::spatial::Spatial::scout_sight`] is 0xf), after it on all. The list's new
    /// count (`+8`, `FUN_141662720`), which it reads and clears, is the list's length: only the
    /// AI's `FUN_140b90b90`, which a playback does not run, reads it too, and the player's update
    /// ends with the list emptied (`FUN_141661030`). Gaia's update has no tail.
    pub fn world_player_tail(&mut self, player_id: PlayerId) -> GenieResult<()> {
        if self.players.player(player_id)?.kind == PlayerKind::Gaia
            || self.spatial.scout_map(player_id).is_none()
        {
            return Ok(());
        }
        let Some(tiles) = self
            .spatial
            .sight
            .as_ref()
            .and_then(|visibility| visibility.new_tiles.get(player_id.index()))
            .cloned()
        else {
            return Ok(());
        };
        for pair in tiles {
            let (x, y) = (pair.x(), pair.y());
            let seen = self
                .spatial
                .scout_sight(player_id, Tile::new(x, y))
                .is_some_and(TileSight::seen);
            if (self.world_time < EARLY_SCOUTING_MS && seen) || EARLY_SCOUTING_MS <= self.world_time
            {
                self.spatial.scout(player_id, Tile::new(x, y));
            }
        }
        Ok(())
    }
}
