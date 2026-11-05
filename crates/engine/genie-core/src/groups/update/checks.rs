//! What the update asks of the world: the units kept apart, an interruption, rest, a target
//! still to act on, the relics, a member's idleness, the sprites shown and the lead.

use crate::combat::attack::ATTACK;
use crate::groups::GroupId;
use crate::groups::attack::moved_place;
use crate::groups::update::{
    BUILD, DELIVER_RELIC, FARTHEST, NO_COMMAND, PICK_UP_RELIC, RELIC_DELIVERY_ACTION,
    RELIC_PICKUP_ACTION,
};
use crate::groups::{UnitGroup, WorldObject};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::maxss;
use crate::movement::{
    Mover, footprint_gap as movement_footprint_gap,
    footprint_gap_squared as movement_footprint_gap_squared,
};
use crate::objects::ActionState;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::unit::Unit;
use crate::objects::{ObjectId, ObjectKey};
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::world::Unread;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::{GenieError, GenieResult};

impl World {
    /// `FUN_141007ca0`: the units the group keeps apart (`+0x458`, the save's `objId`, size
    /// `+0x468`), in order. One the world does not hold leaves the list (`FUN_140a63bd0`); one
    /// that may not be interrupted (slot 0x2d0 with 1, [`World::may_interrupt`]) stays. One whose
    /// unit AI attacks (its action 600) looks for the target of a member's top action (slot
    /// 0x190: the action's target id when not negative, `FUN_140a78450`) nearest it (the
    /// footprints' squared gap, `FUN_14118f8c0`) within its reach squared (`MAXSS` of its slot
    /// 0x2c8, the master's search radius, a static object's slot 0x210, its line of sight, and its
    /// master's line of sight; `COMISS` then `JBE`: a NaN is no nearer), the first of equals; any
    /// other, and an attacker that found none, leaves the list and stands (slot 0x560,
    /// [`crate::movement::Mover::stand`]).
    ///
    /// # Errors
    ///
    /// An attacker that found a member's target (the group's check of it, `FUN_14100d2a0`, and
    /// the unit AI's `AttackObject`, slot 0xb8, are not modelled), and what the checks read that
    /// the slice does not hold.
    ///
    /// When the world does not hold `group`.
    pub(crate) fn keep_apart(&mut self, group: GroupId) -> GenieResult<()> {
        let mut index = 0;
        loop {
            let held = self.groups.held(group)?;
            let Some(&id) = held.objects.get(index) else {
                break;
            };
            let Some(object) = self.world_object(id) else {
                let _ = self.groups.held_mut(group)?.objects.remove(index);
                continue;
            };
            if !self.may_interrupt(object)? {
                index += 1;
                continue;
            }
            if let WorldObject::Unit(player_id, unit, _) = object
                && self
                    .parts
                    .unit_ai
                    .get(unit.id)
                    .is_some_and(|ai| ai.current_action == Some(AiAction::Attack))
            {
                let master = unit.master(self.master_tables(), player_id)?;
                // The relics' master class (`FUN_141017c70`'s test of the target's class).
                // The monastery's master id (`FUN_141017850`'s test of the target's master).
                // The master types from which an object moves (`RGE_Moving_Object`), acts
                // (`RGE_Action_Object`) and is a TRIBE combat unit.
                // The trade cart's and the trade boat's master classes (`FUN_141017f00`).
                let search = if master.kind < MasterKind::ACTION {
                    master.line_of_sight
                } else {
                    master.action_part().search_radius
                };
                let reach = maxss(search.get(), master.line_of_sight.get());
                let reach = reach * reach;
                let mut nearest = FARTHEST;
                let mut found = false;
                for member in &held.members {
                    let Some(other) = self.world_object(member.id) else {
                        continue;
                    };
                    if other.id() == object.id() {
                        continue;
                    }
                    let Some(target) = self.action_target(other)? else {
                        continue;
                    };
                    let gap = self.footprint_gap_squared(object, target)?;
                    if reach > gap.get() && nearest > gap.get() {
                        nearest = gap.get();
                        found = true;
                    }
                }
                if found {
                    return Err(GenieError::unsupported(
                        "a kept-apart attacker near a member's target (FUN_14100d2a0, AttackObject)",
                    ));
                }
            }
            let _ = self.groups.held_mut(group)?.objects.remove(index);
            if self.object(id).is_some()
                && let Some(mover) = self.parts.mover_mut(id)
            {
                mover.stand()?;
            }
        }
        Ok(())
    }

    /// Slot 0x2d0 with 1 of `object`: false for an object below type 40 (a ghost's, a static,
    /// animated or moving object's `FUN_141071d80`); an action object's (`FUN_14106d170`) is false
    /// while its group (`+0x6d0`, by id) is held and not at rest ([`World::at_rest`],
    /// `FUN_141007a50`) or its unit AI has queued orders (`+0x18`), else whether its top action may
    /// be interrupted ([`Action::interruptible`](crate::objects::layers::Action::interruptible)).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, and what
    /// [`Action::interruptible`](crate::objects::layers::Action::interruptible) does not model.
    pub(super) fn may_interrupt(&self, object: WorldObject<'_>) -> GenieResult<bool> {
        let WorldObject::Unit(player_id, unit, _) = object else {
            return Ok(false);
        };
        self.unit_may_interrupt(player_id, unit, self.parts.actions_of(unit.id))
    }

    /// [`World::may_interrupt`] of a unit of player `player_id` with its action layer
    /// `actions`: a unit that updates holds both out of the world's tables.
    ///
    /// # Errors
    ///
    /// As [`World::may_interrupt`].
    pub(crate) fn unit_may_interrupt(
        &self,
        player_id: PlayerId,
        unit: &Unit,
        actions: &Action,
    ) -> GenieResult<bool> {
        if unit.master(self.master_tables(), player_id)?.kind < MasterKind::ACTION {
            return Ok(false);
        }
        if let Some(group) = unit.group.id
            && self.groups.get(group).is_some()
            && !self.at_rest(group)
        {
            return Ok(false);
        }
        if self
            .parts
            .unit_ai
            .get(unit.id)
            .is_some_and(|ai| !ai.orders.is_empty())
        {
            return Ok(false);
        }
        actions.interruptible(self.parts.unread.get(unit.id))
    }

    /// `FUN_141007a50` of group `group`: at rest without a command (-1), its motion not moving
    /// ([`crate::groups::Motion::moving`]) and no member the world holds moving (slot 0x5c8,
    /// [`crate::movement::Mover::is_moving`]; a static object's `FUN_141071d90` 0). False for a
    /// group the world does not hold.
    #[must_use]
    pub fn at_rest(&self, group: GroupId) -> bool {
        let Some(group) = self.groups.get(group) else {
            return false;
        };
        if group.current_command != NO_COMMAND || group.motion.moving() {
            return false;
        }
        !group.members.iter().any(|member| {
            matches!(self.world_object(member.id), Some(WorldObject::Unit(_, unit, _))
                if self.parts.mover_of(unit.id).is_some_and(Mover::is_moving))
        })
    }

    /// Slot 0x190 of `object`: an action object's top action's target (`FUN_14106d090` ->
    /// `FUN_140a78450`: its target id, `+0x30`, when not negative, by the world's table); none
    /// for any other object (`FUN_141071d40`) or without a top action.
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, or with an action the slice does not
    /// model.
    pub(super) fn action_target(
        &self,
        object: WorldObject<'_>,
    ) -> GenieResult<Option<WorldObject<'_>>> {
        let WorldObject::Unit(player_id, unit, _) = object else {
            return Ok(None);
        };
        if unit.master(self.master_tables(), player_id)?.kind < MasterKind::ACTION {
            return Ok(None);
        }
        if let Some(error) = self.parts.unread.get(unit.id).map(Unread::error) {
            return Err(error);
        }
        Ok(self
            .parts
            .actions_of(unit.id)
            .top_target()
            .filter(|&object_id| !object_id.is_recyclable())
            .and_then(|object_id| self.world_object(object_id)))
    }

    /// `FUN_14118f8c0` of `object` and `world_object`: the squared gap of their footprints (their
    /// masters' radii, `+0x5d8` and `+0x5dc`; [`crate::movement::footprint_gap_squared`]).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub(super) fn footprint_gap_squared(
        &self,
        object: WorldObject<'_>,
        world_object: WorldObject<'_>,
    ) -> GenieResult<DistanceSquared> {
        let ra = self
            .master(object.player(), object.master_id(self.model.slots))?
            .radius;
        let rb = self
            .master(
                world_object.player(),
                world_object.master_id(self.model.slots),
            )?
            .radius;
        Ok(movement_footprint_gap_squared(
            object.position(),
            ra,
            world_object.position(),
            rb,
        ))
    }

    /// `FUN_14118f680` of `object` and `world_object`: [`World::footprint_gap_squared`]'s per-axis
    /// gaps, the length of the two ([`crate::movement::footprint_gap`]).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub(super) fn footprint_gap(
        &self,
        object: WorldObject<'_>,
        world_object: WorldObject<'_>,
    ) -> GenieResult<Distance> {
        let ra = self
            .master(object.player(), object.master_id(self.model.slots))?
            .radius;
        let rb = self
            .master(
                world_object.player(),
                world_object.master_id(self.model.slots),
            )?
            .radius;
        Ok(movement_footprint_gap(
            object.position(),
            ra,
            world_object.position(),
            rb,
        ))
    }

    /// `FUN_1410176b0` of group `group`: whether its target is still to act on. None without
    /// members (or a group the world does not hold). A relic's pick-up (132) and delivery (136)
    /// ask their own ([`World::relic_pickup_pending`], `FUN_141017c70`;
    /// [`World::relic_delivery_pending`], `FUN_141017850`). Otherwise the target (`+0x3c0`) the
    /// world holds, in a state up to 2 (0 for a build, 101), inside no other object
    /// (`FUN_141188da0`), of a player the owner does not count an ally (`FUN_141663d90` with the
    /// ally flag; a build skips this) and seen by the owner ([`World::sees_object`],
    /// `FUN_141188ea0`).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, or what the sight test does not hold.
    pub fn target_pending(&self, group: GroupId) -> GenieResult<bool> {
        let Some(group) = self.groups.get(group) else {
            return Ok(false);
        };
        if group.members.is_empty() {
            return Ok(false);
        }
        match group.current_command {
            PICK_UP_RELIC => return self.relic_pickup_pending(group),
            DELIVER_RELIC => return self.relic_delivery_pending(group),
            _ => {}
        }
        let Some(target) = self.world_object(group.current_target.target) else {
            return Ok(false);
        };
        let owner = group.owner_player()?;
        let most = if group.current_command == BUILD {
            ObjectState::Foundation
        } else {
            ObjectState::Alive
        };
        if most < target.state() || target.inside() {
            return Ok(false);
        }
        if group.current_command != BUILD && self.spatial.allied_with(owner, target.player()) {
            return Ok(false);
        }
        self.sees_object(target, owner)
    }

    /// `FUN_141017c70`: a member the world holds with a unit AI whose order is `0x2db` or whose
    /// action is `0x277`, on a group still picking up a relic (132), whose master's tasks
    /// ([`World::has_task`]) pick up relics (132) and its target (the unit AI's, `+0x48`) is a
    /// relic (its master's class `0x2a`), or deliver them (136) and its target is a relic or
    /// none the world holds.
    pub(super) fn relic_pickup_pending(&self, group: &UnitGroup) -> GenieResult<bool> {
        let owner = group.owner_player()?;
        for member in &group.members {
            let Some(WorldObject::Unit(_, unit, _)) = self.world_object(member.id) else {
                continue;
            };
            let Some(ai) = self.parts.unit_ai.get(unit.id) else {
                continue;
            };
            let picks = self.has_task(owner, unit.life.master_id, RELIC_PICKUP_ACTION)?;
            let delivers = self.has_task(owner, unit.life.master_id, RELIC_DELIVERY_ACTION)?;
            let target = self.world_object(ai.current_target);
            let relic = match target {
                Some(world_object) => self
                    .model
                    .roles
                    .of_class(
                        self.master(
                            world_object.player(),
                            world_object.master_id(self.model.slots),
                        )?
                        .class,
                    )
                    .contains(ClassRoles::RELIC),
                None => false,
            };
            if group.current_command == PICK_UP_RELIC
                && (ai.current_order == Some(OrderKind::OnTarget)
                    || ai.current_action == Some(AiAction::OnTarget))
                && ((picks && relic) || (delivers && (relic || target.is_none())))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `FUN_141017850`: as [`World::relic_pickup_pending`], on a group still delivering a relic
    /// (136): the target is a monastery (master 104) and the member's master's tasks deliver or
    /// pick up relics.
    pub(super) fn relic_delivery_pending(&self, group: &UnitGroup) -> GenieResult<bool> {
        let owner = group.owner_player()?;
        for member in &group.members {
            let Some(WorldObject::Unit(_, unit, _)) = self.world_object(member.id) else {
                continue;
            };
            let Some(ai) = self.parts.unit_ai.get(unit.id) else {
                continue;
            };
            let picks = self.has_task(owner, unit.life.master_id, RELIC_PICKUP_ACTION)?;
            let delivers = self.has_task(owner, unit.life.master_id, RELIC_DELIVERY_ACTION)?;
            let monastery = self
                .world_object(ai.current_target)
                .is_some_and(|world_object| {
                    self.model
                        .roles
                        .of_master(world_object.master_id(self.model.slots))
                        .contains(MasterRoles::MONASTERY)
                });
            if group.current_command == DELIVER_RELIC
                && (ai.current_order == Some(OrderKind::OnTarget)
                    || ai.current_action == Some(AiAction::OnTarget))
                && monastery
                && (delivers || picks)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `FUN_1415c60b0`: player `player` has master `master` (`FUN_141659d20`), of a type above
    /// 39, with a task of action type `action` (`+0xe` of its task list, `+0x788`).
    ///
    /// # Errors
    ///
    /// A player whose masters the slice does not have.
    pub(crate) fn has_task(
        &self,
        player: PlayerId,
        master: MasterId,
        action: ActionKind,
    ) -> GenieResult<bool> {
        let Some(masters) = self.model.masters.of(player) else {
            return self.master(player, master).map(|_| false);
        };
        Ok(masters.get(master).is_some_and(|master| {
            MasterKind::ACTION <= master.kind
                && master
                    .action_part()
                    .tasks
                    .iter()
                    .any(|task| task.action_kind == action)
        }))
    }

    /// `FUN_141188ea0` of `object` for player `player`: its tile (`CVTTSS2SI` of its x and y)
    /// seen by the player now (slot 0x1e0, `FUN_1416635e0`, 0xf: [`crate::map::spatial::Spatial::
    /// sees_tile`]), or, for a master whose `fog_visibility` (`+0x605`) is set, seen or explored
    /// ([`crate::map::spatial::Spatial::explored`]).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, and the explored test without the
    /// players' sight.
    pub(super) fn sees_object(
        &self,
        object: WorldObject<'_>,
        player: PlayerId,
    ) -> GenieResult<bool> {
        let master = self.master(object.player(), object.master_id(self.model.slots))?;
        let (x, y) = (object.position().x(), object.position().y());
        let (x, y) = (cvttss2si(x), cvttss2si(y));
        if master.fog_visibility == FogVisibility::INVISIBLE {
            Ok(self.spatial.sees_tile(player, Tile::new(x, y)))
        } else {
            self.spatial.explored(player, Tile::new(x, y))
        }
    }

    /// `FUN_141017f00` of object `id` (the group it gets is not read): whether the member is
    /// free for the group's moves. Not for an object the world does not hold. Its top action's
    /// state (slot 0x2b0; 0 without one, and for an object below type 40): above 1, state 2 is
    /// free for a trade cart (class 19) or a trade boat (2) and not for an Attack (slot 0x530,
    /// type 9), and states 3, 13 and 14 go on to the unit AI, any other is not free. A unit AI
    /// whose action is not `0x2c0` ([`AiAction::GroupAside`]) is not free while
    /// its action (its order for action `0x25b`) is not -1; without a unit AI it is free.
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, or with an action the slice does not
    /// model.
    pub fn member_idle(&self, id: impl ObjectKey) -> GenieResult<bool> {
        let Some(object) = self.world_object(id) else {
            return Ok(false);
        };
        let WorldObject::Unit(player_id, unit, _) = object else {
            return Ok(true);
        };
        self.parts.actions_of(unit.id).free_for_group(
            self.parts.unread.get(unit.id),
            unit.master(self.master_tables(), player_id)?,
            self.parts.unit_ai.get(unit.id),
            &self.model.roles,
        )
    }

    /// `FUN_141016870`: the group's lead (`+0xc4`) is the member the world holds nearest (in 3D,
    /// slot 0x520's place) the target's place (`mCurrentTarget`'s original position), the first
    /// of equals; none without one.
    #[must_use]
    pub fn nearest_to_target(&self, group: GroupId) -> Option<ObjectId> {
        let group = self.groups.get(group)?;
        let to = group.current_target.original_position;
        let mut best = f32::MAX;
        let mut lead = None;
        for member in &group.members {
            let Some((_, unit)) = self.object(member.id) else {
                continue;
            };
            // slot 0x520: the game's vector of where it last moved to
            let moved_place = moved_place(unit, self.parts.mover_of(unit.id)) - to;
            let squared = moved_place.x() * moved_place.x()
                + moved_place.height() * moved_place.height()
                + moved_place.y() * moved_place.y();
            if squared < best {
                lead = Some(member.id);
                best = squared;
            }
        }
        lead
    }
}

impl Action {
    /// [`World::member_idle`] of the unit itself (`FUN_141017f00`) by its action layer, of
    /// master `master` and unit AI `ai`, as its own update asks it (slot 0x5e0,
    /// [`crate::movement::GroupSpeed`]); `unread`: the load could not read it whole
    /// ([`crate::world::Unread`]).
    ///
    /// # Errors
    ///
    /// A unit with an action the slice does not model.
    pub(crate) fn free_for_group(
        &self,
        unread: Option<&Unread>,
        master: &Master,
        ai: Option<&UnitAi>,
        roles: &Roles,
    ) -> GenieResult<bool> {
        let actions = self;
        let state = if master.kind < MasterKind::ACTION {
            ActionState::NO_ACTION
        } else {
            if let Some(unread) = unread {
                return Err(unread.error());
            }
            actions
                .top_action()
                .map_or(ActionState::NO_ACTION, |top_action| top_action.state)
        }
        .get();
        if 1 < state {
            if state == 2 {
                if roles
                    .of_class(master.class)
                    .contains(ClassRoles::TRADE_CART)
                    || roles
                        .of_class(master.class)
                        .contains(ClassRoles::TRADE_BOAT)
                {
                    return Ok(true);
                }
                if actions.top_action_kind() == Some(ATTACK) {
                    return Ok(false);
                }
            } else if !matches!(state, 3 | 13 | 14) {
                return Ok(false);
            }
        }
        if let Some(ai) = ai {
            let action = ai.current_action;
            if action != Some(AiAction::GroupAside) {
                // on the chosen action the order decides
                let busy = if action == Some(AiAction::Heal) {
                    ai.current_order.is_some()
                } else {
                    action.is_some()
                };
                if busy {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
