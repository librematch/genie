//! The income: the trickles, the mills' food, the attribute adds and their sharing, and the
//! update's tail.

use crate::GenieError;
use crate::map::Tile;
use crate::math::fmath::at_most_or_unordered;
use crate::objects::masters::ids;
use crate::objects::masters::master_type::BUILDING;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::players::ResourceStat;
use crate::players::attribute::{self, AttributeId, RESOURCES};
use crate::players::tracker::ListKey;
use crate::players::world_player::{EARLY_SCOUTING_MS, LIVESTOCK_SCALE, MINUTE, SECOND, get, set};
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

impl World {
    /// The trickles of player `p`: each of attributes 230 to 233 above 0 (`COMISS` with 0,
    /// `JBE` skips a NaN) adds its sixtieth to food, wood, stone and gold in turn, through the
    /// attribute add with its statistic and its sharing ([`World::add_resource`] with 1 and 1).
    pub fn trickles(&mut self, p: PlayerId) -> Result<(), GenieError> {
        for kind in RESOURCES {
            let rate = get(
                &self.players.player(p)?.attributes,
                attribute::TRICKLES.offset(kind.index())?,
            );
            if at_most_or_unordered(rate.get(), 0.0) {
                continue;
            }
            self.add_resource(p, kind, rate / MINUTE, ResourceStat::GATHERED, true)?;
        }
        Ok(())
    }

    /// The food of player `p`'s mills' livestock: over the ids its tracker files under the mill
    /// (master 68, [`crate::players::tracker::Tracker::list`]), each object the world holds whose type is
    /// a building's sums its livestock (`mLivestockSummation`, `+0xc28`, in the list's order from
    /// 0); food grows by attribute 254 over 60 times (`time` times logf(sum / 200 + 1))
    /// ([`crate::math::fmath::log`]), with its statistic and its sharing ([`World::add_resource`]),
    /// also when that is 0. `time` is the update time before the loop's step.
    pub fn mill_food(&mut self, p: PlayerId, time: Seconds) -> Result<(), GenieError> {
        let mut sum = 0.0;
        for &id in self
            .players
            .player(p)?
            .tracker
            .list(ListKey::master(ids::MILL))
        {
            let Some((owner, unit)) = self.object(id) else {
                continue;
            };
            let building = unit
                .master(&self.masters, owner)
                // The buildings' type (the object's `+0xf`, `'P'`).
                .is_ok_and(|m| m.kind == BUILDING);
            if building {
                sum += unit.livestock_summation.get();
            }
        }
        let grows = crate::math::fmath::log(sum / LIVESTOCK_SCALE + SECOND);
        let rate = get(&self.players.player(p)?.attributes, attribute::MILL_FOOD).get() / MINUTE;
        self.add_resource(
            p,
            attribute::FOOD,
            Amount::new(rate * (time * grows)),
            ResourceStat::GATHERED,
            true,
        )?;
        Ok(())
    }

    /// The world player's attribute add (slot 0xf8, `FUN_141616820`) of `amount` to resource
    /// `kind` (food, wood, stone, gold: 0 to 3) of player `p`, with `stat` (its fourth argument)
    /// and `share` (its fifth): with `stat` 1 the gathered total grows first (166 to 168; gold:
    /// any `stat` but 0 adds to attribute 49 unchecked, then 1 adds to 169), then the resource
    /// ([`World::add_attribute_base`]). The resource panel's event when a resource crosses 100
    /// (`FUN_140c898a0` 0x80) is the interface's. The kinds 50, 183 and 196 and the others are
    /// not resources: their branches are [`crate::world::Player::add_attribute`]'s.
    pub fn add_resource(
        &mut self,
        p: PlayerId,
        kind: AttributeId,
        amount: Amount,
        stat: ResourceStat,
        share: bool,
    ) -> Result<(), GenieError> {
        if kind == attribute::GOLD {
            if stat.counts_gold() {
                let a = &mut self.players.player_mut(p)?.attributes;
                let counter = get(a, attribute::GOLD_COUNTER);
                set(a, attribute::GOLD_COUNTER, amount + counter);
                if stat.gathered() {
                    self.add_attribute_base(
                        p,
                        attribute::GOLD_GATHERED,
                        amount,
                        ResourceStat::GATHERED,
                        share,
                    )?;
                }
            }
        } else if stat.gathered() {
            self.add_attribute_base(
                p,
                attribute::GATHERED.offset(kind.index())?,
                amount,
                ResourceStat::GATHERED,
                share,
            )?;
        }
        self.add_attribute_base(p, kind, amount, stat, share)?;
        Ok(())
    }

    /// The base player's attribute add (`FUN_14165d040`) of `amount` to attribute `kind` of
    /// player `p`, for a kind out of 0 to the attribute count nothing: the attribute takes
    /// `amount` plus its value. A computer player's resource fills its escrow: an `amount` above
    /// 0 with a share above 0 (`COMISS`, `JBE`) adds the share times it; an escrow above the
    /// resource (`JBE` skips a NaN) falls to it, or to 0 when the resource is below 0. With
    /// `share`, a player who shares its gathered resources (`+0x1426`,
    /// [`crate::world::Player::sharing`]) gives each resource to the players, in order, it counts
    /// as allies and who count it as theirs ([`World::shares_with`]): their slot 0xf8 with `stat`
    /// and no sharing. Attribute 11's peak (`+0x6fc`, unsaved) is not held; no resource add
    /// reaches it.
    pub(super) fn add_attribute_base(
        &mut self,
        p: PlayerId,
        kind: AttributeId,
        amount: Amount,
        stat: ResourceStat,
        share: bool,
    ) -> Result<(), GenieError> {
        let player = self.players.player_mut(p)?;
        let k = kind.index();
        if player.attributes.len() <= k {
            return Ok(());
        }
        *player.attributes.checked_mut(k)? += amount;
        if player.kind == PlayerType::COMPUTER && kind.is_resource() {
            let pct = *player.escrow_percentage.checked(k)?;
            if !at_most_or_unordered(amount.get(), 0.0) && !at_most_or_unordered(pct.get(), 0.0) {
                *player.escrow.checked_mut(k)? += pct.of(amount);
            }
            let held = *player.attributes.checked(k)?;
            if !at_most_or_unordered(player.escrow.checked(k)?.get(), held.get()) {
                *player.escrow.checked_mut(k)? = if at_most_or_unordered(0.0, held.get()) {
                    held
                } else {
                    Amount::ZERO
                };
            }
        }
        if share && player.sharing && kind.is_resource() {
            for q in PlayerId::each(self.players.len()) {
                if q != p && self.shares_with(p, q) {
                    self.add_resource(q, kind, amount, stat, false)?;
                }
            }
        }
        Ok(())
    }

    /// `FUN_14165d040`'s test of the players a sharing player `p` gives to: player `q` is not
    /// Gaia and one of the world's, `p` counts it as an ally (relation 0 or 4), and `q` counts
    /// `p` as one (its relation to `p` 0 or 4, `& 0xfb`). A relation 1 would count both ways
    /// while the world's treaty holds (`FUN_1415e8e10`: `world+0x820`'s `+0x10` is 1), which the
    /// slice does not model.
    #[must_use]
    pub fn shares_with(&self, p: PlayerId, q: PlayerId) -> bool {
        let n = self.players.len();
        if q == PlayerId::GAIA || n <= q.index() || p == q {
            return false;
        }
        let mine = self
            .players
            .player(p)
            .ok()
            .and_then(|pl| pl.relations.get(q).copied());
        if !mine.is_some_and(|r| r.allied(true)) {
            return false;
        }
        p != PlayerId::GAIA
            && p.index() < n
            && self
                .players
                .player(q)
                .ok()
                .and_then(|pl| pl.relations.get(p))
                .is_some_and(|r| r.allied(true))
    }

    /// `WorldPlayer::update` after `WorldPlayerBase::update`: player `p`'s scout map (`+0x1370`,
    /// once made) takes 1 on each tile its tile list (`+0x180`) holds, in order: until the world
    /// time is 5000 ms only on the tiles the player sees ([`crate::map::spatial::Spatial::scout_sight`]
    /// is 0xf), after it on all. The list's new count (`+8`, `FUN_141662720`), which it reads and
    /// clears, is the list's length: only the AI's `FUN_140b90b90`, which a playback does not
    /// run, reads it too, and the player's update ends with the list emptied (`FUN_141661030`).
    /// Gaia's update has no tail.
    pub fn world_player_tail(&mut self, p: PlayerId) -> Result<(), GenieError> {
        if self.players.player(p)?.kind == PlayerType::GAIA || self.spatial.scout_map(p).is_none() {
            return Ok(());
        }
        let Some(tiles) = self
            .spatial
            .sight
            .as_ref()
            .and_then(|v| v.new_tiles.get(p.index()))
            .cloned()
        else {
            return Ok(());
        };
        for pair in tiles {
            let (x, y) = (pair.x(), pair.y());
            let seen = self
                .spatial
                .scout_sight(p, Tile::new(x, y))
                .is_some_and(crate::map::spatial::TileSight::seen);
            if (self.world_time < EARLY_SCOUTING_MS && seen) || EARLY_SCOUTING_MS <= self.world_time
            {
                self.spatial.scout(p, Tile::new(x, y));
            }
        }
        Ok(())
    }
}
