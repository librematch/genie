//! A computer player's AI hears its explorer's notify (`WorldPlayer` slot 0x3c8, the hear logic
//! `FUN_14162e1b0`, mType 0x202): the explorer's action waits for a point and no search of the
//! unit AI gave one, so the AI picks the next tile ("I need to do a search for unit #%d").
//!
//! The unit AI hands the notify to its owner (slot 0x48, slot 0x40, the owner's slot 0x2e8,
//! `FUN_14162dab0`), which runs the hear logic between a push and a pop of every stream's state
//! (`FUN_140efe4a0`, `FUN_140efe450`): the draws of the search leave no trace but its result.

use crate::GenieError;
use crate::map::Point;
use crate::map::Tile;
use crate::math::rng::Stream;
use crate::objects::masters::ObjectFlags;
use crate::unit_ai::NotifyKind;
use crate::world::WorldTime;

use crate::objects::ObjectId;
use crate::objects::masters::class;
use crate::players::ai_module::search::RING_SLACK;
use crate::players::ai_module::{AiModule, SearchUnit};
use crate::unit_ai::world_view::{UnitView, owner_view, town_centre_unit};
use crate::unit_ai::{ActionKind, Notify, alert_owner};

/// The world time (ms) before which an explorer's wait slot of -1 sends the search around the
/// player's town centre instead of straight to the unit's place.
const EARLY: WorldTime = WorldTime::from_millis(300_000);
/// The slot of the tactical module that holds the explorer's wait, in seconds.
const WAIT_SLOT: u32 = 0x100;

/// What the AI decided of its explorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    /// Nothing: the action is not an explore, or the unit is not the explorer's to task.
    Nothing,
    /// The AI tasks the explorer to the tile (`FUN_140c0c900`).
    Task(Tile),
    /// The AI finds no tile and detasks the unit (`FUN_140bf5480`).
    Detask,
}

impl UnitView<'_, '_> {
    /// The owner hears `n` (the hear logic after slot 0x2e8): a computer player's AI that holds
    /// its module hears an explorer wanting a point. Its tasking and detasking of the unit
    /// (the tactical module's groups) are not modelled.
    ///
    /// # Errors
    ///
    /// The tasking or detasking, and what the search does not model.
    pub(crate) fn hears(&mut self, n: Notify) -> Result<(), GenieError> {
        let player = self.player;
        if !self.computer || n.kind != NotifyKind::REQUIRES_SEARCH {
            return Ok(());
        }
        let Some(mut module) = self.c.w.ai.take(player) else {
            return Ok(());
        };
        let saved = self.c.w.streams.clone();
        let heard = self.hear_explore(&mut module, n);
        self.c.w.streams = saved;
        self.c.w.ai.put_back(player, module);
        match heard? {
            Heard::Nothing => Ok(()),
            Heard::Task(t) => alert_owner(true).map_err(|e| {
                e.with("tasks explorer", ObjectId::field(n.caller))
                    .with("to", format_args!("({}, {})", t.x(), t.y()))
            }),
            Heard::Detask => {
                alert_owner(true).map_err(|e| e.with("detasks explorer", ObjectId::field(n.caller)))
            }
        }
    }

    /// The hear logic's case 0x202 (`FUN_14162e1b0`): the explorer's search.
    ///
    /// # Errors
    ///
    /// A player without a town centre or a search the world does not model.
    pub(crate) fn hear_explore(
        &mut self,
        module: &mut AiModule,
        n: Notify,
    ) -> Result<Heard, GenieError> {
        let explore = [
            ActionKind::Explore.value(),
            ActionKind::ExploreAndDestroy.value(),
        ];
        if !explore.contains(&n.p[0]) {
            return Ok(Heard::Nothing);
        }
        let own = self.unit.master(&self.c.w.masters, self.player)?;
        let (class_id, sight, flags) = (own.class, own.line_of_sight, own.object_flags);
        let id = self.unit.id;
        let unit = SearchUnit {
            id,
            position: self.unit.position().ground(),
            civilian: class_id == class::VILLAGER,
        };
        let now = self.c.w.world_time;
        let slot = module.tactical.slot(WAIT_SLOT);
        let (explorer_alive, centre) = {
            let view = owner_view(&self.c.w.players, self.c.player, self.new_owner)?;
            let alive = module
                .info
                .explorer
                .is_some_and(|e| e == id || view.find(e).is_some());
            let centre = match town_centre_unit(view)? {
                Some(u) => {
                    let sight = u.master(&self.c.w.masters, self.player)?.line_of_sight;
                    Some((u.position().ground(), sight))
                }
                None => None,
            };
            (alive, centre)
        };
        let mut unsynced = self.c.w.streams.get(Stream::Unsynced).clone();
        let (mut area, map, _) = self.scout_area()?;
        let info = &mut module.info;
        let mut search = |info: &mut crate::players::ai_module::InfoModule, from: Point| {
            info.closest_unexplored_tile(&unit, from, &mut area, (map, &mut unsynced))
        };
        let waited = WorldTime::from_millis(slot.wrapping_mul(1000) as u32);
        // the wait slot says the search starts at the unit's place; else the AI looks around
        let here = (slot != -1 || EARLY <= now) && waited <= now;
        if here {
            return Ok(search(info, unit.position)?.map_or(Heard::Detask, Heard::Task));
        }
        match info.explorer {
            None => info.explorer = Some(id),
            Some(_) if !explorer_alive => info.explorer = Some(id),
            Some(e) if e != id => {
                return Ok(search(info, unit.position)?.map_or(Heard::Detask, Heard::Task));
            }
            Some(_) => {}
        }
        if flags.contains(ObjectFlags::SHIP) {
            return Ok(Heard::Detask);
        }
        let Some((at, centre_sight)) = centre else {
            // no town centre: the AI's own first civilian (`FUN_140b446f0`), not modelled
            return alert_owner(true).map(|()| Heard::Nothing);
        };
        let mut tile = info.top_point();
        if tile == Tile::new(-1, -1) {
            let spacing = sight + RING_SLACK;
            for ring in (1..=5).rev() {
                info.push_ring(ring as f32 * spacing + centre_sight, at);
            }
            tile = info.pop_point();
        }
        // one more turn than the stack holds points: a point already on top is met twice
        for _ in 0..=info.search.len() {
            if !info.tile_uncovered(tile)?
                && let Some(p) = search(info, Point::new(tile.x() as f32, tile.y() as f32))?
            {
                return Ok(Heard::Task(p));
            }
            tile = info.pop_point();
        }
        Ok(Heard::Detask)
    }
}
