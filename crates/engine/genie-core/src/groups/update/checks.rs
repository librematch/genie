//! What the update asks of the world: the units kept apart, an interruption, rest, a target
//! still to act on, the relics, a member's idleness, the sprites shown and the lead.

use crate::GenieError;
use crate::groups::GroupId;
use crate::groups::group_list::member_gone;
use crate::groups::update::{
    BUILD, DELIVER_RELIC, FARTHEST, NO_COMMAND, PICK_UP_RELIC, RELIC_DELIVERY_ACTION,
    RELIC_PICKUP_ACTION, Shown, TRADE_CART_SPRITES,
};
use crate::groups::{UnitGroup, WorldObject};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Tile;
use crate::objects::ActionState;
use crate::objects::SpriteId;
use crate::objects::masters::ActionType;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::ids::MONASTERY;
use crate::objects::masters::master_type::ACTION;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::masters::master_type::MOVING;
use crate::objects::unit::Unit;
use crate::objects::{ObjectId, ObjectKey};
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::slots::PlayerSlots;

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
    pub(crate) fn keep_apart(&mut self, group: GroupId) -> Result<(), GenieError> {
        let mut i = 0;
        loop {
            let g = self.groups.held(group)?;
            let Some(&id) = g.objects.get(i) else {
                break;
            };
            let Some(object) = self.world_object(id) else {
                let _ = self.groups.held_mut(group)?.objects.remove(i);
                continue;
            };
            if !self.may_interrupt(object)? {
                i += 1;
                continue;
            }
            if let WorldObject::Unit(p, u) = object
                && u.combat
                    .ai
                    .as_ref()
                    .is_some_and(|ai| ai.current_action == Some(ActionKind::Attack))
            {
                let master = u.master(&self.masters, p)?;
                // The relics' master class (`FUN_141017c70`'s test of the target's class).
                // The monastery's master id (`FUN_141017850`'s test of the target's master).
                // The master types from which an object moves (`RGE_Moving_Object`), acts
                // (`RGE_Action_Object`) and is a TRIBE combat unit.
                // The trade cart's and the trade boat's master classes (`FUN_141017f00`).
                let search = if master.kind < ACTION {
                    master.line_of_sight
                } else {
                    master.search_radius
                };
                let reach = crate::math::fmath::maxss(search.get(), master.line_of_sight.get());
                let reach = reach * reach;
                let mut nearest = FARTHEST;
                let mut found = false;
                for m in &g.members {
                    let Some(other) = self.world_object(m.id) else {
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
            let _ = self.groups.held_mut(group)?.objects.remove(i);
            if let Some((_, u)) = self.object_mut(id)
                && let Some(mover) = u.body.mover_mut()
            {
                mover.stand()?;
            }
        }
        Ok(())
    }

    /// Slot 0x2d0 with 1 of `object`: false for an object below type 40 (a ghost's, a static,
    /// animated or moving object's `FUN_141071d80`); an action object's (`FUN_14106d170`) is
    /// false while its group (`+0x6d0`, by id) is held and not at rest ([`World::at_rest`],
    /// `FUN_141007a50`) or its unit AI has queued orders (`+0x18`), else whether its top action
    /// may be interrupted ([`crate::objects::unit::Unit::interruptible`]).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, and what [`crate::objects::unit::Unit::interruptible`] does not
    /// model.
    pub(super) fn may_interrupt(&self, object: WorldObject<'_>) -> Result<bool, GenieError> {
        let WorldObject::Unit(p, u) = object else {
            return Ok(false);
        };
        if u.master(&self.masters, p)?.kind < ACTION {
            return Ok(false);
        }
        if let Some(group) = u.group.id
            && self.groups.get(group).is_some()
            && !self.at_rest(group)
        {
            return Ok(false);
        }
        if u.combat.ai.as_ref().is_some_and(|ai| !ai.orders.is_empty()) {
            return Ok(false);
        }
        u.interruptible()
    }

    /// `FUN_141007a50` of group `group`: at rest without a command (-1), its motion not moving
    /// ([`crate::groups::Motion::moving`]) and no member the world holds moving (slot 0x5c8,
    /// [`crate::movement::Mover::is_moving`]; a static object's `FUN_141071d90` 0). False for a
    /// group the world does not hold.
    #[must_use]
    pub fn at_rest(&self, group: GroupId) -> bool {
        let Some(g) = self.groups.get(group) else {
            return false;
        };
        if g.current_command != NO_COMMAND || g.motion.moving() {
            return false;
        }
        !g.members.iter().any(|m| {
            matches!(self.world_object(m.id), Some(WorldObject::Unit(_, u))
                if u.body.mover().is_some_and(crate::movement::Mover::is_moving))
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
    ) -> Result<Option<WorldObject<'_>>, GenieError> {
        let WorldObject::Unit(p, u) = object else {
            return Ok(None);
        };
        if u.master(&self.masters, p)?.kind < ACTION {
            return Ok(None);
        }
        if let Some(e) = u.actions.unmodelled_action.clone() {
            return Err(e);
        }
        Ok(u.top_target()
            .filter(|&t| !t.is_recyclable())
            .and_then(|t| self.world_object(t)))
    }

    /// `FUN_14118f8c0` of `a` and `b`: the squared gap of their footprints (their masters'
    /// radii, `+0x5d8` and `+0x5dc`; [`crate::movement::footprint_gap_squared`]).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub(super) fn footprint_gap_squared(
        &self,
        a: WorldObject<'_>,
        b: WorldObject<'_>,
    ) -> Result<DistanceSquared, GenieError> {
        let ra = self.master(a.player(), a.master_id())?.radius;
        let rb = self.master(b.player(), b.master_id())?.radius;
        Ok(crate::movement::footprint_gap_squared(
            a.position(),
            ra,
            b.position(),
            rb,
        ))
    }

    /// `FUN_14118f680` of `a` and `b`: [`World::footprint_gap_squared`]'s per-axis gaps, the
    /// length of the two ([`crate::movement::footprint_gap`]).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub(super) fn footprint_gap(
        &self,
        a: WorldObject<'_>,
        b: WorldObject<'_>,
    ) -> Result<Distance, GenieError> {
        let ra = self.master(a.player(), a.master_id())?.radius;
        let rb = self.master(b.player(), b.master_id())?.radius;
        Ok(crate::movement::footprint_gap(
            a.position(),
            ra,
            b.position(),
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
    pub fn target_pending(&self, group: GroupId) -> Result<bool, GenieError> {
        let Some(g) = self.groups.get(group) else {
            return Ok(false);
        };
        if g.members.is_empty() {
            return Ok(false);
        }
        match g.current_command {
            PICK_UP_RELIC => return self.relic_pickup_pending(g),
            DELIVER_RELIC => return self.relic_delivery_pending(g),
            _ => {}
        }
        let Some(target) = self.world_object(g.current_target.target) else {
            return Ok(false);
        };
        let owner = g.owner_player()?;
        let most = if g.current_command == BUILD {
            crate::objects::ObjectState::Foundation
        } else {
            crate::objects::ObjectState::Alive
        };
        if most < target.state() || target.inside() {
            return Ok(false);
        }
        if g.current_command != BUILD && self.spatial.allied_with(owner, target.player()) {
            return Ok(false);
        }
        self.sees_object(target, owner)
    }

    /// `FUN_141017c70`: a member the world holds with a unit AI whose order is `0x2db` or whose
    /// action is `0x277`, on a group still picking up a relic (132), whose master's tasks
    /// ([`World::has_task`]) pick up relics (132) and its target (the unit AI's, `+0x48`) is a
    /// relic (its master's class `0x2a`), or deliver them (136) and its target is a relic or
    /// none the world holds.
    pub(super) fn relic_pickup_pending(&self, g: &UnitGroup) -> Result<bool, GenieError> {
        let owner = g.owner_player()?;
        for m in &g.members {
            let Some(WorldObject::Unit(_, u)) = self.world_object(m.id) else {
                continue;
            };
            let Some(ai) = u.combat.ai.as_ref() else {
                continue;
            };
            let picks = self.has_task(owner, u.life.master_id, RELIC_PICKUP_ACTION)?;
            let delivers = self.has_task(owner, u.life.master_id, RELIC_DELIVERY_ACTION)?;
            let target = self.world_object(ai.current_target);
            let relic = match target {
                Some(t) => self.master(t.player(), t.master_id())?.class == class::RELIC,
                None => false,
            };
            if g.current_command == PICK_UP_RELIC
                && (ai.current_order == Some(OrderKind::OnTarget)
                    || ai.current_action == Some(ActionKind::OnTarget))
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
    pub(super) fn relic_delivery_pending(&self, g: &UnitGroup) -> Result<bool, GenieError> {
        let owner = g.owner_player()?;
        for m in &g.members {
            let Some(WorldObject::Unit(_, u)) = self.world_object(m.id) else {
                continue;
            };
            let Some(ai) = u.combat.ai.as_ref() else {
                continue;
            };
            let picks = self.has_task(owner, u.life.master_id, RELIC_PICKUP_ACTION)?;
            let delivers = self.has_task(owner, u.life.master_id, RELIC_DELIVERY_ACTION)?;
            let monastery = self
                .world_object(ai.current_target)
                .is_some_and(|t| t.master_id() == MONASTERY);
            if g.current_command == DELIVER_RELIC
                && (ai.current_order == Some(OrderKind::OnTarget)
                    || ai.current_action == Some(ActionKind::OnTarget))
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
        action: ActionType,
    ) -> Result<bool, GenieError> {
        let Some(masters) = self.masters.of(player) else {
            return self.master(player, master).map(|_| false);
        };
        Ok(masters
            .get(master)
            .is_some_and(|m| ACTION <= m.kind && m.tasks.iter().any(|t| t.action_type == action)))
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
    ) -> Result<bool, GenieError> {
        let master = self.master(object.player(), object.master_id())?;
        let (x, y) = (object.position().x(), object.position().y());
        let (x, y) = (
            crate::math::conversions::cvttss2si(x),
            crate::math::conversions::cvttss2si(y),
        );
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
    /// whose action is not `0x2c0` ([`ActionKind::GroupAside`]) is not free while
    /// its action (its order for action `0x25b`) is not -1; without a unit AI it is free.
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, or with an action the slice does not
    /// model.
    pub fn member_idle(&self, id: impl ObjectKey) -> Result<bool, GenieError> {
        let Some(object) = self.world_object(id) else {
            return Ok(false);
        };
        let WorldObject::Unit(p, u) = object else {
            return Ok(true);
        };
        u.free_for_group(u.master(&self.masters, p)?)
    }

    /// Slot 0x660 of object `id`: it shows its standing sprite. Nothing for an object below type
    /// 30 (`FUN_141072270`) or one the world does not hold. An action object's
    /// (`FUN_14106e2e0`) asks its top action first (slot 0x430, then the action's slot 0x58 with
    /// 3, which only a trade's, `FUN_140ae22b0`, answers: the slice holds no trades); a moving
    /// object's (`FUN_14115cf80`) shows its standing sprite (slot 0x798: its master's, `+0x470`;
    /// a trade cart's with goods, `FUN_14106a940`, is its trade task's, and the unit's own
    /// `+0x8a9` override is not held) when it has one (slot 0x68, as [`crate::orders`]'s: the top
    /// speed is the standing sprite's, [`crate::movement::Mover::shows_standing`], and the
    /// sprite itself where the slice holds it).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, with an action the slice does not model,
    /// a trade cart (whose goods the slice does not hold), and a standing sprite the slice's
    /// graphics lack.
    pub(crate) fn show_standing(&mut self, id: ObjectId) -> Result<(), GenieError> {
        self.show_sprite(id, Shown::Standing).map(|_| ())
    }

    /// Slot 0x658 of object `id` (an action object's `FUN_14106e340`): it shows its walking
    /// sprite; false when it has none. A trade cart (class 19) with goods (`FUN_14106a940`)
    /// shows its trade task's (slot 0x7c8 then slot 0x68); an action object's top action is
    /// asked next (slot 0x430, then the action's slot 0x58 with 0: only a trade's answers, the
    /// slice holds none); then `FUN_14115d0e0` shows the walking sprite (slot 0x7c8: the
    /// master's, `+0x748`; the unit's own `+0x8a9` override is not held) when it has one (slot
    /// 0x68: the top speed is the walking sprite's, [`crate::movement::MoveStats::walk_speed`]).
    /// The members it serves are units (type 70).
    ///
    /// # Errors
    ///
    /// As [`World::show_standing`].
    pub(crate) fn show_walking(&mut self, id: ObjectId) -> Result<bool, GenieError> {
        self.show_sprite(id, Shown::Walking)
    }

    /// Slot 0x650 of object `id` (an action object's `FUN_14106e3c0`): it shows its running
    /// sprite; false when it has none. An action object's top action is asked first (slot 0x430,
    /// then the action's slot 0x58 with 1: only a trade's answers); then `FUN_14115d300` shows the
    /// master's running sprite (`+0x750`) when it has one (slot 0x68: the top speed is the running
    /// sprite's, [`crate::movement::MoveStats::run_speed`]).
    ///
    /// # Errors
    ///
    /// As [`World::show_standing`], a trade cart aside.
    pub(crate) fn show_running(&mut self, id: ObjectId) -> Result<bool, GenieError> {
        self.show_sprite(id, Shown::Running)
    }

    /// The sprite object `id` shows as `shown`, none when it has none or is not a moving
    /// object.
    ///
    /// # Errors
    ///
    /// A trade cart, or an action the slice does not model ([`World::show_sprite`]).
    fn sprite_to_show(&self, id: ObjectId, shown: Shown) -> Result<Option<SpriteId>, GenieError> {
        let Some((p, u)) = self.object(id) else {
            return Ok(None);
        };
        let master = u.master(&self.masters, p)?;
        if master.kind < MOVING {
            return Ok(None);
        }
        let trade_cart = match shown {
            Shown::Standing => master.kind == COMBAT && master.class == class::TRADE_CART,
            Shown::Walking => ACTION <= master.kind && master.class == class::TRADE_CART,
            Shown::Running => false,
        };
        if trade_cart {
            return Err(TRADE_CART_SPRITES);
        }
        if ACTION <= master.kind
            && let Some(e) = u.actions.unmodelled_action.clone()
        {
            return Err(e);
        }
        // the slots of a combat object read its terrain look ([`crate::objects::sprite::standing`]); a
        // trade cart's goods are passed above
        let look = u.combat.abilities.look.as_ref();
        let sprite = match shown {
            Shown::Standing => crate::objects::sprite::standing(master, look, false),
            Shown::Walking => crate::objects::sprite::walking(master, look, false),
            Shown::Running => crate::objects::sprite::running(master, look),
        };
        Ok(sprite)
    }

    /// Slots 0x660, 0x658 and 0x650 of object `id` ([`Shown`]): whether it shows the sprite.
    pub(super) fn show_sprite(&mut self, id: ObjectId, shown: Shown) -> Result<bool, GenieError> {
        let Some(sprite) = self.sprite_to_show(id, shown)? else {
            return Ok(false);
        };
        let graphic = self.graphics.get(sprite).copied();
        let (_, u) = self.object_mut(id).ok_or_else(|| member_gone(id))?;
        if let Some(m) = u.body.mover_mut() {
            m.shows(graphic.as_ref());
        }
        if u.sprite.is_none() {
            return Ok(true);
        }
        let g = graphic.ok_or(GenieError::unsupported(
            "a unit without its sprite's graphic",
        ))?;
        let place = self.place(id).ok_or_else(|| member_gone(id))?;
        let rng = self.streams.get(crate::math::rng::Stream::Sprite);
        let u = match place {
            crate::orders::Place::Live(p, i) => self.players.live_mut(p, i)?,
            crate::orders::Place::Sleeping(p, i) => self.players.asleep_mut(p, i)?,
        };
        crate::objects::sprite::new_sprite(&mut u.sprite, u.body.move_stats_mut(), g, rng);
        Ok(true)
    }

    /// `FUN_141016870`: the group's lead (`+0xc4`) is the member the world holds nearest (in 3D,
    /// slot 0x520's place) the target's place (`mCurrentTarget`'s original position), the first
    /// of equals; none without one.
    #[must_use]
    pub fn nearest_to_target(&self, group: GroupId) -> Option<ObjectId> {
        let g = self.groups.get(group)?;
        let to = g.current_target.original_position;
        let mut best = f32::MAX;
        let mut lead = None;
        for m in &g.members {
            let Some((_, u)) = self.object(m.id) else {
                continue;
            };
            // slot 0x520: the game's vector of where it last moved to
            let d = crate::math::fmath::sub(crate::groups::attack::moved_place(u), to);
            let squared = d.x() * d.x() + d.height() * d.height() + d.y() * d.y();
            if squared < best {
                lead = Some(m.id);
                best = squared;
            }
        }
        lead
    }
}

impl Unit {
    /// [`World::member_idle`] of the unit itself (`FUN_141017f00`), of master `master`, as its
    /// own update asks it (slot 0x5e0, [`crate::movement::GroupSpeed`]).
    ///
    /// # Errors
    ///
    /// A unit with an action the slice does not model.
    pub(crate) fn free_for_group(&self, master: &Master) -> Result<bool, GenieError> {
        let u = self;
        let state = if master.kind < ACTION {
            ActionState::NO_ACTION
        } else {
            if let Some(e) = u.actions.unmodelled_action.clone() {
                return Err(e);
            }
            u.top_action().map_or(ActionState::NO_ACTION, |a| a.state)
        }
        .get();
        if 1 < state {
            if state == 2 {
                if master.class == class::TRADE_CART || master.class == class::TRADE_BOAT {
                    return Ok(true);
                }
                if u.top_action_kind() == Some(crate::combat::attack::ATTACK) {
                    return Ok(false);
                }
            } else if !matches!(state, 3 | 13 | 14) {
                return Ok(false);
            }
        }
        if let Some(ai) = u.combat.ai.as_ref() {
            let action = ai.current_action;
            if action != Some(ActionKind::GroupAside) {
                // on the chosen action the order decides
                let busy = if action == Some(ActionKind::Heal) {
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
