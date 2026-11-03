//! The unit's Transport: the action's update on the step and a sail to a point.

use crate::GenieError;
use crate::combat::abilities::Look;
use crate::garrison::transport::{MapShore, NO_TRANSPORT, Ship, TransportAction, TransportReads};
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::movement::Me;
use crate::movement::move_to::MoveDefaults;
use crate::objects::layers::Action;
use crate::objects::layers::MotionMut;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMasters;
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
    /// step, the action copied out of its list and written back after): what it reads of the
    /// unit's master ([`crate::garrison::transport::TransportReads`]: the sprite its `MoveTo`'s
    /// start shows, `FUN_140aa0d30`'s default for none); its code, whose 1 or 3 deletes it.
    pub(crate) fn transport_update(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let mut t = *actions.list.transport().ok_or(NO_TRANSPORT)?;
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.model.masters.of(c.player),
        )?;
        let given = TransportAction::given_sprite(t.task, master)?;
        let reads = TransportReads {
            radius: master.radius,
            restriction: master.terrain_restriction,
            defaults: MoveDefaults::of(
                master,
                c.w.parts.combat_of(self.id).abilities.look.as_ref(),
                roles,
            ),
            given,
            standing: master
                .standing_sprite
                .and_then(|s| c.w.model.graphics.get(s))
                .copied(),
        };
        // written back before an error too: the stop's context reads the list
        let code = t.update(c, self, actions, me, &reads);
        if let Some(own) = actions.list.transport_mut() {
            *own = t;
        }
        Ok(HeadCode::of(code?))
    }

    /// The Transport's slot 8 on a move to `point` ([`crate::garrison::transport::TransportAction::move_to`])
    /// with the map about the unit, then what its state change leaves: the unit stops (its
    /// `MoveTo`'s teardown) and takes its standing sprite; `motion` is the unit's motion layer.
    pub(crate) fn transport_sail(
        &mut self,
        actions: &mut Action,
        mut motion: MotionMut<'_>,
        point: Position,
        (spatial, zones, pathing): (&Spatial, &mut Zones, &mut Pathing),
        model: &Model,
        (masters, look, sprite_rng): (OwnerMasters<'_>, Option<&Look>, &mut Well512),
    ) -> Result<(), GenieError> {
        let master = self.master_in(masters)?;
        let task = actions
            .list
            .transport()
            .ok_or(GenieError::unreachable("a sail without its Transport"))?
            .task;
        let ship = Ship::of_unit(
            (self, motion.mover()),
            master,
            spatial.map,
            (&model.graphics, look),
            task,
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
        let t = actions
            .list
            .transport_mut()
            .ok_or(GenieError::unreachable("a sail without its Transport"))?;
        let changed = t.move_to(
            point,
            &ship,
            &mut shore,
            &model.tuning.transport,
            &model.tuning.move_to,
        )?;
        if changed.stopped
            && let Some(m) = motion.mover_mut()
        {
            m.stop()?;
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
