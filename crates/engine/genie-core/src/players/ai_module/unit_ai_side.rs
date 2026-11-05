//! A computer player's AI hears its explorer's notify (`WorldPlayer` slot 0x3c8, the hear logic
//! `FUN_14162e1b0`, mType 0x202): the explorer's action waits for a point and no search of the
//! unit AI gave one, so the AI picks the next tile ("I need to do a search for unit #%d").
//!
//! The unit AI hands the notify to its owner (slot 0x48, slot 0x40, the owner's slot 0x2e8,
//! `FUN_14162dab0`), which runs the hear logic between a push and a pop of every stream's state
//! (`FUN_140efe4a0`, `FUN_140efe450`): the draws of the search leave no trace but its result.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Tile;
use crate::math::rng::Stream;
use crate::objects::masters::ObjectFlags;
use crate::players::PlayerKind;
use crate::unit_ai::NotifyKind;
use crate::world::WorldTime;

use crate::objects::ObjectId;
use crate::players::ai_module::InfoModule;
use crate::players::ai_module::{AiModule, SearchUnit};
use crate::unit_ai::world_view::{UnitView, owner_view, town_centre_unit};
use crate::unit_ai::{AiAction, Notify, alert_owner};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;

/// The world time (ms) before which an explorer's wait slot of -1 sends the search around the
/// player's town centre instead of straight to the unit's place.
const EARLY: WorldTime = WorldTime::from_millis(300_000);
/// The slot of the tactical module that holds the explorer's wait, in seconds.
const WAIT_SLOT: u32 = 0x100;

/// What the AI decided of its explorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Heard {
    /// Nothing: the action is not an explore, or the unit is not the explorer's to task.
    Nothing,
    /// The AI tasks the explorer to the tile (`FUN_140c0c900`).
    Task(Tile),
    /// The AI finds no tile and detasks the unit (`FUN_140bf5480`).
    Detask,
}

impl UnitView<'_, '_> {
    /// The owner hears `notify` (the hear logic after slot 0x2e8) of the unit whose unit AI's
    /// target is `ai_target`: a computer player's AI that holds its module hears an explorer
    /// wanting a point. Its tasking and detasking of the unit (the tactical module's groups) are
    /// not modelled.
    ///
    /// # Errors
    ///
    /// The tasking or detasking, and what the search does not model.
    pub(crate) fn hears(
        &mut self,
        notify: Notify,
        ai_target: Option<ObjectId>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let player = self.player;
        if !self.player_kind.is_computer() || notify.kind != NotifyKind::REQUIRES_SEARCH {
            return Ok(());
        }
        let Some(mut module) = self.step.world.ai.take(player) else {
            return Ok(());
        };
        let saved = self.step.world.streams.clone();
        let heard = self.hear_explore(&mut module, notify, ai_target, roles);
        self.step.world.streams = saved;
        self.step.world.ai.put_back(player, module);
        match heard? {
            Heard::Nothing => Ok(()),
            Heard::Task(tile) => alert_owner(PlayerKind::Computer).map_err(|error| {
                error
                    .with("tasks explorer", ObjectId::field(notify.caller))
                    .with("to", format_args!("({}, {})", tile.x(), tile.y()))
            }),
            Heard::Detask => alert_owner(PlayerKind::Computer)
                .map_err(|error| error.with("detasks explorer", ObjectId::field(notify.caller))),
        }
    }

    /// The hear logic's case 0x202 (`FUN_14162e1b0`): the explorer's search (its unit AI's
    /// target `ai_target`, which its top speed reads).
    ///
    /// # Errors
    ///
    /// A player without a town centre or a search the world does not model.
    pub(crate) fn hear_explore(
        &mut self,
        module: &mut AiModule,
        notify: Notify,
        ai_target: Option<ObjectId>,
        roles: &Roles,
    ) -> GenieResult<Heard> {
        let explore = [
            AiAction::Explore.value(),
            AiAction::ExploreAndDestroy.value(),
        ];
        if !explore.contains(&notify.params[0]) {
            return Ok(Heard::Nothing);
        }
        let own = self
            .unit
            .master(self.step.world.master_tables(), self.player)?;
        let (class_id, sight, flags) = (own.class, own.line_of_sight, own.object_flags);
        let id = self.unit.id;
        let unit = SearchUnit {
            position: self.unit.position().ground(),
            civilian: roles.of_class(class_id).contains(ClassRoles::VILLAGER),
        };
        let now = self.step.world.world_time;
        let group_tuning = self.step.world.model.tuning.group;
        let slot = module.tactical.slot(WAIT_SLOT);
        let (explorer_alive, centre) = {
            let view = owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?;
            let alive = module
                .info
                .explorer
                .is_some_and(|object_id| object_id == id || view.find(object_id).is_some());
            let centre = match town_centre_unit(view, self.step.world.model.slots.master)? {
                Some(unit) => {
                    let sight = unit
                        .master(self.step.world.master_tables(), self.player)?
                        .line_of_sight;
                    Some((unit.position().ground(), sight))
                }
                None => None,
            };
            (alive, centre)
        };
        let mut unsynced = self.step.world.streams.get_mut(Stream::Unsynced).clone();
        let (mut area, map, _) = self.scout_area(ai_target)?;
        let info = &mut module.info;
        let mut search = |info: &mut InfoModule, from: Point| {
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
            Some(object_id) if object_id != id => {
                return Ok(search(info, unit.position)?.map_or(Heard::Detask, Heard::Task));
            }
            Some(_) => {}
        }
        if flags.contains(ObjectFlags::SHIP) {
            return Ok(Heard::Detask);
        }
        let Some((at, centre_sight)) = centre else {
            // no town centre: the AI's own first civilian (`FUN_140b446f0`), not modelled
            return alert_owner(PlayerKind::Computer).map(|()| Heard::Nothing);
        };
        let mut tile = info.top_point();
        if tile == Tile::new(-1, -1) {
            let spacing = sight + Distance::new(group_tuning.catch_up_span);
            for ring in (1..=5).rev() {
                info.push_ring(ring as f32 * spacing + centre_sight, at);
            }
            tile = info.pop_point();
        }
        // one more turn than the stack holds points: a point already on top is met twice
        for _ in 0..=info.search.len() {
            if !info.tile_uncovered(tile)?
                && let Some(found) = search(info, Point::new(tile.x() as f32, tile.y() as f32))?
            {
                return Ok(Heard::Task(found));
            }
            tile = info.pop_point();
        }
        Ok(Heard::Detask)
    }
}
