//! A work command's Hunt: its start from the factory and the Hunt action's own start.

use crate::GenieError;
use crate::combat::attack::BeginLayer;
use crate::gathering::hunt::{HuntAction, HuntStart, HuntTask, Quarry};
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ListIndex;
use crate::objects::ObjectKey;
use crate::objects::lifecycle::Held;
use crate::objects::masters::MasterTables;
use crate::objects::sprite::new_sprite;
use crate::orders::gather::HuntOrder;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::slots::PlayerSlots;
use crate::world::{HUNTER_UNMOVING, NO_STANDING_SPRITE};

impl World {
    /// The Hunt from the factory (`FUN_140aca4b0` case 0x6e): the action takes its target (slot
    /// 0x178); the work command's unit AI notify (`FUN_14106e4e0`, `notify`) sets the hunt order;
    /// the install (`FUN_14106e990`) clears the action list and starts the hunt.
    pub(in crate::orders) fn start_hunt(&mut self, order: HuntOrder) -> Result<(), GenieError> {
        let HuntOrder {
            player,
            index: idx,
            target,
            task,
            point,
            clears,
            notify,
        } = order;
        let mut quarry = self.quarry(player, target)?;
        quarry.drop_site = self.drops_at_main(player, idx, target)?;
        self.take(target, self.releaser(player, idx)?)?;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id).filter(|_| notify) {
            ai.notify_work(
                Some(OrderKind::Hunt),
                Some(AiAction::Hunt),
                Some((target, quarry.class)),
                point,
                clears,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(player, idx, (Some(OrderKind::Hunt), Some(AiAction::Hunt)))?;
        }
        self.clear_actions(player, idx)?;
        self.begin_hunt(player, idx, (task, &quarry))
    }

    /// [`World::start_hunt`]'s start of the Hunt action itself, once the unit's actions are
    /// cleared: the engagement and arms, the action's start and its first sprite, and a load to its
    /// own drop site.
    ///
    /// # Errors
    ///
    /// What the start does not model.
    fn begin_hunt(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (task, quarry): (HuntTask, &Quarry),
    ) -> Result<(), GenieError> {
        let target = quarry.id;
        let engagement = self.engagement(player, idx, target)?;
        let arms = self.arms(player, idx, &task)?;
        let player_type = self.players.player(player)?.kind;
        let carry_rule = self.settings.carry_rule();
        let unit = self.players.live_mut(player, idx)?;
        let walks = unit.move_walks(
            self.parts.mover_of(unit.id),
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        )?;
        let mover = self.parts.mover_mut(unit.id).ok_or(HUNTER_UNMOVING)?;
        let mut held = unit.life.held();
        let (hunt, arrived) = HuntAction::start(
            HuntStart {
                task,
                target: quarry,
                player,
                player_type,
                carry_rule,
                held: &mut held,
                walks,
            },
            &engagement,
            arms,
            mover,
            BeginLayer {
                roles: &self.model.roles,
                attack_tuning: &self.model.tuning.attack,
                move_to_tuning: &self.model.tuning.move_to,
            },
            &self.model.tuning.hunt,
        )?;
        Held {
            kind: unit.life.attribute_type,
            amount: unit.life.amount,
        } = held;
        // an Attack whose MoveTo started arrived: the unit shows the idle sprite (slot 0x68) and
        // its `+0x7b8` clears (`Attack::arrived_sprite`)
        if arrived {
            let g = arms.idle_sprite.ok_or(NO_STANDING_SPRITE)?;
            new_sprite(
                &mut unit.sprite,
                self.parts.move_stats_mut(unit.id),
                g,
                self.streams.get(Stream::Sprite),
                self.model.tuning.sprite,
            );
            self.parts.set_anim_speed(unit.id, Speed::ZERO);
        }
        let drops = hunt.target.is_none() && hunt.carry_to.key() == Some(target);
        self.parts.actions_mut(unit.id)?.list.set_hunt(Some(hunt));
        // a load to its own drop site: the target taken as the second (FUN_140a7a190), then let
        // go as the first (slot 0x68 with none)
        if drops {
            self.take(target, self.releaser(player, idx)?)?;
            let by = self.releaser(player, idx)?;
            self.release(target, by)?;
        }
        Ok(())
    }
}
