//! The unit's Transport: the action's update on the step and a sail to a point.

use crate::GenieResult;
use crate::combat::abilities::Look;
use crate::garrison::transport::{MapShore, Ship, TransportAction, TransportReads};
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::movement::Me;
use crate::movement::move_to::MoveDefaults;
use crate::objects::layers::Action;
use crate::objects::layers::MotionMut;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::pathing::Pathing;
use crate::pathing::zones::Zones;
use crate::world::layer::Roles;
use crate::world::step::Step;
use genie_model::model::Model;

impl Unit {
    /// The Transport's update ([`crate::garrison::transport::TransportAction::update`] on the
    /// step): `transport_action`, the head's copy, runs and is written back after. What it reads of
    /// the unit's master ([`crate::garrison::transport::TransportReads`]: the sprite its `MoveTo`'s
    /// start shows, `FUN_140aa0d30`'s default for none); its code, whose 1 or 3 deletes it.
    pub(crate) fn transport_update(
        &mut self,
        mut transport_action: TransportAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let master = object_master(
            step.world.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.world.model.masters.of(step.player),
        )?;
        let given = TransportAction::given_sprite(transport_action.task, master)?;
        let reads = TransportReads {
            radius: master.radius,
            restriction: master.terrain_restriction,
            defaults: MoveDefaults::of(
                master,
                step.world.parts.combat_of(self.id).abilities.look.as_ref(),
                roles,
            ),
            given,
            standing: master
                .standing_sprite
                .and_then(|sprite_id| step.world.model.graphics.get(sprite_id))
                .copied(),
        };
        // written back before an error too: the stop's context reads the list
        let code = transport_action.update(step, self, actions, me, &reads);
        if let Some(own) = actions.list.transport_mut() {
            *own = transport_action;
        }
        Ok(HeadCode::of(code?))
    }

    /// The Transport's slot 8 on a move to `point`
    /// ([`crate::garrison::transport::TransportAction::move_to`]) with the map about the unit, then
    /// what its state change leaves: the unit stops (its `MoveTo`'s teardown) and takes its
    /// standing sprite. `transport_action` is the head's copy, which goes back into `actions` after
    /// its move; `motion` is the unit's motion layer.
    pub(crate) fn transport_sail(
        &mut self,
        (mut transport_action, actions): (TransportAction, &mut Action),
        mut motion: MotionMut<'_>,
        point: Position,
        (spatial, zones, pathing): (&Spatial, &mut Zones, &mut Pathing),
        model: &Model,
        (masters, look, sprite_rng): (OwnerMasters<'_>, Option<&Look>, &mut Well512),
    ) -> GenieResult<()> {
        let master = self.master_in(masters)?;
        let ship = Ship::of_unit(
            (self, motion.mover()),
            master,
            spatial.map,
            (&model.graphics, look),
            transport_action.task,
            &model.roles,
        )?;
        let restriction = master.terrain_restriction;
        let mut shore = MapShore::new(
            spatial,
            zones,
            pathing,
            restriction,
            self.position(),
            &model.tuning.tile_path,
        )?;
        let changed = transport_action.move_to(
            point,
            &ship,
            &mut shore,
            &model.tuning.transport,
            &model.tuning.move_to,
        );
        actions.list.set_transport(Some(transport_action));
        let changed = changed?;
        if changed.stopped
            && let Some(mover) = motion.mover_mut()
        {
            mover.stop()?;
        }
        if changed.stands {
            self.show_standing(
                motion,
                masters,
                &model.graphics,
                sprite_rng,
                model.tuning.sprite,
            )?;
        }
        Ok(())
    }
}
