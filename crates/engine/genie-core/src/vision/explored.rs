//! `Process_New_Tiles` at the end of a player's update and a new object's visible resources.

use crate::GenieError;
use crate::map::Point;
use crate::map::Tile;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::slots::PlayerSlots;

impl World {
    /// `Process_New_Tiles` (`FUN_1412899a0`, the end of `WorldPlayerBase::update`): on each tile
    /// player `p`'s map explored since its last update, in order, each object (the tile's list,
    /// in its order) of a master that gathers by itself (`enable_auto_gather`, `+0x681`) joins
    /// the player's list of its group (`+0x683`) at its place ([`VisibleResources::add`]). A
    /// ghost's master (243) does not gather; no recyclable master does, so the objects have ids of
    /// 0 and up (`FUN_141288830` reads them from the world's table `DAT_143f69168`).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    ///
    /// [`VisibleResources::add`]: crate::vision::visibility::VisibleResources::add
    pub(crate) fn process_new_tiles(&mut self, p: PlayerId) -> Result<(), GenieError> {
        let Some(sight) = self.spatial.sight.as_ref() else {
            return Ok(());
        };
        let (Some(tiles), Some(lists)) = (
            sight.new_tiles.get(p.index()),
            sight.resources.get(p.index()),
        ) else {
            return Ok(());
        };
        if lists.lists.is_empty() {
            return Ok(());
        }
        let mut found = Vec::new();
        for &pair in tiles {
            let (x, y) = (pair.x(), pair.y());
            for (id, _) in self.spatial.objects_on(Tile::new(x, y)) {
                let Some((owner, unit)) = self.object(id) else {
                    continue;
                };
                let master = unit.master(self.master_tables(), owner)?;
                if master.auto_gather.is_on() {
                    found.push((id, master.gather_group, unit.position()));
                }
            }
        }
        if let Some(r) = self
            .spatial
            .sight
            .as_mut()
            .and_then(|v| v.resources.get_mut(p.index()))
        {
            for (id, group, at) in found {
                r.add(id, group, at.ground());
            }
        }
        Ok(())
    }

    /// `WorldPlayer::make_new_object`'s visible resources (`FUN_14162ce00`, player slot 0x1c0,
    /// after the master made the object, the last of player `p`'s list): an object of a master
    /// that gathers by itself (`+0x681`) joins, at its place, the lists of its group (`+0x683`)
    /// of the players, in order, that see or explored its tile (`FUN_141663500` on its x and y
    /// truncated, [`Spatial::explored`]); the world's state (`+0x88`) is 0 in a game.
    ///
    /// # Errors
    ///
    /// A master the slice does not have, or no players' sight.
    ///
    /// [`Spatial::explored`]: crate::map::spatial::Spatial::explored
    pub(crate) fn made_new_object(&mut self, p: PlayerId) -> Result<(), GenieError> {
        let Some(u) = self.players.player(p)?.objects.last() else {
            return Ok(());
        };
        let (id, at) = (u.id, u.position());
        let (x, y) = (at.x(), at.y());
        let master = u.master(self.master_tables(), p)?;
        if !master.auto_gather.is_on() {
            return Ok(());
        }
        let group = master.gather_group;
        let (tx, ty) = (x as i32, y as i32);
        let mut explored = Vec::new();
        for q in PlayerId::each(self.players.len()) {
            explored.push(self.spatial.explored(q, Tile::new(tx, ty))?);
        }
        if let Some(v) = self.spatial.sight.as_mut() {
            for (q, _) in explored.iter().enumerate().filter(|&(_, &e)| e) {
                if let Some(r) = v.resources.get_mut(q) {
                    r.add(id, group, Point::new(x, y));
                }
            }
        }
        Ok(())
    }
}
