//! The income: the trickles, the mills' food, a monastery's relic goods, the attribute adds
//! and their sharing, and the update's tail.

use crate::GenieResult;
use crate::map::Tile;
use crate::map::spatial::TileSight;
use crate::math::fmath::at_most_or_unordered;
use crate::math::fmath::log;
use crate::objects::Relics;
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
use crate::world::ClockSeconds;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::slots::PlayerSlotsExt;

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

    /// A monastery's relic goods for `seconds` whole seconds (`TRIBE_Building_Object::update`,
    /// `0x1411c9000`, a monastery in state 2 whose player plays): each of player `player_id`'s
    /// relic rates over 60 (`DIVSS`, then `MULSS` by `relics`' relic seconds,
    /// [`Relics::relic_seconds`]: attributes 225, 264, 265 and 191) gives food, wood, stone and
    /// gold in turn through slot 0xf8 with 1 and 1 ([`World::add_resource`]), the gold then
    /// attribute 100 too. With no relics each adds 0.0. The local player's statistics
    /// (`FUN_14097daf0` four times) are not held.
    ///
    /// # Errors
    ///
    /// No such player, and the adds'.
    pub(crate) fn relic_goods(
        &mut self,
        player_id: PlayerId,
        relics: Relics,
        seconds: ClockSeconds,
    ) -> GenieResult<()> {
        let relic_seconds = relics.relic_seconds(seconds);
        let slots = self.model.slots.attribute;
        let attributes = &self.players.player(player_id)?.attributes;
        let goods = [
            slots.relic_food_rate,
            slots.relic_wood_rate,
            slots.relic_stone_rate,
            slots.relic_gold_rate,
        ]
        .map(|rate| get(attributes, rate) / MINUTE * relic_seconds);
        for (kind, amount) in AttributeId::resources().into_iter().zip(goods) {
            self.add_resource(
                player_id,
                kind,
                amount,
                ResourceStat::Gathered,
                ResourceShare::Shared,
            )?;
        }
        let [.., gold] = goods;
        self.add_resource(
            player_id,
            slots.relic_income_summation,
            gold,
            ResourceStat::Gathered,
            ResourceShare::Shared,
        )
    }

    /// The world player's attribute add, slot 0xf8 (`FUN_141616820`), of `amount` to attribute
    /// `kind` of player `player_id`, with `stat` (its fourth argument) and `share` (its fifth,
    /// [`ResourceShare`]). Its branches by the kind, in the game's order:
    ///
    /// - gold ([`AttributeRoles::GOLD`], the game's 3): with any `stat` but 0 its counter (49)
    ///   grows unchecked, with 1 its gathered total (169) through the base add
    ///   ([`World::gold_counts`]);
    /// - the spread attribute (196, [`AttributeRoles::SPREAD_ATTRIBUTE`]): above 0 (`COMISS`, `JA`:
    ///   not a NaN) the add does nothing at all; else the other players' at most 0 take `amount`
    ///   too ([`World::spread_attribute_add`]);
    /// - food, wood and stone (0 to 2): with `stat` 1 their gathered totals (166 to 168) grow
    ///   through the base add;
    /// - the allied vision (50, [`AttributeRoles::ALLIED_VISION`]) turns on first
    ///   ([`World::allied_vision_on`]);
    /// - the shared vision (183, [`AttributeRoles::SHARED_VISION`]) rebuilds the masks first (the
    ///   world's slot 0x78).
    ///
    /// Then the base add of the attribute itself ([`World::add_attribute_base`]). The interface's
    /// events (`FUN_140c898a0` 0x94 and 0x8b, and the resource panel's 0x80 when a resource
    /// crosses 100) are not the simulation's.
    ///
    /// # Errors
    ///
    /// A player the world does not hold, and the vision's rebuild.
    pub fn add_resource(
        &mut self,
        player_id: PlayerId,
        kind: AttributeId,
        amount: Amount,
        stat: ResourceStat,
        share: ResourceShare,
    ) -> GenieResult<()> {
        let held = self.model.roles.of_attribute(kind);
        if held.contains(AttributeRoles::GOLD) {
            self.gold_counts(player_id, amount, stat, share)?;
        } else if held.contains(AttributeRoles::SPREAD_ATTRIBUTE) {
            let spread = self.model.slots.attribute.spread_attribute;
            if Amount::ZERO < get(&self.players.player(player_id)?.attributes, spread) {
                return Ok(());
            }
            self.spread_attribute_add(player_id, amount)?;
        } else if kind.is_resource() {
            // the game's kinds 0, 1 and 2
            if stat.gathered() {
                self.add_attribute_base(
                    player_id,
                    self.model.slots.attribute.gathered.offset(kind.index())?,
                    amount,
                    ResourceStat::Gathered,
                    share,
                )?;
            }
        } else if held.contains(AttributeRoles::ALLIED_VISION) {
            self.allied_vision_on(player_id)?;
        } else if held.contains(AttributeRoles::SHARED_VISION) {
            self.rebuild_vision()?;
        }
        self.add_attribute_base(player_id, kind, amount, stat, share)
    }

    /// Slot 0xf8 with 0 and 1 ([`World::add_resource`] with [`ResourceStat::None`] and
    /// [`ResourceShare::Shared`]) of `amount` to attribute `kind` of player `player_id`: the
    /// call of a storage's entries (`FUN_141193860`, `FUN_1411a56c0`, `FUN_1411e63f0`), a
    /// foundation's refund (`FUN_1411ccbb0`), a tribute (`FUN_140e7d040`), a Make's food
    /// (`FUN_140a99e30`) and a technology's resource add (`FUN_141661160`).
    ///
    /// # Errors
    ///
    /// [`World::add_resource`]'s.
    pub(crate) fn add_player_attribute(
        &mut self,
        player_id: PlayerId,
        kind: AttributeId,
        amount: Amount,
    ) -> GenieResult<()> {
        self.add_resource(
            player_id,
            kind,
            amount,
            ResourceStat::None,
            ResourceShare::Shared,
        )
    }

    /// Slot 0xf8 with 1 and 1 ([`World::add_resource`] with [`ResourceStat::Gathered`] and
    /// [`ResourceShare::Shared`]) on a unit's load of `amount` of `kind`, which player `player_id`
    /// gains with its gathered total: a gatherer's, a hunter's or a builder's drop
    /// (`FUN_140aadc50`), the load a unit takes into a building (`FUN_140a867b0`), a villager's
    /// at its upgrade, a generation's and a farm's bonus (`FUN_1411c66a0`,
    /// [`World::give_farm_bonus`]). The statistics (slot 0xb8) and the local player's tracker
    /// (`FUN_14097daf0`) are not held.
    ///
    /// # Errors
    ///
    /// [`World::add_resource`]'s.
    pub(crate) fn give_load(
        &mut self,
        player_id: PlayerId,
        kind: AttributeId,
        amount: Amount,
    ) -> GenieResult<()> {
        self.add_resource(
            player_id,
            kind,
            amount,
            ResourceStat::Gathered,
            ResourceShare::Shared,
        )
    }

    /// Slot 0xf8's gold branch for player `player_id`'s add of `amount`: with any `stat` but 0 its
    /// gold counter (49) grows unchecked (`+0x188`, straight into it), with 1 its gathered
    /// gold (169) through the base add with `share` ([`World::add_attribute_base`]).
    fn gold_counts(
        &mut self,
        player_id: PlayerId,
        amount: Amount,
        stat: ResourceStat,
        share: ResourceShare,
    ) -> GenieResult<()> {
        if !stat.counts_gold() {
            return Ok(());
        }
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
        Ok(())
    }

    /// Slot 0xf8's spread of attribute 196 from player `player_id`, whose own is at most 0: each
    /// other player from 1 on whose attribute 196 is at most 0 (`COMISS`, `JC`: not a NaN) takes
    /// `amount` plus it, straight into it.
    fn spread_attribute_add(&mut self, player_id: PlayerId, amount: Amount) -> GenieResult<()> {
        let spread = self.model.slots.attribute.spread_attribute;
        for other in PlayerId::each(self.players.len()).skip(1) {
            if other == player_id {
                continue;
            }
            let amounts = &mut self.players.player_mut(other)?.attributes;
            let theirs = get(amounts, spread);
            if theirs <= Amount::ZERO {
                set(amounts, spread, amount + theirs);
            }
        }
        Ok(())
    }

    /// The base player's attribute add (`FUN_14165d040`) of `amount` to attribute `kind` of player
    /// `owner`: for a kind within the player's count the player's own add with its escrow
    /// ([`crate::world::Player::add_attribute`]), and with [`ResourceShare::Shared`], a player who
    /// shares its gathered resources (`+0x1426`, [`crate::world::AllySharing::RESOURCES`]) gives
    /// each resource to the players, in order, it counts as allies and who count it as theirs
    /// ([`World::shares_with`]): their slot 0xf8 with `stat` and no sharing. A kind past the
    /// count does nothing, the sharing neither.
    pub(super) fn add_attribute_base(
        &mut self,
        owner: PlayerId,
        kind: AttributeId,
        amount: Amount,
        stat: ResourceStat,
        share: ResourceShare,
    ) -> GenieResult<()> {
        let player = self.players.player_mut(owner)?;
        if player.attribute(kind).is_none() {
            return Ok(());
        }
        player.add_attribute(kind, amount);
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
