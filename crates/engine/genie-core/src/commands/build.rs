//! BUILD (0x66, `FUN_140e7a400`): a player's foundation and the builders it sends to it.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::commands::{QueueId, SpriteOverride};
use crate::map::Point;
use crate::map::Position;
use crate::objects::Frame;
use crate::objects::masters::MasterId;
use crate::players::CivId;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlots;

/// BUILD (0x66, `FUN_140e7a400`): the player places a foundation of `master` at `position` (state
/// 0, hit points 1), with the unit AI the factory gives its master. The player pays the master's
/// cost, and each builder, in packet order, takes the master it builds with and starts a Build
/// action (`Model::masters`). The slice takes the
/// placement as given: the recording's game accepted it.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildCommand {
    /// The player who places it.
    pub player: PlayerId,
    /// The foundation's master (`+0x10`, an int the handler compares as one).
    pub master: MasterId,
    /// Where it is placed (`+0x8`, `+0xc`); the game sets its height from the ground.
    pub position: Point,
    /// `+0x14`, the formatter's `qId`: a human player's BUILD with one other than −1 does
    /// nothing (`FUN_140e7a400`'s first test).
    pub queue_id: QueueId,
    /// `+0x18`: the foundation's sprite override (`+0x788`, `FUN_141189e70`), which a house
    /// (master 70) of civ 12 keeps from 0x2c9 to 0x2ce; any other is −1.
    pub sprite_override: SpriteOverride,
    /// The units told to build it, in packet order (the count at `+0x4`, the ids from `+0x20`).
    pub builders: UnitList,
    /// `+0x1e`: queued (a builder with a unit AI takes it as an order in its queue).
    pub queued: bool,
    /// `+0x1f` is 0: a builder with a unit AI holding an order not ready takes it as an order
    /// too, and the queue's orders become ready after (`FUN_140c42b60`).
    pub commits: bool,
    /// `+0x1c`: the frame the foundation's standing sprite takes once built (its `+0xc54`,
    /// [`crate::objects::layers::Building::build_frame`]).
    pub frame: Frame,
}

impl BuildCommand {
    /// `do_command`'s case checks the builders first (a count past the packet's ids refuses it,
    /// [`World::read_units`]; another player's become −1; a human's BUILD left without one is
    /// refused). The handler (`FUN_140e7a400`) does nothing for a human's BUILD with a `qId`
    /// (`+0x14`) other than −1. A farm on rice terrain takes its rice master; then the pending debit
    /// (`FUN_141630d10` with 0), the builders' test, the placement test and the payment: a
    /// refused BUILD places nothing. The foundation takes its sprite override
    /// ([`BuildCommand::sprite_override_of`]) and the command's frame (`+0xc54`) before its
    /// builders get their tasks.
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        let Some(listed) = world.read_units(self.player, &self.builders) else {
            return Ok(());
        };
        let player = self.player;
        let Some(builders) = world.command_check(player, listed)? else {
            return Ok(());
        };
        if self.queue_id.is_queued() && !world.players.player(player)?.kind.is_computer() {
            return Ok(());
        }
        let (x, y) = (self.position.x(), self.position.y());
        let master = world.farm_on_rice_terrain(self.master, Position::new(x, y, 0.0));
        world.debit_listed(player, master)?;
        if !world.build_has_builder(player, master, &builders)? {
            return Ok(());
        }
        if !world.build_allowed(player, master, self.position)? {
            return Ok(());
        }
        let (id, position) = world.found(player, master, Position::new(x, y, 0.0))?;
        let sprite_override =
            self.sprite_override_of(world.players.player(player)?.civ, &world.model.roles);
        if let Some((_, layer)) = world.object_and_layer_mut(id)? {
            if let Some(construction) = layer.construction.as_mut() {
                construction.sprite_override = sprite_override != SpriteOverride::NONE;
            }
            layer.build_frame = self.frame;
        }
        world.task_builders(
            player,
            id,
            master,
            position,
            &builders,
            (self.queued, self.commits),
        )
    }

    /// The sprite override the handler gives the foundation (`FUN_141189e70`): the packet's
    /// `+0x18` for a house (master 70) of civ 12 when it is 0x2c9 to 0x2ce, else −1.
    #[must_use]
    pub fn sprite_override_of(&self, civ: CivId, roles: &Roles) -> SpriteOverride {
        const CIV: CivId = CivId::new(0xc);
        if self.sprite_override.kept()
            && roles.of_master(self.master).contains(MasterRoles::HOUSE)
            && civ == CIV
        {
            self.sprite_override
        } else {
            SpriteOverride::NONE
        }
    }
}
