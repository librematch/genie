//! The idle checks of the boar, the hunted animals and the lions.

use crate::GenieError;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::sqrt;
use crate::math::rng::Well512;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::players::PlayerKind;
use crate::players::QueryRelation;
use crate::unit_ai::OrderForce;
use crate::unit_ai::Pace;
use crate::unit_ai::TargetRules;
use crate::unit_ai::flight::BadPoint;
use crate::unit_ai::{
    AI_MOVE_RANGE, AiMove, BOAR_HOME, BOAR_LEASH, BOAR_LEASH_DRAW, BOAR_OFFSET, BOAR_STEP,
    BOAR_WANDER, Effect, Found, HUNTED_CARED, HUNTED_HOME, HUNTED_SIGHT_GROWTH, HUNTED_STEP,
    LION_CARED, LION_IDLE_TIMEOUT, Memory, Pick, Search, Surroundings, UnitAi, UnitSide,
    point_gap_squared,
};
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use std::cmp::Ordering;

impl UnitAi {
    /// The boar's idle check (slot 72, `FUN_140c1e630`): `FUN_140c41e50` first
    /// ([`UnitAi::cap_human_order`]). A furious boar, or any in mode 1, stays. Then one draw of
    /// `0..81` widens its leash: outside it, a boar not yet heading home goes home; inside, once
    /// in 7 checks (a second draw) it goes home from more than 2 tiles away, or wanders up to 3
    /// tiles (two more draws). Its moves keep its height. True when it produced an action (6, not
    /// 5): every way but staying and the 6 in 7 checks inside the leash that do nothing,
    /// including the one heading home already.
    pub(super) fn boar_idle(
        &mut self,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        self.cap_human_order(side, ai)?;
        let (produced, moved) = self.boar_wander(at, ai);
        if let Some(m) = moved {
            side.effect_now(Effect::Move(m), self, ai)?;
        }
        Ok(produced)
    }

    /// [`UnitAi::boar_idle`] past its cap: whether it produced an action, and its move.
    pub(super) fn boar_wander(
        &mut self,
        at: &Surroundings,
        ai: &mut Well512,
    ) -> (bool, Option<AiMove>) {
        let Some(home) = self.home else {
            return (false, None);
        };
        if home.furious || at.mode_1 {
            return (false, None);
        }
        let (x, y, z) = (at.position.x(), at.position.y(), at.position.z());
        let dx = x - home.origin.x();
        let dy = y - home.origin.y();
        let d2 = dy * dy + dx * dx;
        let leash = ai.below(BOAR_LEASH_DRAW) as i32 as f32 + BOAR_LEASH;
        let go_home = AiMove {
            point: home.origin.with_z(z),
            range: AI_MOVE_RANGE,
            force: OrderForce::Forced,
            pace: Pace::Walk,
        };
        if home.go_home {
            let m = self.move_to(go_home);
            if let Some(h) = self.home.as_mut() {
                h.go_home = false;
            }
            return (true, m);
        }
        // comiss; jbe: unordered counts as inside
        if d2.partial_cmp(&leash) != Some(Ordering::Greater) {
            if !ai.rand15().is_multiple_of(BOAR_WANDER) {
                return (false, None);
            }
            if d2 > BOAR_HOME {
                return (true, self.move_to(go_home));
            }
            let step = |ai: &mut Well512| (ai.rand15() % BOAR_STEP) as i32 - BOAR_OFFSET;
            let nx = step(ai) as f32 + x;
            let ny = step(ai) as f32 + y;
            return (
                true,
                self.move_to(AiMove {
                    point: Position::new(nx, ny, z),
                    ..go_home
                }),
            );
        }
        if self.target_point.x() == home.origin.x() || self.target_point.y() == home.origin.y() {
            return (true, None);
        }
        (true, self.move_to(go_home))
    }

    /// The hunted animal's idle check (slot 72, `FUN_140c1eea0`). `FUN_140c41e50` first
    /// ([`UnitAi::cap_human_order`]). A king (class 59) does nothing more (5) unless it is a
    /// computer player's, not of master 704, not inside another and without an order: then its
    /// unit's slot 0x7e8 on `FUN_140c16930`'s object, which is not modelled. Beyond its leash
    /// (squared, from the unit's footprint, `FUN_140a847c0`) it goes home. Then its search: with
    /// nothing found, one draw; once in 7 it goes home from more than 2 tiles away, else once in
    /// 7 (the same draw) it wanders, two more draws; mode 1's herd is not modelled. A found object
    /// it evades ([`UnitAi::evade`]). Its moves keep its height and come within 0.5, forced and
    /// walking. True when it moved (6), false for 5.
    /// `FUN_140c63e20`'s alert after a search with its argument 4 set (`ClosestObject`'s, 1 at every
    /// call probed): enemies found set the alert mode and start the time since the last sighting
    /// again (past 10 seconds it would tell the owner first, slot 0x2e8 with notify 0x210, which
    /// the slice does not hold); none clear the alert mode. A search that read the last answer
    /// leaves it.
    pub(crate) const fn alert(&mut self, found: Found, _dt: Seconds) {
        if !found.fresh {
            return;
        }
        self.alert_mode = found.enemies;
        if found.enemies {
            self.time_since_enemy = Seconds::ZERO;
        }
    }

    /// The hunted animal's evade of the object its search found (`FUN_140c1eea0`): from its
    /// place away from the object by three times its speed (slot 0x208; one tile more in mode 1,
    /// not here), at its height, through `evasiveMoveTo` (slot 0x150, [`UnitAi::evasive_move`])
    /// with arguments 5 and 6 1 and 0 (`[RSP+0x20]`, `[RSP+0x28]`): an object on its own point
    /// gives coordinates that are not numbers, which become its own place plus or minus one;
    /// the point is held on the map and the unit moves there. True (6).
    pub(super) fn evade(
        &mut self,
        at: &Surroundings,
        other: Position,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        let (x, y, z) = (at.position.x(), at.position.y(), at.position.z());
        let reach = (at.speed * unit_ai_tuning.evade_reach).get();
        let (dx, dy) = (other.x() - x, other.y() - y);
        let d = sqrt(dy * dy + dx * dx);
        let point = Position::new(x - (dx / d) * reach, y - (dy / d) * reach, z);
        let _ = self.evasive_move(point, (OrderForce::Forced, BadPoint::Kept), at, ai, side)?;
        Ok(true)
    }

    pub(super) fn hunted_idle(
        &mut self,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        let roles = &at.roles;
        self.cap_human_order(side, ai)?;
        if roles.of_some_class(at.class).contains(ClassRoles::KING) {
            let led = !roles.of_master(at.master).contains(MasterRoles::KING_UNLED)
                && !at.garrisoned
                && self.current_order.is_none()
                && player_kind.is_computer();
            if led {
                return Err(GenieError::unsupported(
                    "a computer player's king's idle check (FUN_140c16930, slot 0x7e8)",
                ));
            }
            return Ok(false);
        }
        self.hunted_wander(at, ai, side)
    }

    /// [`UnitAi::hunted_idle`] for an animal that is not a king: its move, if any.
    pub(super) fn hunted_wander(
        &mut self,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        let h = self.hunted.ok_or(GenieError::unsupported(
            "a hunted animal without its origin",
        ))?;
        let (x, y, z) = (at.position.x(), at.position.y(), at.position.z());
        let go_home = AiMove {
            point: h.origin.with_z(z),
            range: AI_MOVE_RANGE,
            force: OrderForce::Forced,
            pace: Pace::Walk,
        };
        let leash = h.leash.squared();
        if DistanceSquared::ZERO < leash {
            let gap = point_gap_squared(h.origin.ground(), at.position, at.size);
            if leash < gap {
                if let Some(m) = self.move_to(go_home) {
                    side.effect_now(Effect::Move(m), self, ai)?;
                }
                return Ok(true);
            }
        }
        if at.mode_1 {
            return Err(GenieError::unsupported(
                "a hunted animal's idle check in mode 1 (its herd)",
            ));
        }
        let sight = at.los + HUNTED_SIGHT_GROWTH;
        let radius = if sight <= at.search_radius {
            at.search_radius
        } else {
            sight
        };
        let found = side.closest(&Search {
            at: Point::new(x, y),
            radius,
            classes: &HUNTED_CARED,
            size: at.size,
            pick: Pick::Closest {
                group: None,
                relation: None,
                state: None,
                master: None,
                workers: WorkerCount::NO_CAP,
                spared: None,
                reach: true,
            },
            memory: Memory::default(),
        })?;
        self.alert(found, at.dt_seconds);
        if found.id.is_some() {
            let other = found
                .position
                .ok_or(GenieError::unsupported("a found object without its place"))?;
            return self.evade(at, other, ai, side);
        }
        let r = ai.rand15();
        if r.is_multiple_of(BOAR_WANDER) {
            let dx = x - h.origin.x();
            let dy = y - h.origin.y();
            if HUNTED_HOME < dy * dy + dx * dx {
                if let Some(m) = self.move_to(go_home) {
                    side.effect_now(Effect::Move(m), self, ai)?;
                }
                return Ok(true);
            }
            let step = |ai: &mut Well512| {
                let n = (ai.rand15() % (2 * HUNTED_STEP as u32)) as i32;
                if HUNTED_STEP <= n {
                    n - 2 * HUNTED_STEP
                } else {
                    n + 1
                }
            };
            let sx = step(ai) as f32;
            let sy = step(ai) as f32;
            let wander = self.move_to(AiMove {
                point: Position::new(x + sx, sy + y, z),
                ..go_home
            });
            if let Some(m) = wander {
                side.effect_now(Effect::Move(m), self, ai)?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// The lion's idle check (slot 72, `FUN_140c1fb70`, factory type 5: wolves, bears).
    /// `FUN_140c41e50` first ([`UnitAi::cap_human_order`]); its idle timeout becomes 6000,
    /// redrawn (`FUN_140c41bc0`, slot 0x1a8: one draw). Its search looks for an enemy villager
    /// (class 4) in state 2, then for any enemy in state 2, from the same place (the second reads
    /// the first's answer). A found one within its reach (the footprints' squared gap,
    /// `FUN_14118f8c0`, against the reach squared) it attacks (`AttackObject`, slot 0xb8, not
    /// forced, [`UnitAi::attack_object`]): the reach is its line of sight, outside mode 1 half
    /// of it, or a third of it when it found no villager. The scenario modes' reach (a third by
    /// a setting's `+0x10` of 4) is not modelled. True (6) once it called `AttackObject`,
    /// whatever that did; false (5) when it found nothing within its reach.
    pub(super) fn lion_idle(
        &mut self,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        let slots = at.slots;
        self.cap_human_order(side, ai)?;
        self.set_idle_timeout(LION_IDLE_TIMEOUT, ai);
        let sight = at.los + HUNTED_SIGHT_GROWTH;
        let radius = if sight <= at.search_radius {
            at.search_radius
        } else {
            sight
        };
        let search = |group| Search {
            at: at.position.ground(),
            radius,
            classes: &LION_CARED,
            size: at.size,
            pick: Pick::Closest {
                group,
                relation: Some(QueryRelation::Enemies),
                state: Some(ObjectState::Alive),
                master: None,
                workers: WorkerCount::NO_CAP,
                spared: None,
                reach: true,
            },
            memory: Memory::default(),
        };
        let mut reach = if at.mode_1 {
            at.los
        } else {
            at.los * unit_ai_tuning.lion_sight_share
        };
        if at.scenario {
            return Err(GenieError::unsupported(
                "a lion's reach in the scenario modes",
            ));
        }
        // each search that queries sets the alert mode (`FUN_140c63e20`)
        let mut found = side.closest(&search(Some(slots.class.villager)))?;
        self.alert(found, at.dt_seconds);
        // none or a negative id (`JL`)
        if found.id.is_none_or(|id| id.get() < 0) {
            found = side.closest(&search(None))?;
            self.alert(found, at.dt_seconds);
            if !at.mode_1 {
                reach = at.los / unit_ai_tuning.lion_sight_divisor;
            }
        }
        let Some(id) = found.id else {
            return Ok(false);
        };
        let Some(t) = side.seen_unit(id)? else {
            return Ok(false);
        };
        if Distance::ZERO <= reach && reach.squared() < t.gap_squared {
            return Ok(false);
        }
        let _ = self.attack_object(id, OrderForce::Unforced, TargetRules::Own, at, side)?;
        Ok(true)
    }
}
