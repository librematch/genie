//! What happens to an object once it is dead: the state steps of `RGE_Static_Object::update`
//! (158041 `0x14118ad50`) and `RGE_Animated_Object::update` (`0x141071880`), up to the update that
//! returns 1 and so takes the object off its player's list (`RGE_Object_List::update`,
//! `0x141166550`).
//!
//! The states are the save's `object_state`: 2 alive, 3 dying, 4 playing its dying sprite, 5
//! dead, 7 its resource going back, 8 gone. An object's death spawn (state 7) and an undead state
//! are [`GenieError`].

use crate::GenieError;
use crate::math::rng::Well512;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::AmountRate;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use genie_diff::Diff;

pub(crate) mod death_parts;
mod object_state;

pub use self::object_state::ObjectState;

/// What the dead-object steps read from an object's master that the header save does not hold;
/// they come from the `.dat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct MasterData {
    /// Master `+0x620`: how much of the held attribute a dead object loses per second.
    pub decay: AmountRate,
    /// Master `+0x490`: a dead object turns undead (state 6) instead of leaving.
    pub undead: bool,
}

/// Which update chain the object runs. `RGE_Animated_Object::update` and everything derived from
/// it (types 20 and up) adds a step after the static update's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub enum Class {
    /// Type 10, `RGE_Static_Object`.
    #[default]
    Static,
    /// Types 20 and up.
    Animated,
}

/// What an object holds (`+0x308` and `+0x310`), as a reader of the object takes it: the
/// attribute (none for −1) and the amount.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Held {
    /// The attribute, none for −1.
    pub kind: Option<AttributeId>,
    /// The amount.
    pub amount: Amount,
}

/// The static init's storage (`FUN_1411a45a0`) of a new object of a master with storage
/// `storage`: the entries of flags 1, 2 and 32 with an attribute go to its `owner` at once
/// (`FUN_141193860`); the last entry of flag 0 with one is what the object holds
/// (`FUN_141193710`; none without one).
///
/// # Errors
///
/// An attribute the owner does not have.
pub(crate) fn static_storage(
    owner: &mut crate::world::player::Player,
    storage: &[(Option<AttributeId>, Amount, u8)],
) -> Result<Held, GenieError> {
    let mut held = Held::default();
    for &(kind, amount, flag) in storage {
        let Some(kind) = kind else {
            continue;
        };
        if matches!(flag, 1 | 2 | 32) {
            owner.add_attribute(kind, amount)?;
        }
        if flag == 0 {
            held = Held {
                kind: Some(kind),
                amount,
            };
        }
    }
    Ok(held)
}

/// The fields the dead-object steps read and write, with their offsets in 158041.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Lifecycle {
    /// The object's class: static or animated.
    pub class: Class,
    /// `+0x640`, the save's `object_state`.
    pub state: ObjectState,
    /// The masked hit points (`+0x278`).
    pub hp: HitPoints,
    /// `+0x308`: the attribute of what the object holds, none for −1.
    pub attribute_type: Option<AttributeId>,
    /// `+0x310`: how much it holds.
    pub amount: Amount,
    /// The master's death sprite (`+0x480`, the slot 0x7a0's): an animated object with one plays
    /// it in state 4.
    #[diff(gap = "stays the import's master's after a master change (a villager's new task)")]
    pub dying_sprite: Option<i16>,
    /// The master spawns an object when this one is gone (its death spawn `+0x5e4`, state 7).
    pub death_spawn: bool,
    /// The master's `death_spawn_obj_id2` (`+0x5e6`, the `.dat`'s `blood_unit_id`, −1 for none):
    /// what an object below type 80 leaves when its dying sprite ends (`FUN_1411a4e90`, the
    /// caller's).
    pub remains: Option<MasterId>,
    /// A static object's sprite after its death (the master's dying sprite, else its current
    /// one) starts on a random facet (sequence flag 4) of this many (more than one): its death
    /// draws the facet from the Object stream.
    pub death_facets: Option<i16>,
    /// `+0x2f0`: the object it is inside (its `inside_obj`), whose update decides for it.
    pub inside: Option<ObjectId>,
    /// `+0x643`: the next update moves the object to the sleeping list.
    pub goto_sleep: bool,
    /// The master's id.
    pub master_id: MasterId,
    /// What the dead-object steps read of the master.
    pub master: Option<MasterData>,
}

impl Default for Lifecycle {
    /// A live object that holds nothing.
    fn default() -> Self {
        Self {
            class: Class::Static,
            state: ObjectState::Alive,
            hp: HitPoints::new(1.0),
            attribute_type: None,
            amount: Amount::ZERO,
            dying_sprite: None,
            death_spawn: false,
            remains: None,
            death_facets: None,
            inside: None,
            goto_sleep: false,
            master_id: MasterId::default(),
            master: None,
        }
    }
}

impl Lifecycle {
    /// What the object holds.
    #[must_use]
    pub const fn held(&self) -> Held {
        Held {
            kind: self.attribute_type,
            amount: self.amount,
        }
    }

    /// Whether the unit AI still runs: slot 4 (`0x140c66b10`) returns at once above state 2.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.state <= ObjectState::Alive
    }

    /// One update of `dt_seconds` ([`seconds`]), with the tick's Object stream and whether the
    /// object's current sprite still animates (the sprite list's slot 0x20, `FUN_1413b8790` ->
    /// `FUN_1413b38a0`: its animate flag on and the graphic animated). True when the object leaves
    /// its list.
    pub fn update(
        &mut self,
        dt_seconds: Seconds,
        object: &mut Well512,
        animating: bool,
    ) -> Result<bool, GenieError> {
        if self.goto_sleep {
            // the object's update moves it to the sleeping list first (`Unit::update`)
            return Err(GenieError::unsupported("the object goes to sleep"));
        }
        match self.state {
            ObjectState::Foundation | ObjectState::Alive => {
                if self.hp < HitPoints::new(1.0) {
                    self.die(object)?;
                }
                Ok(false)
            }
            ObjectState::Dying | ObjectState::Dead => {
                self.dead(dt_seconds)?;
                // RGE_Animated_Object::update after the static step: state 3 goes on to 4 with a
                // dying sprite (slot 0x7a0), else 5 (slot 0xa0)
                if self.state == ObjectState::Dying && self.class == Class::Animated {
                    self.state = if self.dying_sprite.is_some() {
                        ObjectState::DyingSprite
                    } else {
                        ObjectState::Dead
                    };
                }
                Ok(false)
            }
            // RGE_Animated_Object::update: the dying sprite plays; once it stops, state 5 (the
            // caller clears the animation speed, `+0x7b8`, and leaves the remains)
            ObjectState::DyingSprite => {
                if !animating {
                    self.state = ObjectState::Dead;
                }
                Ok(false)
            }
            // RGE_Static_Object::update: an object of a resource (FUN_140fdd470) leaves Gaia's
            // list of that resource (FUN_14165b910: player +0x50, +0x48, +0x60, +0x58 for kinds
            // 0 to 3, for Gaia's objects only), which the slice does not hold; then state 8
            ObjectState::Removed => {
                self.state = ObjectState::Gone;
                Ok(false)
            }
            ObjectState::Gone => Ok(true),
            _ => Err(GenieError::unsupported("object state not modelled")),
        }
    }

    /// The static update's death (state 3, `set_state` `FUN_14119d710`), then the animated update's
    /// step to state 5. A static object takes its dying sprite (slot 0x68) and, starting on a
    /// random facet, draws it (`rand15 × facets / 0x7fff`, at most the last; the facet, `+0xe`, is
    /// not held). The storage the caller checks, and the players' fog ghosts of a master whose
    /// fog visibility is 2 (`FUN_14118e990`, for players that explored the tile but do not see
    /// it) are the caller's too ([`crate::vision::ghost`]); the statistics and the sounds are not held.
    /// The visible-resource lists (`FUN_1411980b0`) are the caller's
    /// ([`crate::objects::lifecycle::death_parts::forget_resource`]).
    fn die(&mut self, object: &mut Well512) -> Result<(), GenieError> {
        if self.inside.is_some() {
            return Err(GenieError::unsupported("an object inside another dies"));
        }
        // master 61 (the marlin) of Gaia, without a dying sprite and first holding fish (17),
        // goes to state 5 at once in its state-2 update (`FUN_14118ad50`, `set_state(5)`): the
        // same end as this death of an animated object without a dying sprite, whose
        // hit points (0) bring it here in the same update
        self.state = ObjectState::Dying;
        if let Some(facets) = self.death_facets.filter(|_| self.class == Class::Static) {
            let facet = (object.rand15() as i32 * i32::from(facets)) / 0x7fff;
            let _facet = facet.min(i32::from(facets) - 1);
        }
        // RGE_Animated_Object::update: state 4 with a dying sprite (which the static update gave
        // the object, slot 0x68: the caller's), else 5
        if self.class == Class::Animated {
            self.state = if self.dying_sprite.is_some() {
                ObjectState::DyingSprite
            } else {
                ObjectState::Dead
            };
        }
        Ok(())
    }

    /// States 3 and 5: once nothing is held the object moves on to state 7; until then the held
    /// amount decays, never below 0.
    fn dead(&mut self, dt_seconds: Seconds) -> Result<(), GenieError> {
        let master = self
            .master
            .ok_or(GenieError::unsupported("no master data for a dead object"))?;
        if self.attribute_type.is_none() || self.amount <= Amount::ZERO {
            if master.undead {
                return Err(GenieError::unsupported("an undead object (state 6)"));
            }
            // set_state(7), then its death spawn (slot 0x160, the caller's)
            self.state = ObjectState::Removed;
        } else if master.decay > AmountRate::ZERO {
            self.amount -= master.decay * dt_seconds;
            if self.amount < Amount::ZERO {
                self.amount = Amount::ZERO;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::attribute;
    use crate::players::attribute::AttributeId;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::world::Millis;

    /// The states after each update until the object leaves, and the update (1-based) it leaves in.
    fn run(mut life: Lifecycle, dt: Millis, limit: usize) -> (Vec<ObjectState>, Option<usize>) {
        let mut states = Vec::new();
        for i in 1..=limit {
            if life
                .update(dt.seconds(), &mut Well512::default(), false)
                .unwrap()
            {
                return (states, Some(i));
            }
            states.push(life.state);
        }
        (states, None)
    }

    fn dead(class: Class) -> Lifecycle {
        Lifecycle {
            class,
            hp: HitPoints::ZERO,
            master: Some(MasterData::default()),
            ..Lifecycle::default()
        }
    }

    #[test]
    fn an_object_with_hit_points_stays_alive() {
        let (states, left) = run(Lifecycle::default(), Millis::new(104), 20);
        assert_eq!((states, left), (vec![ObjectState::Alive; 20], None));
    }

    #[test]
    fn a_dead_static_object_holds_its_state_while_its_amount_decays_then_leaves() {
        // FLARE3 (MarvinBlue/426048731 object 10025): 1.0 of attribute 12, decay 1.0 per second;
        // the game's states after updates 1 to 13, then it is gone in update 14
        let flare = Lifecycle {
            attribute_type: Some(AttributeId::numbered(12)),
            amount: Amount::new(1.0),
            master: Some(MasterData {
                decay: AmountRate::new(1.0),
                undead: false,
            }),
            ..dead(Class::Static)
        };
        let (states, left) = run(flare, Millis::new(104), 20);
        let mut expected = vec![ObjectState::Dying; 11];
        expected.extend([ObjectState::Removed, ObjectState::Gone]);
        assert_eq!((states, left), (expected, Some(14)));
    }

    #[test]
    fn a_dead_animated_object_without_a_death_sprite_leaves_in_its_fourth_update() {
        // PLACEHOLDER2 and the empty town-centre annex of the same recording
        let (states, left) = run(dead(Class::Animated), Millis::new(104), 20);
        assert_eq!(
            (states, left),
            (
                vec![ObjectState::Dead, ObjectState::Removed, ObjectState::Gone],
                Some(4)
            )
        );
    }

    #[test]
    fn the_decayed_amount_never_drops_below_zero() {
        let mut life = Lifecycle {
            state: ObjectState::Dying,
            attribute_type: Some(AttributeId::numbered(12)),
            amount: Amount::new(0.05),
            master: Some(MasterData {
                decay: AmountRate::new(1.0),
                undead: false,
            }),
            ..dead(Class::Static)
        };
        let _ = life
            .update(Millis::new(104).seconds(), &mut Well512::default(), false)
            .unwrap();
        assert_eq!(
            (life.state, life.amount),
            (ObjectState::Dying, Amount::ZERO)
        );
    }

    /// `0x141071880`: an animated object in state 3 after the static step (one that holds an
    /// amount, loaded so) goes on to state 4 with a dying sprite, else 5; a static one stays.
    #[test]
    fn an_animated_object_in_state_3_goes_on_to_4_or_5() {
        let held = |class, dying_sprite| Lifecycle {
            state: ObjectState::Dying,
            attribute_type: Some(attribute::MEAT),
            amount: Amount::new(100.0),
            dying_sprite,
            ..dead(class)
        };
        let step = |mut l: Lifecycle| {
            let _ = l
                .update(Seconds::new(0.025), &mut Well512::default(), true)
                .unwrap();
            l.state
        };
        assert_eq!(
            step(held(Class::Animated, Some(8220))),
            ObjectState::DyingSprite
        );
        assert_eq!(step(held(Class::Animated, None)), ObjectState::Dead);
        assert_eq!(step(held(Class::Static, None)), ObjectState::Dying);
    }

    /// MarvinBlue/426048731's sheep 9520 (dying sprite 8220): killed, it dies into state 4, which
    /// holds while its dying sprite animates (updates 1252 to 1333), then 5.
    #[test]
    fn a_dying_sprite_holds_state_4_while_it_animates() {
        let mut sheep = Lifecycle {
            dying_sprite: Some(8220),
            ..dead(Class::Animated)
        };
        let mut rng = Well512::default();
        let _ = sheep.update(Seconds::new(0.025), &mut rng, true).unwrap();
        assert_eq!(sheep.state, ObjectState::DyingSprite);
        let _ = sheep.update(Seconds::new(0.025), &mut rng, true).unwrap();
        assert_eq!(sheep.state, ObjectState::DyingSprite);
        let _ = sheep.update(Seconds::new(0.025), &mut rng, false).unwrap();
        assert_eq!(sheep.state, ObjectState::Dead);
    }

    /// The marlin (master 61) without a dying sprite, at 0 hit points, ends in state 5 in the
    /// update it dies, as the game's direct `set_state(5)` leaves it.
    #[test]
    fn a_marlin_goes_to_state_5_at_once() {
        let mut marlin = Lifecycle {
            master_id: master_id(61),
            dying_sprite: None,
            ..dead(Class::Animated)
        };
        marlin.state = ObjectState::Alive;
        let _ = marlin
            .update(Seconds::new(0.025), &mut Well512::default(), false)
            .unwrap();
        assert_eq!(marlin.state, ObjectState::Dead);
    }

    #[test]
    fn the_paths_not_modelled_are_unsupported() {
        let cases = [
            Lifecycle {
                inside: Some(object_id(1)),
                ..dead(Class::Static)
            },
            Lifecycle {
                goto_sleep: true,
                ..Lifecycle::default()
            },
            Lifecycle {
                state: ObjectState::Dead,
                master: Some(MasterData {
                    decay: AmountRate::ZERO,
                    undead: true,
                }),
                ..dead(Class::Animated)
            },
            Lifecycle {
                state: ObjectState::Dead,
                master: None,
                ..dead(Class::Animated)
            },
        ];
        for (i, mut life) in cases.into_iter().enumerate() {
            assert!(
                life.update(Seconds::new(0.104), &mut Well512::default(), false)
                    .is_err(),
                "case {i}"
            );
        }
    }

    #[test]
    fn an_object_of_a_resource_in_state_7_goes_to_state_8_and_then_leaves() {
        // MarvinBlue/426048731's Gaia objects 0, 1, 43, 44 and 47 in the header's state 7
        let mut life = Lifecycle {
            state: ObjectState::Removed,
            attribute_type: Some(attribute::FOOD),
            ..dead(Class::Animated)
        };
        let mut object = Well512::default();
        assert_eq!(
            life.update(Seconds::new(0.104), &mut object, false),
            Ok(false)
        );
        assert_eq!(life.state, ObjectState::Gone);
        assert_eq!(
            life.update(Seconds::new(0.104), &mut object, false),
            Ok(true)
        );
    }

    #[test]
    fn a_felled_tree_draws_its_facet_and_keeps_its_wood() {
        // MarvinBlue/426048731's tree 9357 in update 1128: its second hit left 0 hit points; it
        // dies (state 3) and its falling sprite takes a random facet from the Object stream
        let mut tree = Lifecycle {
            attribute_type: Some(attribute::WOOD),
            amount: Amount::new(100.0),
            death_spawn: true,
            death_facets: Some(8),
            ..dead(Class::Static)
        };
        let mut object = Well512::default();
        object.set_seed(1);
        let _ = tree.update(Seconds::new(0.02), &mut object, false).unwrap();
        assert_eq!(
            (tree.state, tree.amount, object.draws),
            (ObjectState::Dying, Amount::new(100.0), 1)
        );
        // dead with its wood, it waits (no decay) and draws nothing more
        let _ = tree.update(Seconds::new(0.02), &mut object, false).unwrap();
        assert_eq!((tree.state, object.draws), (ObjectState::Dying, 1));
    }

    #[test]
    fn the_unit_ai_runs_up_to_state_2() {
        let mut life = dead(Class::Animated);
        assert!(life.is_alive());
        let _ = life
            .update(Seconds::new(0.104), &mut Well512::default(), false)
            .unwrap();
        assert!(!life.is_alive());
    }

    /// The static init's storage (a new missile's too): flags 1, 2 and 32 with an attribute
    /// go to the owner (resources and attributes alike), one without an attribute (−1) does
    /// not; the last of flag 0 is what the object holds.
    #[test]
    fn a_new_object_s_storage_gives_its_owner_flags_1_2_and_32_and_holds_flag_0() {
        let mut owner = crate::world::player::Player {
            attributes: vec![Amount::ZERO; 20],
            ..crate::world::player::Player::default()
        };
        let storage = [
            (Some(attribute::WOOD), 2.0, 1),
            (Some(attribute::POPULATION), 5.0, 2),
            (Some(AttributeId::numbered(9)), 2.0, 0),
            (None, 1.0, 32),
            (Some(AttributeId::numbered(12)), 3.0, 32),
            (Some(attribute::RELICS_HELD), 1.5, 0),
            (Some(AttributeId::numbered(13)), 4.0, 4),
        ]
        .map(|(kind, amount, flag)| (kind, Amount::new(amount), flag));
        assert_eq!(
            static_storage(&mut owner, &storage),
            Ok(Held {
                kind: Some(attribute::RELICS_HELD),
                amount: Amount::new(1.5)
            })
        );
        assert_eq!(
            owner.resources(),
            crate::test_ids::amounts([0.0, 2.0, 0.0, 0.0])
        );
        let gained: Vec<(usize, f32)> = owner
            .attributes
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, v)| v != Amount::ZERO)
            .map(|(k, v)| (k, v.get()))
            .collect();
        assert_eq!(gained, [(1, 2.0), (11, 5.0), (12, 3.0)]);
        assert_eq!(static_storage(&mut owner, &[]), Ok(Held::default()));
    }
}
