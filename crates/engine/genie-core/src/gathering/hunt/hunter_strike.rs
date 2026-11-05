//! The hunt's strike as the hunter's update runs it: the hunter as a gatherer and an attacker,
//! the herd it leads, the hunt's update and what follows.

use crate::combat::AttackTargetId;
use crate::combat::attack::AttackSource;
use crate::combat::attack::{AttackReads, Fight};
use crate::combat::attack::{arms, engagement};
use crate::combat::damage::Victim;
use crate::combat::victims::{file_kill, victim};
use crate::gathering::DropSiteId;
use crate::gathering::drop_sites::nearest_drop_site;
use crate::gathering::hunt::DecayRule;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::TreeSearch;
use crate::gathering::hunt::{Herd, HuntAction, HuntNext, HuntReads, HuntSprites};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::movement::move_to::{MoveDefaults, MoveTo};
use crate::movement::{Me, footprint_gap_squared};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::layers::Action;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::Amount;
use crate::world::HUNTER_MASTER;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// The hunt's strike in the hunter's update (`0x140ac7a00`'s states 4, 6, 7, 10 and 12): what
    /// the hunt reads of the hunter as a gatherer and an attacker ([`Unit::hunt_reads`]), of its
    /// target and of the herd it leads, the hunt's update on the step with the hunt copied out of
    /// its list, then the hunt back, the retaliation's changes to the hunter
    /// ([`Unit::hit_back`]) and what follows that reads the hunt there ([`HuntNext`]): the
    /// drop-off, the search, or the end of an attack that failed (`own`: the hunter as a combat
    /// object, taken).
    pub(crate) fn hunt_strike(
        &mut self,
        (hunt, actions): (&mut HuntAction, &mut Action),
        step: &mut Step<'_>,
        me: Me,
        (own, tree_lost): (&mut Option<Victim>, bool),
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = object_master(
            step.world.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.world.model.masters.of(step.player),
        )?;
        let victim = hunt_victim(step, hunt, roles)?;
        let own = own.take().ok_or(HUNTER_MASTER)?;
        let herd = self.herd_for(
            step,
            OthersRef::of(&step.world.players, &step.world.index, step.player)?,
            hunt,
            master,
            &victim,
            roles,
        )?;
        let hunt_reads = self.hunt_reads(step, master, hunt, herd, tree_lost, roles)?;
        let mut fight = Fight { victim, own };
        // written back before an error too: the stop's context reads the list
        let next = hunt.update(step, self, actions, me, &mut fight, &hunt_reads);
        actions.list.set_hunt(Some(*hunt));
        let next = next?;
        // the retaliation's kill of the unit itself, which follows its step; it changes the
        // unit's actions only by livestock's change to Gaia, and livestock hunts nothing: the
        // game goes on with the Hunt itself
        if let Some(own) = self.hit_back(
            fight.own,
            step.world.parts.ai_and_combat_mut(self.id)?,
            step.world.world_time,
        )? {
            file_kill(step, (self, actions), own, roles)?;
        }
        match next {
            HuntNext::Stay => Ok(()),
            HuntNext::Drop => self.hunt_drop_off(hunt, actions, step, roles),
            HuntNext::Search => self.hunt_set_search(hunt, actions, step, roles),
            HuntNext::AttackFailed => self.hunt_attack_failed(hunt, actions, step, roles),
        }
    }

    /// What the hunt's update reads of the hunter of `master` in `hunt` ([`HuntReads`]): its
    /// Attack's reads ([`AttackReads::of`]); the master's work rate (`+0x798`) plus the powerup
    /// (slot 0x6e8), the capacity, the productivity of the task's attribute; the decay's stop
    /// (attribute 292), the unit AI's target class, the next tree's search done (`tree_lost`),
    /// the game's predator setting, the task's sprites and the herd it leads (`herd`).
    pub(crate) fn hunt_reads(
        &self,
        step: &Step<'_>,
        master: &Master,
        hunt: &HuntAction,
        herd: Option<Herd>,
        tree_lost: bool,
        roles: &Roles,
    ) -> GenieResult<HuntReads> {
        let owner = step.world.players.player(step.player)?;
        let productivity = (hunt.task.productivity)
            .and_then(|attribute_id| owner.attributes.get(attribute_id.index()))
            .copied()
            .filter(|&amount| Amount::ZERO < amount)
            .unwrap_or(Amount::new(1.0));
        let graphic_of = |id: Option<SpriteId>| {
            id.and_then(|sprite_id| step.world.model.graphics.get(sprite_id).copied())
        };
        Ok(HuntReads {
            defaults: MoveDefaults::of(
                master,
                step.world.parts.combat_of(self.id).abilities.look.as_ref(),
                roles,
            ),
            attack: AttackReads::of(
                AttackSource {
                    combat: step.world.parts.combat_of(self.id),
                    top_speed: step.world.top_speed_terms(step.player, self)?,
                    master,
                    masters: OwnerMasters::new(
                        &step.world.parts.own_master,
                        step.world.model.masters.of(step.player),
                    ),
                    graphics: &step.world.model.graphics,
                    ai: step.world.parts.unit_ai.get(self.id),
                    player_kind: owner.kind,
                    now: step.world.world_time,
                },
                roles,
                &step.world.model.tuning.turn,
            ),
            work_rate: step.world.parts.combat_of(self.id).work_rate_powerup
                + master.action_part().work_rate,
            capacity: Amount::of_master(master.capacity),
            productivity,
            decay: if owner.attributes.get(292) == Some(&Amount::new(1.0)) {
                DecayRule::Stops
            } else {
                DecayRule::Runs
            },
            has_ai: step.world.parts.unit_ai.contains(self.id),
            ai_class: step
                .world
                .parts
                .unit_ai
                .get(self.id)
                .and_then(|ai| ai.target_class),
            tree_searched: tree_lost,
            predator_hunts_end: step.world.settings.predator_hunts_end(),
            sprites: HuntSprites {
                moving: graphic_of(hunt.task.sprites.moving),
                working: graphic_of(hunt.task.sprites.working),
                carrying: graphic_of(hunt.task.sprites.carrying),
                standing: graphic_of(master.standing_sprite),
            },
            herd,
        })
    }

    /// The hunt's end of an Attack that failed (`FUN_140ac4670` with -1 and 5): a hunter whose
    /// unit AI targets trees (its class, else the hunt's, that the civilian's slot 0x1b0 gives
    /// 0) takes the next tree within 5 of it ([`Unit::hunt_tree_search`]); any other, or one
    /// that finds none, searches (`set_state(3)` with the target kept,
    /// [`Unit::hunt_search_state`]).
    pub(crate) fn hunt_attack_failed(
        &mut self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let tree = step.world.parts.unit_ai.get(self.id).is_some_and(|ai| {
            let class = ai.target_class.or(hunt.target_class);
            roles.of_some_class(class).contains(ClassRoles::TREE)
        });
        if tree
            && self.hunt_tree_search(
                (hunt, actions),
                step,
                None,
                Some(Distance::new(step.world.model.tuning.hunt.tree_near)),
                roles,
            )? == TreeSearch::Found
        {
            return Ok(());
        }
        let torn = hunt.tear_down(step.world.parts.mover_mut(self.id));
        if torn.is_ok() {
            hunt.state = HuntState::Search;
        }
        actions.list.set_hunt(Some(*hunt));
        torn?;
        self.hunt_search_state(hunt, actions, step, roles)
    }

    /// What `set_state(6)` on live livestock reads (`0x140ac60e0` case 6), and the lead's update
    /// (state 10, `0x140ac7a00` case 9: whatever the target, a dead one too) ([`Herd`]): the
    /// hunter's engagement and arms, the animal's owner, its drop site with the gap to it, the
    /// animal's idleness and the hunter's special graphic; none in the other states and none
    /// without the hunt's target (`Herd::target`).
    fn herd_for(
        &self,
        step: &Step<'_>,
        others: OthersRef<'_>,
        action: &HuntAction,
        master: &Master,
        victim: &Victim,
        roles: &Roles,
    ) -> GenieResult<Option<Herd>> {
        // the game reads the herd only on the hunt's target: a lead without one searched before
        // the strike (`Unit::hunt_part`)
        let Some(target) = action.target else {
            return Ok(None);
        };
        let faces = matches!(action.state, HuntState::Walk { .. })
            && action
                .state
                .move_to()
                .is_some_and(|move_to_action| move_to_action.state == MoveTo::Arrived);
        if !((faces
            && roles.of_class(victim.class).contains(ClassRoles::LIVESTOCK)
            && victim.state <= ObjectState::Alive)
            || action.state == HuntState::Lead)
        {
            return Ok(None);
        }
        let at = |id: Option<ObjectId>| {
            step.world
                .spatial
                .get_known(id)?
                .ok_or(GenieError::unsupported("a hunt's object without a place"))
        };
        let (me_at, them) = (at(Some(self.id))?, at(Some(target.id()))?);
        let speed = step.world.top_speed(step.player, self)?;
        let engagement = engagement(
            step.world.parts.unit_ai.get(self.id),
            speed,
            master,
            me_at,
            them,
            step.world.world_time,
        )?;
        let moving = action.task.walk_sprite_id(self.life.held());
        let arms = arms(
            master,
            &step.world.model.graphics,
            action.task.sprites,
            moving,
            roles,
        );
        let own = them.player == step.player;
        let drop_site = if !own {
            None
        } else if let Some(site_id) = action.drop_site {
            let site = step.world.spatial.get(site_id);
            site.map(|placed| {
                let distance_squared = footprint_gap_squared(
                    them.position,
                    them.radius,
                    placed.position,
                    placed.radius,
                );
                (site_id, distance_squared)
            })
        } else {
            if step.world.players.player(step.player)?.kind.is_computer() {
                return Err(GenieError::unsupported(
                    "a computer player's hunt of its livestock (its drop-site search)",
                ));
            }
            let sites: Vec<MasterId> = step
                .world
                .model
                .drop_sites
                .iter()
                .filter(|drop_site| drop_site.livestock)
                .map(|drop_site| drop_site.building)
                .collect();
            nearest_drop_site(others, &sites, them.position)
                .map(|(id, _)| -> GenieResult<(DropSiteId, DistanceSquared)> {
                    let placed = at(Some(id.id()))?;
                    Ok((
                        id,
                        footprint_gap_squared(
                            them.position,
                            them.radius,
                            placed.position,
                            placed.radius,
                        ),
                    ))
                })
                .transpose()?
        };
        // the animal's top action without a target or a point (slots 0x190, 0x288)
        let animal = others.get(Some(target));
        let idle = match animal {
            Some(unit) => step
                .world
                .parts
                .actions_of(unit.id)
                .top_action_idle(step.world.parts.unread.get(unit.id))?,
            None => false,
        };
        Ok(Some(Herd {
            target,
            own,
            drop_site,
            engagement,
            arms,
            idle,
            special_graphic: match master.creatable_part().special_sprite {
                Some(sprite_id) if master.kind == MasterKind::COMBAT => {
                    Some(step.world.model.graphics.get(sprite_id).copied().ok_or(
                        GenieError::unsupported("a special graphic the data file does not have"),
                    )?)
                }
                _ => None,
            },
        }))
    }
}

/// The object the hunt's strike reads of the world ([`victim`]): its target, else
/// its Attack's. A hunt whose start dropped its target (gone or past state 6) and holds no
/// Attack reads none (state 7's end without a target): a stand-in no object holds
/// ([`ObjectId::NOBODY`]).
fn hunt_victim(step: &Step<'_>, action: &HuntAction, roles: &Roles) -> GenieResult<Victim> {
    let target = action.target.map_or_else(
        || {
            action
                .state
                .attack()
                .and_then(|attack| attack.target.map(AttackTargetId::id))
        },
        |role_id| Some(role_id.id()),
    );
    match target {
        None => Ok(Victim {
            id: ObjectId::NOBODY,
            ..Victim::default()
        }),
        Some(object_id) => victim(step, object_id, roles)?.ok_or(GenieError::unsupported(
            "an attack's target the slice does not hold live",
        )),
    }
}
