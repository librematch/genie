//! The mill's and the dock's queues: the farms and pastures (`farm_queue_count`, `+0x141c`) and
//! the fish traps (`fishTrapQueueCount`, `+0x1420`) a player paid for ahead, which GAME's
//! sub-commands 0xd, 0x15 and 0x11 buy and 0xe, 0x16 and 0x12 give back. Each queue holds at
//! most 40 before a purchase.
//!
//! 158041: the farms' purchase `FUN_141617130`, their return `FUN_141630f10`; the fish traps'
//! `FUN_141616540` and `FUN_1416308f0`; the payment `FUN_14162fb60`
//! ([`crate::production::pay`]) and the refund `FUN_141630770`
//! ([`crate::production::refund_master`]).

use crate::GenieError;
use crate::gathering::QueueCount;
use crate::objects::ListIndex;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::ids::FARM;
use crate::objects::masters::ids::FISH_TRAP;
use crate::objects::masters::ids::TOWN_CENTRE_621;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::production::CostCount;
use crate::production::{cost_test, pay, refund_master};
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// A queue takes a purchase only below this count (`0x28`).
pub const QUEUE_LIMIT: QueueCount = QueueCount::of_data(40);
/// The pasture a mill's queue buys (`0x761`, GAME 0x15 and 0x16).
pub const PASTURE: MasterId = MasterId::named(0x761);

/// A reseeding gatherer's farm spot of none (`DAT_14330ec20`, −1.0, read at `0x140a8aa9e`).
const NO_FARM: f32 = crate::math::image::farm_queue::NO_FARM;
/// The one farm or fish trap a reseed pays (`DAT_14330eb8c`, 1.0, read at `0x140a8ac75` and
/// `0x140a8b025`).
const ONE_FARM: CostCount = CostCount::new(crate::math::image::farm_queue::ONE_FARM);

/// Which of a player's queues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Queue {
    /// `+0x141c`: farms and pastures.
    Farms,
    /// `+0x1420`: fish traps.
    FishTraps,
}

impl World {
    /// `FUN_141617130`: player `p` buys `count` of master `master` (the farm 0x32 or the pasture
    /// 0x761) for its mill's queue ([`World::buy_ahead`]).
    pub(crate) fn queue_farms(
        &mut self,
        p: PlayerId,
        count: QueueCount,
        master: MasterId,
    ) -> Result<bool, GenieError> {
        self.buy_ahead(p, count, master, Queue::Farms)
    }

    /// `FUN_141616540`: player `p` buys `count` fish traps (199) for its dock's queue
    /// ([`World::buy_ahead`]).
    pub(crate) fn queue_fish_traps(
        &mut self,
        p: PlayerId,
        count: QueueCount,
    ) -> Result<bool, GenieError> {
        // The fish trap (master 199, its master table's `+0x638`).
        // The farm a mill's queue buys (`0x32`, GAME 0xd and 0xe).
        self.buy_ahead(p, count, FISH_TRAP, Queue::FishTraps)
    }

    /// `FUN_141630f10`: player `p` takes back `count` of master `master` from its mill's queue
    /// ([`World::give_back`]).
    pub(crate) fn unqueue_farms(
        &mut self,
        p: PlayerId,
        count: QueueCount,
        master: MasterId,
    ) -> Result<bool, GenieError> {
        self.give_back(p, count, master, Queue::Farms)
    }

    /// `FUN_1416308f0`: player `p` takes back `count` fish traps from its dock's queue
    /// ([`World::give_back`]).
    pub(crate) fn unqueue_fish_traps(
        &mut self,
        p: PlayerId,
        count: QueueCount,
    ) -> Result<bool, GenieError> {
        self.give_back(p, count, FISH_TRAP, Queue::FishTraps)
    }

    /// The purchase of `FUN_141617130` and `FUN_141616540`: with `count` above 0 and the queue
    /// below [`QUEUE_LIMIT`], from `count` down to 1 the first number the player can pay
    /// without the population's test ([`cost_test`]: a master it has of type 70 or more, an
    /// annex's costs its head's, each entry with an attribute at most what the attribute holds,
    /// `COMISS`, `JA`: a NaN passes) is paid (`FUN_14162fb60` with 1, [`pay`]) and joins the
    /// queue, which may then pass the limit. False when none can, or the payment failed.
    fn buy_ahead(
        &mut self,
        p: PlayerId,
        count: QueueCount,
        master: MasterId,
        queue: Queue,
    ) -> Result<bool, GenieError> {
        if !count.any() || QUEUE_LIMIT <= self.queued(p, queue)? {
            return Ok(false);
        }
        let Some(masters) = self.masters.of(p) else {
            return Ok(false);
        };
        let player = self.players.player_mut(p)?;
        let mut n = count;
        loop {
            if cost_test(masters, master, n.factor(), false, &*player)? {
                if !pay(masters, master, n.factor(), &mut *player)? {
                    return Ok(false);
                }
                match queue {
                    Queue::Farms => player.farm_queue += n,
                    Queue::FishTraps => player.fish_trap_queue += n,
                }
                return Ok(true);
            }
            if n <= QueueCount::ONE {
                return Ok(false);
            }
            n -= QueueCount::ONE;
        }
    }

    /// The return of `FUN_141630f10` and `FUN_1416308f0`: at most the queue's count; with one
    /// or more the queue loses them and the player gets their costs back (`FUN_141630770`,
    /// [`refund_master`]: each entry of a master of type 70 or more with an attribute and the
    /// paid flag, its amount times the count). False with none.
    fn give_back(
        &mut self,
        p: PlayerId,
        count: QueueCount,
        master: MasterId,
        queue: Queue,
    ) -> Result<bool, GenieError> {
        let n = count.min(self.queued(p, queue)?);
        if !n.any() {
            return Ok(false);
        }
        let player = self.players.player_mut(p)?;
        match queue {
            Queue::Farms => player.farm_queue -= n,
            Queue::FishTraps => player.fish_trap_queue -= n,
        }
        // the game reads the master without a test: a queue of a master the player lacks
        // cannot have been paid for
        if let Some(m) = player_master(&self.masters, p, master) {
            refund_master(m, n.get(), player)?;
        }
        Ok(true)
    }

    /// `FUN_141663f80`: player `p` can make master `master` (one it has): the master its base
    /// id names (`+0x260`; for a building heading a stack, `+0x9b8`, the head's base id, none
    /// when the player lacks the head) is available (`+0x5f0`); the town centre 621 also while the
    /// town centres standing ([`World::town_centre_count`]) are fewer than attribute 218
    /// (`CVTSI2SS`, `COMISS`). A base id the player lacks makes none (the game reads its table's
    /// slot unchecked; the `.dat`'s masters name their own).
    pub fn can_make(&self, p: PlayerId, master: MasterId) -> Result<bool, GenieError> {
        let Some(ms) = self.masters.of(p) else {
            return Ok(false);
        };
        let Some(m) = ms.get(master) else {
            return Ok(false);
        };
        let Some(based) = m.base_id.and_then(|b| ms.get(b)) else {
            return Ok(false);
        };
        let mut base = m.base_id;
        if based.kind == crate::objects::masters::master_type::BUILDING
            && let Some(head) = based.building.as_ref().and_then(|b| b.head_unit)
        {
            let Some(h) = ms.get(head) else {
                return Ok(false);
            };
            base = h.base_id;
        }
        let Some(b) = base.and_then(|b| ms.get(b)) else {
            return Ok(false);
        };
        // The farm class (49), whose masters the reseed takes (`FUN_141658b40`'s `-0xe7b`).
        // The villager's class (4, `-0xea8`), which reseeds farms, and the fishing ship's (21, `0x15`),
        // which reseeds fish traps.
        // The town centre (621, `0x26d`), whose availability also counts the town centres standing
        // against attribute 218 (`FUN_141663f80`).
        if base == Some(TOWN_CENTRE_621) {
            let limit = self
                .players
                .player(p)?
                .attribute(attribute::TOWN_CENTRE_LIMIT)
                .unwrap_or(Amount::ZERO);
            if (self.town_centre_count(p) as f32) < limit.get() {
                return Ok(true);
            }
        }
        Ok(b.available)
    }

    /// `FUN_141658b40`: the farm a reseeding villager of player `p` makes: the farm (0x32) when
    /// the player can make it ([`World::can_make`]) and it is of the farm class, else the pasture
    /// (0x761) so; else none.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub fn reseed_master(&self, p: PlayerId) -> Result<Option<MasterId>, GenieError> {
        for m in [FARM, PASTURE] {
            if self.can_make(p, m)?
                && player_master(&self.masters, p, m).is_some_and(|m| m.class == class::FARM)
            {
                return Ok(Some(m));
            }
        }
        Ok(None)
    }

    /// `FUN_140a8aa60`: player `p`'s gatherer at `idx` reseeds its farm at its farm spot
    /// (`mFarmX`, `mFarmY`; none at −1). A villager (class 4) with the automatic farm queue
    /// (`+0x1424`) or farms queued (`+0x141c`) makes the farm [`World::reseed_master`] names; any
    /// other gatherer, with the automatic fish trap queue (`+0x1425`) or fish traps queued
    /// (`+0x1420`), makes a fish trap when it is a fishing ship (class 21); else none. (For the
    /// farm the game looks the spot's terrain up, `FUN_140f690e0`, `FUN_1413fbec0`,
    /// `FUN_1413fc220`, `FUN_1413fc100`, and drops the answers: the master stays.) The master's
    /// placement test (slot 0x50 with the explored test on, `p6` 1, whatever the player; for a
    /// farm its scan passes over the gather's target and that building's annexes, `p12`,
    /// [`World::placement_code_beside`]) must pass. With the automatic queue the player pays one
    /// when it can without the population (`FUN_141618db0`, `FUN_14162fb60` with 1: [`cost_test`],
    /// [`pay`]), else it takes one from the queue, which must hold one (else the interface's
    /// event 0x7e); without the automatic queue it takes one from the queue. The master's slot
    /// 0x108 makes the foundation there ([`World::make_building`], its unit AI by
    /// [`crate::unit_ai::factory_type`]), the queue loses the one it gave (`FUN_14163c5b0`,
    /// `FUN_14163c590`: only while above 0); the target's workers (slot 0x790, a building's
    /// `+0xcb8`, which only a pasture fills: a farm's is empty, and a pasture's gather is not
    /// modelled) would build it too; then the gatherer's ignore list empties (`FUN_141197da0`)
    /// and its slot 0x128 sends it to build the foundation at the spot ([`World::build_task`],
    /// [`World::build_directly`]). True when the foundation was made.
    pub(crate) fn reseed(&mut self, p: PlayerId, idx: ListIndex) -> Result<bool, GenieError> {
        let unit = &self.players.live(p, idx)?;
        let Some(gather) = unit.actions.gather().copied() else {
            return Ok(false);
        };
        let farm = gather.farm;
        if farm.x() == NO_FARM {
            return Ok(false);
        }
        let class = unit.master(&self.masters, p)?.class;
        let Some((master, auto, queue)) = self.reseed_choice(p, class)? else {
            return Ok(false);
        };
        // the farm's scan passes over the old target and, for a building, its annexes; the fish
        // trap's over nothing
        let mut own = Vec::new();
        if queue == Queue::Farms
            && let Some(target) = gather.target
        {
            own.push(target);
            let is_building = match self.object(target) {
                Some((q, t)) => {
                    t.master(&self.masters, q)?.kind
                        == crate::objects::masters::master_type::BUILDING
                }
                None => false,
            };
            if is_building {
                let mut k = 0;
                while k < own.len() {
                    own.extend(self.spatial.annexes_of(own[k]));
                    k += 1;
                }
            }
        }
        if !self
            .placement_code_beside(p, master, farm, true, &own)?
            .is_allowed()
        {
            return Ok(false);
        }
        let Some(from_queue) = self.pay_for_reseed(p, (master, auto, queue))? else {
            return Ok(false);
        };
        let made = self.master(p, master)?;
        let ai = Ok(made.unit_ai_type(master));
        let at = farm.with_height(0.0);
        let (foundation, _) = self.make_building(p, master, ai, at)?;
        if from_queue {
            let player = self.players.player_mut(p)?;
            let count = match queue {
                Queue::Farms => &mut player.farm_queue,
                Queue::FishTraps => &mut player.fish_trap_queue,
            };
            if count.any() {
                *count -= QueueCount::ONE;
            }
        }
        let id = self.players.live(p, idx)?.id;
        let site = (p, foundation, master);
        let task = self.build_task(p, id, site, at)?;
        self.build_directly(p, idx, task, site, at)?;
        Ok(true)
    }

    /// What a reseeding gatherer of `class` makes ([`World::reseed`]): the master, whether the
    /// automatic queue pays and the queue it takes from; none when nothing is to be made.
    fn reseed_choice(
        &self,
        p: PlayerId,
        class: crate::objects::masters::ClassId,
    ) -> Result<Option<(MasterId, bool, Queue)>, GenieError> {
        let owner = self.players.player(p)?;
        let choice =
            if (!owner.auto_farm_queue && !owner.farm_queue.any()) || class != class::VILLAGER {
                if !owner.auto_fishtrap_queue && !owner.fish_trap_queue.any() {
                    return Ok(None);
                }
                if class != class::FISHING_SHIP {
                    return Ok(None);
                }
                (FISH_TRAP, owner.auto_fishtrap_queue, Queue::FishTraps)
            } else {
                let Some(master) = self.reseed_master(p)? else {
                    return Ok(None);
                };
                (master, owner.auto_farm_queue, Queue::Farms)
            };
        Ok(Some(choice))
    }

    /// The farm's or the fish trap's price ([`World::reseed`]): the player pays it when the
    /// automatic queue is on and it can, else takes one from the queue, which must hold one.
    /// None when it can do neither, else whether the queue gave it.
    fn pay_for_reseed(
        &mut self,
        p: PlayerId,
        (master, auto, queue): (MasterId, bool, Queue),
    ) -> Result<Option<bool>, GenieError> {
        let mut from_queue = !auto;
        if auto {
            let Some(masters) = self.masters.of(p) else {
                // unreachable: the gatherer's master was found in this table above
                return Ok(None);
            };
            if cost_test(masters, master, ONE_FARM, false, self.players.player(p)?)? {
                if !pay(masters, master, ONE_FARM, self.players.player_mut(p)?)? {
                    return Ok(None);
                }
            } else {
                if !self.queued(p, queue)?.any() {
                    return Ok(None);
                }
                from_queue = true;
            }
        }
        Ok(Some(from_queue))
    }

    /// State 0xf2's end (`0x140a8e1d0` case 0xf1, [`crate::gathering::gather::Next::Reseed`]) of player
    /// `p`'s gatherer at `idx`, its walk `arrived` or failed: the reseed ([`World::reseed`]);
    /// without one (the interface's event 0x7f, 0x9c for a gatherer not a villager, aside) the
    /// Gather is done or idles ([`crate::gathering::gather::GatherAction::after_reseed`]: standing with
    /// `FUN_140a8b6b0`'s sprite, the targets it lets go released).
    pub(crate) fn reseed_after_walk(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        arrived: bool,
    ) -> Result<(), GenieError> {
        if self.reseed(p, idx)? {
            return Ok(());
        }
        let by = self.releaser(p, idx)?;
        let master_id = self.players.live(p, idx)?.life.master_id;
        let standing = self.master(p, master_id)?.standing_sprite;
        let computer = self.players.player(p)?.computer;
        let unit = self.players.live_mut(p, idx)?;
        let Some(gather) = unit.actions.gather_mut() else {
            return Ok(());
        };
        let stand = crate::gathering::hunt::stand_sprite(
            &gather.task,
            standing,
            unit.life.held(),
            &self.graphics,
        );
        let released = gather.after_reseed(
            arrived,
            computer,
            unit.body.mover_mut(),
            stand,
            &mut unit.sprite,
            self.streams.get(crate::math::rng::Stream::Sprite),
        )?;
        for t in released.into_iter().flatten() {
            self.release(t, by)?;
        }
        Ok(())
    }

    /// Player `p`'s queue `queue`.
    fn queued(&self, p: PlayerId, queue: Queue) -> Result<QueueCount, GenieError> {
        let owner = self.players.player(p)?;
        Ok(match queue {
            Queue::Farms => owner.farm_queue,
            Queue::FishTraps => owner.fish_trap_queue,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::masters::ClassId;
    use crate::objects::masters::ids;
    use crate::objects::masters::{Master, Masters, master_type};
    use crate::players::attribute;
    use crate::players::attribute::AttributeId;
    use crate::test_ids::costs;
    use crate::test_ids::master_id;
    use crate::test_ids::opt_master;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;

    /// A master of type 70 costing `cost` (attribute, amount, paid).
    fn master(id: MasterId, cost: &[(AttributeId, i16, bool)]) -> Master {
        Master {
            id,
            kind: master_type::COMBAT,
            cost: costs(cost),
            ..Master::default()
        }
    }

    /// Gaia and player 1 holding `wood`, with the farm (60 wood), the pasture (100 wood, its
    /// food entry unpaid) and the fish trap (100 wood).
    fn world(wood: f32) -> World {
        let player = Player {
            attributes: crate::test_ids::attributes(300, [0.0, wood, 0.0, 0.0]),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 2],
            vec![Player::default(), player],
        );
        let masters = Masters::new([
            master(FARM, &[(attribute::WOOD, 60, true)]),
            master(
                PASTURE,
                &[(attribute::WOOD, 100, true), (attribute::FOOD, 50, false)],
            ),
            master(FISH_TRAP, &[(attribute::WOOD, 100, true)]),
        ]);
        t.masters = vec![Masters::new([]), masters];
        t
    }

    /// A purchase pays for the most of `count` the player can, then queues them.
    #[test]
    fn the_mill_s_queue_buys_what_the_player_can_pay() {
        let mut t = world(250.0);
        assert!(
            t.queue_farms(PlayerId::new(1), QueueCount::of_data(5), FARM)
                .unwrap()
        );
        assert_eq!(t.players[1].farm_queue, QueueCount::of_data(4));
        assert_eq!(t.players[1].attributes[1], Amount::new(10.0));
        assert!(
            !t.queue_farms(PlayerId::new(1), QueueCount::of_data(1), FARM)
                .unwrap()
        );
        assert!(
            !t.queue_farms(PlayerId::new(1), QueueCount::of_data(0), FARM)
                .unwrap()
        );
        assert_eq!(t.players[1].farm_queue, QueueCount::of_data(4));
    }

    /// The pasture's test asks for its unpaid entry too, its payment takes only its paid one;
    /// the fish traps have their own queue.
    #[test]
    fn pastures_and_fish_traps_pay_their_paid_entries() {
        let mut t = world(1000.0);
        t.players[1].attributes[0] = Amount::new(99.0);
        assert!(
            t.queue_farms(PlayerId::new(1), QueueCount::of_data(2), PASTURE)
                .unwrap()
        );
        assert_eq!(t.players[1].farm_queue, QueueCount::of_data(1));
        assert_eq!(
            t.players[1].resources(),
            crate::test_ids::amounts([99.0, 900.0, 0.0, 0.0])
        );
        t.players[1].attributes[0] = Amount::new(100.0);
        assert!(
            t.queue_farms(PlayerId::new(1), QueueCount::of_data(2), PASTURE)
                .unwrap()
        );
        assert_eq!(
            t.players[1].resources(),
            crate::test_ids::amounts([100.0, 700.0, 0.0, 0.0])
        );
        t.players[1].farm_queue = QueueCount::of_data(2);
        assert!(
            t.queue_fish_traps(PlayerId::new(1), QueueCount::of_data(3))
                .unwrap()
        );
        assert_eq!(t.players[1].fish_trap_queue, QueueCount::of_data(3));
        assert_eq!(t.players[1].farm_queue, QueueCount::of_data(2));
        assert_eq!(t.players[1].attributes[1], Amount::new(400.0));
    }

    /// A queue at 40 takes no purchase; one below may pass 40.
    #[test]
    fn a_full_queue_takes_no_purchase() {
        let mut t = world(10_000.0);
        t.players[1].farm_queue = QueueCount::of_data(40);
        assert!(
            !t.queue_farms(PlayerId::new(1), QueueCount::of_data(1), FARM)
                .unwrap()
        );
        t.players[1].farm_queue = QueueCount::of_data(39);
        assert!(
            t.queue_farms(PlayerId::new(1), QueueCount::of_data(3), FARM)
                .unwrap()
        );
        assert_eq!(t.players[1].farm_queue, QueueCount::of_data(42));
    }

    /// A master the player lacks buys nothing.
    #[test]
    fn a_master_the_player_lacks_buys_nothing() {
        let mut t = world(1000.0);
        assert!(
            !t.queue_farms(PlayerId::new(1), QueueCount::of_data(2), ids::ID_1187)
                .unwrap()
        );
        assert_eq!(t.players[1].attributes[1], Amount::new(1000.0));
    }

    /// A building master `id` of class `class` and base id `base`, available or not, heading
    /// `head` (−1 for none).
    fn building(
        id: MasterId,
        class: ClassId,
        base: MasterId,
        available: bool,
        head: i16,
    ) -> Master {
        Master {
            id,
            kind: master_type::BUILDING,
            class,
            base_id: Some(base),
            available,
            building: Some(crate::objects::masters::BuildingMaster {
                head_unit: opt_master(head),
                stack_unit: None,
                annexes: [None; 4],
                ..crate::objects::masters::BuildingMaster::default()
            }),
            ..Master::default()
        }
    }

    /// `FUN_141663f80`: the master its base id names is available; a stack's head names the
    /// head's base; a master the player lacks, or a base it lacks, makes none.
    #[test]
    fn a_master_can_be_made_by_its_base_s_availability() {
        let mut t = world(0.0);
        t.masters[1] = Masters::new([
            building(FARM, class::FARM, FARM, true, -1),
            building(PASTURE, class::FARM, master_id(300), false, -1),
            building(master_id(300), class::BUILDING, master_id(300), true, 301),
            building(master_id(301), class::BUILDING, master_id(302), false, -1),
            building(master_id(302), class::BUILDING, master_id(302), true, -1),
            building(master_id(303), class::BUILDING, master_id(999), true, -1),
        ]);
        assert!(t.can_make(PlayerId::new(1), FARM).unwrap());
        // the pasture's base 300 heads 301, whose base 302 is available
        assert!(t.can_make(PlayerId::new(1), PASTURE).unwrap());
        assert!(!t.can_make(PlayerId::new(1), ids::ID_1187).unwrap());
        assert!(!t.can_make(PlayerId::new(1), master_id(303)).unwrap());
        t.masters[1] = Masters::new([building(FARM, class::FARM, FARM, false, -1)]);
        assert!(!t.can_make(PlayerId::new(1), FARM).unwrap());
    }

    /// The town centre 621 is also made while the town centres standing are fewer than
    /// attribute 218.
    #[test]
    fn a_town_centre_can_be_made_below_the_town_centre_limit() {
        let mut t = world(0.0);
        t.masters[1] = Masters::new([building(
            TOWN_CENTRE_621,
            class::BUILDING,
            TOWN_CENTRE_621,
            false,
            -1,
        )]);
        assert!(!t.can_make(PlayerId::new(1), TOWN_CENTRE_621).unwrap());
        t.players[1].attributes[attribute::TOWN_CENTRE_LIMIT.index()] = Amount::new(1.0);
        assert!(t.can_make(PlayerId::new(1), TOWN_CENTRE_621).unwrap());
    }

    /// `FUN_141658b40`: the farm when the player can make it, else the pasture, each of the
    /// farm class; else none.
    #[test]
    fn a_reseed_makes_the_farm_else_the_pasture() {
        let mut t = world(0.0);
        let masters = |farm: bool, pasture: bool, class: ClassId| {
            Masters::new([
                building(FARM, class, FARM, farm, -1),
                building(PASTURE, class::FARM, PASTURE, pasture, -1),
            ])
        };
        t.masters[1] = masters(true, true, class::FARM);
        assert_eq!(t.reseed_master(PlayerId::new(1)).unwrap(), Some(FARM));
        t.masters[1] = masters(false, true, class::FARM);
        assert_eq!(t.reseed_master(PlayerId::new(1)).unwrap(), Some(PASTURE));
        t.masters[1] = masters(true, true, class::BUILDING);
        assert_eq!(t.reseed_master(PlayerId::new(1)).unwrap(), Some(PASTURE));
        t.masters[1] = masters(false, false, class::FARM);
        assert_eq!(t.reseed_master(PlayerId::new(1)).unwrap(), None);
    }

    /// A return gives back at most the queue's count, each its paid costs.
    #[test]
    fn a_return_refunds_at_most_the_queue() {
        let mut t = world(0.0);
        t.players[1].farm_queue = QueueCount::of_data(3);
        assert!(
            t.unqueue_farms(PlayerId::new(1), QueueCount::of_data(5), FARM)
                .unwrap()
        );
        assert_eq!(t.players[1].farm_queue, QueueCount::of_data(0));
        assert_eq!(t.players[1].attributes[1], Amount::new(180.0));
        assert!(
            !t.unqueue_farms(PlayerId::new(1), QueueCount::of_data(1), FARM)
                .unwrap()
        );
        t.players[1].farm_queue = QueueCount::of_data(2);
        assert!(
            t.unqueue_farms(PlayerId::new(1), QueueCount::of_data(1), PASTURE)
                .unwrap()
        );
        assert_eq!(
            t.players[1].resources(),
            crate::test_ids::amounts([0.0, 280.0, 0.0, 0.0])
        );
        t.players[1].fish_trap_queue = QueueCount::of_data(1);
        assert!(
            !t.unqueue_fish_traps(PlayerId::new(1), QueueCount::of_data(0))
                .unwrap()
        );
        assert!(
            t.unqueue_fish_traps(PlayerId::new(1), QueueCount::of_data(2))
                .unwrap()
        );
        assert_eq!(t.players[1].fish_trap_queue, QueueCount::of_data(0));
        assert_eq!(t.players[1].attributes[1], Amount::new(380.0));
    }
}
