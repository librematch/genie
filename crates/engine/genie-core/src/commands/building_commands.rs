//! The commands of a building: SET GATHER POINT, the command queue and RESEARCH.

use crate::GenieError;
use crate::commands::GatherPointState;
use crate::commands::gather_point::GatherPointCommand;
use crate::commands::research::ResearchCommand;
use crate::commands::{CASTLE_AGE, CASTLE_AGE_MODE, GatherPoint};
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::master_type::BUILDING;
use crate::orders::tech_outside;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::technology::ResearchState;
use crate::technology::TechId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::world::slots::PlayerSlots;
use crate::world::{BLOCKS_RESEARCH, World};

impl World {
    /// The index of object `id` in player `player`'s live list; none for a cleared id or an
    /// object the player does not hold.
    fn position_of_id(
        &self,
        player: PlayerId,
        id: Option<ObjectId>,
    ) -> Result<Option<ListIndex>, GenieError> {
        let held = self.players.player(player)?;
        Ok(id.and_then(|id| held.objects.position_of(id)))
    }

    /// Whether the player's object `i` takes a gather point: its master garrisons (`+0xb53`) or
    /// the player trains a unit there (`FUN_141625a20`'s count with no object,
    /// [`World::trainable_count`]). The test SET GATHER POINT and SET GATHER STATE make.
    fn takes_gather_point(&self, player: PlayerId, i: ListIndex) -> Result<bool, GenieError> {
        let master = self
            .players
            .live(player, i)?
            .master(&self.masters, player)?;
        Ok(master.garrison.holds() || self.trainable_count(player, master.id) > 0)
    }

    /// SET GATHER POINT's handler ([`GatherPointCommand`], `FUN_140e7e9a0`).
    ///
    /// The point's x and y always take the packet's; its target and master do too unless the
    /// state is [`GatherPointState::KEPT`] (a target of −1 at (−1, −1) clears the point, state 0; any
    /// other point makes the state 1 unless it is kept). With `own` the object itself is the
    /// target (`FUN_1411c7ff0`: its position, its id and master unless kept, state 1 even when it
    /// was kept). The z stays what the point held, 0 for none.
    pub(crate) fn gather_point(
        &mut self,
        g: &GatherPointCommand,
        ids: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        let GatherPointCommand {
            player,
            point,
            target,
            target_master,
            own,
            ..
        } = *g;
        let (player, [x, y]) = (player, point.get());
        for &id in ids {
            // another player's object (the game clears its id) or one that is gone
            let Some(i) = self.position_of_id(player, id)? else {
                continue;
            };
            if !self.takes_gather_point(player, i)? {
                continue;
            }
            let unit = &self.players.live(player, i)?;
            let old = unit.gather_point;
            let kept = old.is_some_and(|g| g.state == GatherPointState::KEPT);
            let z = old.map_or(0.0, |g| g.point.z());
            let gather = if own {
                let (target, target_master) = match old {
                    Some(g) if kept => (g.target, g.target_master),
                    _ => (Some(unit.id), Some(unit.life.master_id)),
                };
                Some(GatherPoint {
                    state: GatherPointState::POINT,
                    point: unit.position().with_z(z),
                    target,
                    target_master,
                })
            } else if target.is_none() && x == -1.0 && y == -1.0 {
                None
            } else {
                let (target, target_master) = match old {
                    Some(g) if kept => (g.target, g.target_master),
                    _ => (target, target_master),
                };
                Some(GatherPoint {
                    state: if kept {
                        GatherPointState::KEPT
                    } else {
                        GatherPointState::POINT
                    },
                    point: Position::new(x, y, z),
                    target,
                    target_master,
                })
            };
            let unit = self.players.live_mut(player, i)?;
            unit.gather_point = gather;
            // the unit AI's slot 0x210 (FUN_140c16790): its command queue empties
            if let Some(ai) = unit.ai.as_mut() {
                ai.building_orders.clear();
            }
        }
        Ok(())
    }

    /// SET GATHER STATE's handler ([`SetGatherStateCommand`], `FUN_140e727e0`): each object of
    /// `ids` the player holds clears its gather point with a state of 0, else, when it takes one
    /// ([`World::takes_gather_point`]), has itself for the point with the packet's state (its
    /// position, id and master). The unit AI's command queue stays.
    ///
    /// [`SetGatherStateCommand`]: crate::commands::SetGatherStateCommand
    pub(crate) fn set_gather_state(
        &mut self,
        player: PlayerId,
        state: Option<GatherPointState>,
        ids: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        for &id in ids {
            let Some(i) = self.position_of_id(player, id)? else {
                continue;
            };
            let gather = if let Some(state) = state {
                if !self.takes_gather_point(player, i)? {
                    continue;
                }
                let unit = &self.players.live(player, i)?;
                Some(GatherPoint {
                    state,
                    point: unit.position(),
                    target: Some(unit.id),
                    target_master: Some(unit.life.master_id),
                })
            } else {
                None
            };
            self.players.live_mut(player, i)?.gather_point = gather;
        }
        Ok(())
    }

    /// `BUILDING_COMMAND_QUEUE`'s handler (`0x140e64310`) for `player`: each object of `ids`
    /// the world holds with a unit AI queues the step (its slot 0x208, `FUN_140c15740`: the
    /// player, the order `0x2db` on a target or `0x2c6` on the point, priority −1, the target,
    /// its owner and class, the point with height −1, range 0, not ready). A building first puts
    /// its gather point in the queue when it has one, the queue is empty (slot 0x218) and the
    /// point is not on itself, then the gather point clears (`FUN_1411c7ff0` with −1). A target
    /// the world does not hold queues the point alone. The view's marker (`FUN_1414cd6d0`) is not
    /// held.
    pub(crate) fn command_queue(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
        target: Option<ObjectId>,
        point: Point,
    ) -> Result<(), GenieError> {
        let [x, y] = point.get();
        let step = |turns: &Self,
                    t: Option<ObjectId>,
                    at: Point|
         -> Result<crate::unit_ai::QueuedOrder, GenieError> {
            // the target's class, which the order on a gone target reads as a master of the
            // owner (the game's)
            let (target, owner, class) = match turns.object(t) {
                Some((tp, u)) => (
                    t,
                    Some(tp),
                    crate::unit_ai::OrderTargetType::of_class(u.master(&turns.masters, tp)?.class),
                ),
                None => (None, None, None),
            };
            Ok(crate::unit_ai::QueuedOrder {
                issuer: Some(crate::unit_ai::Issuer::Player(player)),
                // the game tests the id's sign: a recyclable object's is no target
                order: if target.is_none_or(ObjectId::is_recyclable) {
                    OrderKind::Move
                } else {
                    OrderKind::OnTarget
                },
                priority: -1,
                target,
                target_owner: owner,
                point: at.with_height(-1.0),
                range: Distance::ZERO,
                shown: Point::new(-1.0, -1.0),
                ready: false,
                target_type: class,
            })
        };
        for &id in ids {
            let Some(crate::orders::Place::Live(p, i)) = self.place(id) else {
                if self.place(id).is_some() {
                    return Err(GenieError::unsupported(
                        "a command queue on a sleeping object",
                    ));
                }
                continue;
            };
            let unit = &self.players.live(p, i)?;
            let Some(ai) = unit.ai.as_ref() else {
                continue;
            };
            let building = unit.master(&self.masters, p)?.kind == BUILDING;
            let mut queued = Vec::new();
            if building
                && let Some(g) = unit.gather_point
                && ai.building_orders.is_empty()
                && g.target != Some(unit.id)
            {
                // the gather point's own target id stays even when the world lost it
                let mut first = step(self, g.target, g.point.ground())?;
                if first.target.is_none() && g.target.is_some() {
                    first.target = g.target;
                    first.order = OrderKind::OnTarget;
                }
                queued.push(first);
            }
            queued.push(step(self, target, Point::new(x, y))?);
            let unit = self.players.live_mut(p, i)?;
            if building {
                unit.gather_point = None;
            }
            if let Some(ai) = unit.ai.as_mut() {
                ai.building_orders.extend(queued);
            }
        }
        Ok(())
    }

    /// `do_command`'s case 0x65 (`FUN_140e6f470`), RESEARCH's check before its handler: the
    /// count of buildings it lists (`+0x8` as a short) below 1 is refused; the packet's building
    /// (`+4`), when the world holds it, must be the player's own, or any while players may order
    /// each other's (the setting `+0x1074`), and listed alone it must be held; with more listed,
    /// the listed ones (`+0x11..`) as the unit commands' ([`World::command_check`]). None for a
    /// refused one, else the list with none for the ones left out. The packet's size tests are the
    /// decoder's, and the world's `+0xb12` cheat is not held.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub(crate) fn research_check(
        &self,
        r: &ResearchCommand,
    ) -> Result<Option<Vec<Option<ObjectId>>>, GenieError> {
        let player = r.player;
        let listed = usize::try_from(r.count).unwrap_or(0);
        if listed == 0 {
            return Ok(None);
        }
        let own = self
            .object(r.building)
            .map(|(p, _)| p == player || self.settings.cheats_enabled);
        match own {
            Some(false) => return Ok(None),
            None if listed == 1 => return Ok(None),
            _ => {}
        }
        if listed == 1 {
            return Ok(Some(r.buildings.clone()));
        }
        let (listed_ids, rest) = r.buildings.split_at(listed.min(r.buildings.len()));
        let Some(checked) = self.command_check(player, listed_ids)? else {
            return Ok(None);
        };
        Ok(Some(
            checked.into_iter().chain(rest.iter().copied()).collect(),
        ))
    }

    /// RESEARCH (`0x140e7e270`) for `player`: a `qId` other than −1, or a technology the tree
    /// has not got, does nothing. The costs it lists (`FUN_1413d4a00`) come off the pending
    /// debit (`FUN_141630d10`). The building is the packet's (the least loaded of several,
    /// `FUN_1411d0e90`, and the shared kind's every building, `FUN_1411c3fd0`, are not modelled);
    /// a building of master type 80 alive (state 2) whose master is among the technology's
    /// locations, with the technology in state 1 or repeatable, and without a top action of
    /// 0x7b to 0x7d (`FUN_141227a00`), queues it ([`World::queue_research`]); queued, its unit AI
    /// takes the action value 0x26c. A technology of `linkedTechs.json` ([`World::linked_techs`],
    /// the game settings' table `FUN_140f39f40` looks up) is not modelled (`FUN_140f3a0f0`,
    /// `FUN_140f39e80`). Not held: the interface's view and selection (`FUN_140e6de20`), the
    /// local player's statistics flag. Not modelled: the computer players' parts (`player+0x7a8`, `FUN_1411d5520`), the player's
    /// `+0x648` (0 for every player probed), Castle Age queued in game mode 16 (the world's treaty
    /// monitor's `+0x17`; no other game mode marks it).
    pub(crate) fn research(
        &mut self,
        r: &ResearchCommand,
        buildings: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        let player = r.player;
        let owner = self.players.player(player)?;
        let Some(tech_id) = TechId::of_data(r.tech).filter(|t| t.index() < owner.techs.len())
        else {
            return Ok(());
        };
        if r.q_id != -1 {
            return Ok(());
        }
        let tech = tech_id.index();
        let def = self.techs.get(tech).ok_or(GenieError::unsupported(
            "a technology the data file has not got",
        ))?;
        let get = |i: AttributeId| crate::technology::Attributes::get(owner, i);
        let factors = crate::technology::factors(tech_id, &get)?;
        let listed = owner.techs[tech].listed_costs(def, factors)?;
        debit_listed(self.players.player_mut(player)?, listed);
        let owner = self.players.player(player)?;
        let state = &owner.techs[tech];
        let repeatable = state.repeatable(def, false);
        if def.kind == crate::technology::TechType::SHARED {
            return Err(GenieError::unsupported(
                "RESEARCH of the shared kind (FUN_1411c3fd0 over the packet's buildings)",
            ));
        }
        let building_id = self.least_loaded(r, buildings)?;
        if self.linked_techs.contains_key(&r.tech) {
            return Err(GenieError::unsupported(
                "RESEARCH of a linked technology (FUN_140f3a0f0, FUN_140f39e80)",
            ));
        }
        // a technology that is not available (state 1) and may not be queued again is
        // researchable only when it is linked (`FUN_140f39e80`): no building takes it
        if state.state != ResearchState::Available && !repeatable {
            return Ok(());
        }
        let Some(i) = owner.objects.position_of(building_id) else {
            let elsewhere = self.players.iter().any(|q| {
                q.objects
                    .iter()
                    .chain(&q.sleeping)
                    .any(|u| Some(u.id) == building_id)
            });
            return if elsewhere {
                Err(GenieError::unsupported(
                    "RESEARCH at another player's object",
                ))
            } else {
                Ok(())
            };
        };
        let building = &owner.objects[i];
        let master = building.master(&self.masters, player)?;
        let at = state
            .locations
            .iter()
            .any(|l| l.building == Some(building.life.master_id));
        if master.kind != BUILDING || building.life.state != ObjectState::Alive || !at {
            return Ok(());
        }
        if owner.computer {
            return Err(GenieError::unsupported(
                "a computer player's RESEARCH (FUN_1411d5520, player+0x7a8)",
            ));
        }
        let top = building.top_action_kind();
        if building.unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "RESEARCH at a building with an unmodelled action",
            ));
        }
        if top.is_some_and(|k| BLOCKS_RESEARCH.contains(&k)) {
            return Ok(());
        }
        if self.queue_research(
            player,
            i,
            TechId::try_from(r.tech).map_err(|_| tech_outside(r.tech))?,
            r.shift,
        )? {
            // Castle Age queued in game mode 16 marks the world's treaty monitor (`+0x820`, its
            // `+0x17`), which the slice does not hold; any other game mode leaves it
            if r.tech == CASTLE_AGE && self.settings.game_mode == CASTLE_AGE_MODE {
                return Err(GenieError::unsupported(
                    "RESEARCH of Castle Age in game mode 16 (the treaty monitor's +0x17)",
                ));
            }
            let building = self.players.live_mut(player, i)?;
            if let Some(ai) = building.ai.as_mut() {
                ai.set_action(Some(ActionKind::Research));
            }
        }
        Ok(())
    }

    /// `FUN_1411d70f0` for the building at `i` of `player`: one research, five with `shift`
    /// unless the technology allows only one at a time or its effect upgrades a unit, queued by
    /// the building's part with no master and no button ([`World::queue_at`]). True when any
    /// was queued. The game settings' instant technologies (`FUN_140f39f40`'s `+0x20`, which
    /// `FUN_1413cdcd0` researches at once) are linked ones, which RESEARCH refuses before.
    pub(crate) fn queue_research(
        &mut self,
        player: PlayerId,
        i: ListIndex,
        tech: TechId,
        shift: bool,
    ) -> Result<bool, GenieError> {
        let t = tech.index();
        let def = &self.techs[t];
        let state = &self.players.player(player)?.techs[t];
        let one = !state.allow_multiple || def.upgrades();
        let count: u16 = if !shift || one { 1 } else { 5 };
        self.queue_at(player, i, None, count, Some(tech), -1)
    }
}

/// A research's listed costs go to the pending debit: each attribute 0 to 3 sums its amounts (in
/// 16 bits) and the player's debit of it falls by the sum.
fn debit_listed(owner: &mut crate::world::player::Player, listed: [(Option<AttributeId>, i16); 3]) {
    let mut sums = [0i16; 4];
    for (attribute, amount) in listed {
        if let Some(sum) = attribute.and_then(|k| sums.get_mut(k.index())) {
            *sum = sum.wrapping_add(amount);
        }
    }
    for (debit, amount) in owner.pending_debit.iter_mut().zip(sums) {
        *debit -= f32::from(amount);
    }
}
