//! The hunt's strike as the hunter's update runs it: the hunter as a gatherer and an attacker,
//! the herd it leads, the hunt's update and what follows.

use crate::GenieError;
use crate::combat::attack::{AttackReads, Fight};
use crate::combat::damage::Victim;
use crate::combat::victims::{KillLeft, file_kill, victim};
use crate::gathering::drop_sites::nearest_drop_site;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::NO_HUNT;
use crate::gathering::hunt::{Herd, HuntAction, HuntNext, HuntReads, HuntSprites};
use crate::map::DistanceSquared;
use crate::movement::Speed;
use crate::movement::move_to::{MoveDefaults, MoveTo};
use crate::movement::{Me, footprint_gap_squared};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::Amount;
use crate::world::TREE_NEAR;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

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
        c: &mut Step<'_>,
        me: Me,
        own: &mut Option<Victim>,
        tree_lost: bool,
    ) -> Result<(), GenieError> {
        let master = object_master(
            self.own_master.as_deref(),
            (self.id, self.life.master_id),
            c.w.masters.of(c.player),
        )?;
        let mut hunt = *self.actions.hunt().ok_or(NO_HUNT)?;
        let victim = hunt_victim(c, &hunt)?;
        let own = own.take().ok_or(crate::world::HUNTER_MASTER)?;
        let herd = self.herd_for(
            c,
            OthersRef::of(&c.w.players, c.player)?,
            &hunt,
            master,
            &victim,
        )?;
        let r = self.hunt_reads(c, master, &hunt, herd, tree_lost)?;
        let mut f = Fight { victim, own };
        // written back before an error too: the stop's context reads the list
        let next = hunt.update(c, self, me, &mut f, &r);
        if let Some(h) = self.actions.hunt_mut() {
            *h = hunt;
        }
        let next = next?;
        // the retaliation's kill of the unit itself, which follows its step
        if let Some(k) = self.hit_back(f.own)? {
            file_kill(c, self, KillLeft::own(self.id, k))?;
        }
        match next {
            HuntNext::Stay => Ok(()),
            HuntNext::Drop => self.hunt_drop_off(c),
            HuntNext::Search => self.hunt_set_search(c),
            HuntNext::AttackFailed => self.hunt_attack_failed(c),
        }
    }

    /// What the hunt's update reads of the hunter of `master` in `hunt` ([`HuntReads`]): its
    /// Attack's reads ([`AttackReads::of`]); the master's work rate (`+0x798`) plus the powerup
    /// (slot 0x6e8), the capacity, the productivity of the task's attribute; the decay's stop
    /// (attribute 292), the unit AI's target class, the next tree's search done (`tree_lost`),
    /// the game's predator setting, the task's sprites and the herd it leads (`herd`).
    pub(crate) fn hunt_reads(
        &self,
        c: &Step<'_>,
        master: &Master,
        hunt: &HuntAction,
        herd: Option<Herd>,
        tree_lost: bool,
    ) -> Result<HuntReads, GenieError> {
        let owner = c.w.players.player(c.player)?;
        let productivity = (hunt.task.productivity)
            .and_then(|i| owner.attributes.get(i.index()))
            .copied()
            .filter(|&v| Amount::ZERO < v)
            .unwrap_or(Amount::new(1.0));
        let g = |id: Option<SpriteId>| id.and_then(|s| c.w.graphics.get(&s).copied());
        Ok(HuntReads {
            defaults: MoveDefaults::of(master, self.abilities.look.as_ref()),
            attack: AttackReads::of(
                self,
                master,
                (c.w.masters.of(c.player), &c.w.graphics),
                self.ai.as_ref(),
                (owner.computer, c.w.world_time),
            ),
            work_rate: self.work_rate_powerup + master.work_rate,
            capacity: Amount::of_master(master.capacity),
            productivity: productivity.get(),
            stops_decay: owner.attributes.get(292) == Some(&Amount::new(1.0)),
            has_ai: self.ai.is_some(),
            ai_class: self.ai.as_ref().and_then(|ai| ai.target_class),
            tree_searched: tree_lost,
            predator_hunts_end: c.w.settings.predator_hunts_end(),
            sprites: HuntSprites {
                moving: g(hunt.task.sprites.moving),
                working: g(hunt.task.sprites.working),
                carrying: g(hunt.task.sprites.carrying),
                standing: g(master.standing_sprite),
            },
            herd,
        })
    }

    /// The hunt's end of an Attack that failed (`FUN_140ac4670` with -1 and 5): a hunter whose
    /// unit AI targets trees (its class, else the hunt's, that the civilian's slot 0x1b0 gives
    /// 0) takes the next tree within 5 of it ([`Unit::hunt_tree_search`]); any other, or one
    /// that finds none, searches (`set_state(3)` with the target kept,
    /// [`Unit::hunt_search_state`]).
    pub(crate) fn hunt_attack_failed(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let tree = self.ai.as_ref().is_some_and(|ai| {
            let class = ai
                .target_class
                .or_else(|| self.actions.hunt().and_then(|h| h.target_class));
            class == Some(class::TREE)
        });
        if tree && self.hunt_tree_search(c, None, Some(TREE_NEAR))? {
            return Ok(());
        }
        let hunt = self.actions.hunt_mut().ok_or(NO_HUNT)?;
        hunt.tear_down(self.body.mover_mut())?;
        hunt.state = HuntState::Search;
        self.hunt_search_state(c)
    }

    /// What `set_state(6)` on live livestock reads (`0x140ac60e0` case 6), and the lead's update
    /// (state 10, `0x140ac7a00` case 9: whatever the target, a dead one too) ([`Herd`]): the
    /// hunter's engagement and arms, the animal's owner, its drop site with the gap to it, the
    /// animal's idleness and the hunter's special graphic; none in the other states.
    fn herd_for(
        &self,
        c: &Step<'_>,
        others: OthersRef<'_>,
        action: &HuntAction,
        master: &Master,
        victim: &Victim,
    ) -> Result<Option<Herd>, GenieError> {
        let faces = action.state == HuntState::Walk
            && action.move_to.is_some_and(|m| m.state == MoveTo::Arrived);
        if !((faces && victim.class == class::LIVESTOCK && victim.state <= ObjectState::Alive)
            || action.state == HuntState::Lead)
        {
            return Ok(None);
        }
        let at = |id: Option<ObjectId>| {
            c.w.spatial
                .get_known(id)?
                .ok_or(GenieError::unsupported("a hunt's object without a place"))
        };
        let (me_at, them) = (at(Some(self.id))?, at(action.target)?);
        let speed = self.body.mover().map_or(Speed::ZERO, |m| m.stats.speed);
        let engagement = crate::combat::attack::engagement(
            self.ai.as_ref(),
            speed,
            master,
            me_at,
            them,
            c.w.world_time,
        )?;
        let moving = action.task.walk_sprite_id(self.life.held());
        let arms = crate::combat::attack::arms(master, &c.w.graphics, action.task.sprites, moving);
        let own = them.player == c.player;
        let drop_site = if !own {
            None
        } else if let Some(site_id) = action.drop_site {
            let site = c.w.spatial.get(site_id);
            site.map(|s| {
                let d = footprint_gap_squared(them.position, them.radius, s.position, s.radius);
                (site_id, d)
            })
        } else {
            if c.w.players.player(c.player)?.computer {
                return Err(GenieError::unsupported(
                    "a computer player's hunt of its livestock (its drop-site search)",
                ));
            }
            let sites: Vec<MasterId> =
                c.w.drop_sites
                    .iter()
                    .filter(|d| d.livestock)
                    .map(|d| d.building)
                    .collect();
            nearest_drop_site(others, &sites, them.position)
                .map(
                    |(id, _)| -> Result<(ObjectId, DistanceSquared), GenieError> {
                        let s = at(Some(id))?;
                        Ok((
                            id,
                            footprint_gap_squared(them.position, them.radius, s.position, s.radius),
                        ))
                    },
                )
                .transpose()?
        };
        // the animal's top action without a target or a point (slots 0x190, 0x288)
        let animal = others.get(action.target);
        let idle = match animal {
            Some(u) => u.top_action_idle()?,
            None => false,
        };
        Ok(Some(Herd {
            own,
            drop_site,
            engagement,
            arms,
            idle,
            special_graphic: if master.kind == COMBAT && master.special_graphic {
                Some(
                    master
                        .special_sprite
                        .and_then(|s| c.w.graphics.get(&s))
                        .copied()
                        .ok_or(GenieError::unsupported(
                            "a special graphic the data file does not have",
                        ))?,
                )
            } else {
                None
            },
        }))
    }
}

/// The object the hunt's strike reads of the world ([`victim`]): its target, else
/// its Attack's. A hunt whose start dropped its target (gone or past state 6) and holds no
/// Attack reads none (state 7's end without a target): a stand-in no object holds
/// ([`ObjectId::NOBODY`]).
fn hunt_victim(c: &Step<'_>, action: &HuntAction) -> Result<Victim, GenieError> {
    let target = match action.target {
        None => action.attack.and_then(|a| a.target),
        t => t,
    };
    match target {
        None => Ok(Victim {
            id: ObjectId::NOBODY,
            ..Victim::default()
        }),
        Some(t) => victim(c, t)?.ok_or(GenieError::unsupported(
            "an attack's target the slice does not hold live",
        )),
    }
}
