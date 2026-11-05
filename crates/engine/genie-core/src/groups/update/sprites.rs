//! The sprites the group update shows its members: standing, walking and running, each from the
//! member's master and its action.

use crate::groups::SpriteShow;
use crate::groups::group_list::member_gone;
use crate::groups::update::{Shown, TRADE_CART_SPRITES};
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::masters::MasterKind;
use crate::objects::sprite::Cargo;
use crate::objects::sprite::{new_sprite, running, standing, walking};
use crate::world::Unread;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

impl World {
    /// Slot 0x660 of object `id`: it shows its standing sprite. Nothing for an object below type 30
    /// (`FUN_141072270`) or one the world does not hold. An action object's (`FUN_14106e2e0`) asks
    /// its top action first (slot 0x430, then the action's slot 0x58 with 3, which only a trade's,
    /// `FUN_140ae22b0`, answers: the slice holds no trades); a moving object's (`FUN_14115cf80`)
    /// shows its standing sprite (slot 0x798: its master's, `+0x470`; a trade cart's with goods,
    /// `FUN_14106a940`, is its trade task's, and the unit's own `+0x8a9` override is not held) when
    /// it has one (slot 0x68, as [`crate::orders`]'s: the top speed is the standing sprite's,
    /// [`MoveToAction::shows_standing`](crate::movement::move_to::MoveToAction::shows_standing),
    /// and the sprite itself where the slice holds it).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have, with an action the slice does not model,
    /// a trade cart (whose goods the slice does not hold), and a standing sprite the slice's
    /// graphics lack.
    pub(crate) fn show_standing(&mut self, id: ObjectId) -> GenieResult<()> {
        self.show_sprite(id, Shown::Standing).map(|_| ())
    }

    /// Slot 0x658 of object `id` (an action object's `FUN_14106e340`): it shows its walking
    /// sprite; no sprite when it has none. A trade cart (class 19) with goods (`FUN_14106a940`)
    /// shows its trade task's (slot 0x7c8 then slot 0x68); an action object's top action is
    /// asked next (slot 0x430, then the action's slot 0x58 with 0: only a trade's answers, the
    /// slice holds none); then `FUN_14115d0e0` shows the walking sprite (slot 0x7c8: the
    /// master's, `+0x748`; the unit's own `+0x8a9` override is not held) when it has one (slot
    /// 0x68: the top speed is the walking sprite's, [`crate::objects::sprite::top_speed`]).
    /// The members it serves are units (type 70).
    ///
    /// # Errors
    ///
    /// As [`World::show_standing`].
    pub(crate) fn show_walking(&mut self, id: ObjectId) -> GenieResult<SpriteShow> {
        self.show_sprite(id, Shown::Walking)
    }

    /// Slot 0x650 of object `id` (an action object's `FUN_14106e3c0`): it shows its running sprite;
    /// no sprite when it has none. An action object's top action is asked first (slot 0x430, then
    /// the action's slot 0x58 with 1: only a trade's answers); then `FUN_14115d300` shows the
    /// master's running sprite (`+0x750`) when it has one (slot 0x68: the top speed is the running
    /// sprite's, [`crate::objects::sprite::top_speed`]).
    ///
    /// # Errors
    ///
    /// As [`World::show_standing`], a trade cart aside.
    pub(crate) fn show_running(&mut self, id: ObjectId) -> GenieResult<SpriteShow> {
        self.show_sprite(id, Shown::Running)
    }

    /// The sprite object `id` shows as `shown`, none when it has none or is not a moving
    /// object.
    ///
    /// # Errors
    ///
    /// A trade cart, or an action the slice does not model ([`World::show_sprite`]).
    fn sprite_to_show(&self, id: ObjectId, shown: Shown) -> GenieResult<Option<SpriteId>> {
        let Some((player_id, unit)) = self.object(id) else {
            return Ok(None);
        };
        let master = unit.master(self.master_tables(), player_id)?;
        if master.kind < MasterKind::MOVING {
            return Ok(None);
        }
        let trade_cart = match shown {
            Shown::Standing => {
                master.kind == MasterKind::COMBAT
                    && self
                        .model
                        .roles
                        .of_class(master.class)
                        .contains(ClassRoles::TRADE_CART)
            }
            Shown::Walking => {
                MasterKind::ACTION <= master.kind
                    && self
                        .model
                        .roles
                        .of_class(master.class)
                        .contains(ClassRoles::TRADE_CART)
            }
            Shown::Running => false,
        };
        if trade_cart {
            return Err(TRADE_CART_SPRITES);
        }
        if MasterKind::ACTION <= master.kind
            && let Some(error) = self.parts.unread.get(unit.id).map(Unread::error)
        {
            return Err(error);
        }
        // the slots of a combat object read its terrain look ([`crate::objects::sprite::standing`]); a
        // trade cart's goods are passed above
        let look = self.parts.combat_of(unit.id).abilities.look.as_ref();
        let sprite = match shown {
            Shown::Standing => standing(master, look, Cargo::Empty, &self.model.roles),
            Shown::Walking => walking(master, look, Cargo::Empty, &self.model.roles),
            Shown::Running => running(master, look),
        };
        Ok(sprite)
    }

    /// Slots 0x660, 0x658 and 0x650 of object `id` ([`Shown`]): whether it shows the sprite.
    pub(super) fn show_sprite(&mut self, id: ObjectId, shown: Shown) -> GenieResult<SpriteShow> {
        let Some(sprite) = self.sprite_to_show(id, shown)? else {
            return Ok(SpriteShow::NoSprite);
        };
        let graphic = self.model.graphics.get(sprite).copied();
        let place = self.place(id).ok_or_else(|| member_gone(id))?;
        let unit = match place {
            Place::Live(owner, slot) => self.players.live_mut(owner, slot).ok(),
            Place::Sleeping(player_id, list_index) => {
                self.players.asleep_mut(player_id, list_index).ok()
            }
        }
        .ok_or_else(|| member_gone(id))?;
        let mut motion = self.parts.motion.layer_mut(unit.id);
        if let Some(mover) = motion.mover_mut() {
            mover.shows(graphic.as_ref());
        }
        if unit.sprite.is_none() {
            return Ok(SpriteShow::Shown);
        }
        let graphic = graphic.ok_or(GenieError::unsupported(
            "a unit without its sprite's graphic",
        ))?;
        let rng = self.streams.get_mut(Stream::Sprite);
        new_sprite(
            &mut unit.sprite,
            motion.move_stats_mut(),
            graphic,
            rng,
            self.model.tuning.sprite,
        );
        Ok(SpriteShow::Shown)
    }
}
