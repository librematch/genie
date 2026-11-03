//! A view of the generated object layouts (`generated/world.rs`): the parts the object classes
//! share. Each class's struct holds its base class's as its first field, as the game's loads call
//! the base load first; the factories hold one optional struct per object type.

use super::generated::world::{
    Action, ActionList, Master, Object, ObjectList, OwnRgeMasterActionObject,
    OwnRgeMasterAnimatedObject, OwnRgeMasterMovingObject, OwnRgeMasterStaticObject,
    OwnTribeMasterCombatObject, Player, PlayerObjects, RgeAction, RgeActionObject,
    RgeAnimatedObject, RgeMasterActionObject, RgeMasterMovingObject, RgeMasterStaticObject,
    RgeMovingObject, RgeObject, RgeStaticObject, TribeCombatObject, TribeMasterCombatObject,
    UnitAiModule, WorldPlayer,
};

impl Player {
    /// The player load of its class (`WorldPlayer.cpp`); None for a type no creator handles.
    #[must_use]
    pub fn world_player(&self) -> Option<&WorldPlayer> {
        None.or_else(|| {
            self.scenario_editor_phantom_player
                .as_deref()
                .map(|p| &p.world_player_scenario_editor_phantom)
        })
        .or_else(|| {
            self.human_or_coop_player
                .as_deref()
                .map(|p| &p.world_player_human_or_coop)
        })
        .or_else(|| {
            self.gaia_player
                .as_deref()
                .map(|p| &p.world_player_gaia.world_player)
        })
        .or_else(|| {
            self.computer_player
                .as_deref()
                .map(|p| &p.world_player_computer)
        })
    }

    /// The masters, maps and objects (`WorldPlayer` vtable slot 29).
    #[must_use]
    pub fn objects(&self) -> Option<&PlayerObjects> {
        None.or_else(|| {
            self.scenario_editor_phantom_player
                .as_deref()
                .map(|p| &p.player_objects)
        })
        .or_else(|| {
            self.human_or_coop_player
                .as_deref()
                .map(|p| &p.player_objects)
        })
        .or_else(|| self.gaia_player.as_deref().map(|p| &p.player_objects))
        .or_else(|| self.computer_player.as_deref().map(|p| &p.player_objects))
    }
}

impl PlayerObjects {
    /// The player's master of type `id`; None if the save holds none (a master the player never
    /// enabled).
    #[must_use]
    pub fn master(&self, id: i16) -> Option<&Master> {
        self.masters
            .iter()
            .flatten()
            .map(|m| &m.master)
            .find(|m| m.static_part().is_some_and(|s| s.id() == id))
    }

    /// The `.dat` master whose values the player's master `id` holds where the save does not
    /// ([`RgeMasterStaticObject::copy_id`]): `id` itself unless an effect upgraded it, and `id`
    /// for a master the save does not hold.
    #[must_use]
    pub fn dat_master(&self, id: i16) -> i16 {
        self.master(id)
            .and_then(Master::static_part)
            .map_or(id, RgeMasterStaticObject::copy_id)
    }
}

impl Master {
    /// The static master every class starts with.
    #[must_use]
    pub fn static_part(&self) -> Option<&RgeMasterStaticObject> {
        if let Some(t) = &self.tribe_master_tree_object {
            return Some(t);
        }
        if let Some(c) = &self.tribe_master_combat_object {
            return Some(tribe_combat_master(c));
        }
        if let Some(b) = &self.tribe_master_building_object {
            return Some(tribe_combat_master(&b.tribe_master_combat_object));
        }
        let r = self.rge_master.as_deref()?;
        None.or(r.rge_master_static_object.as_deref())
            .or_else(|| {
                r.rge_master_animated_object
                    .as_deref()
                    .map(|a| &a.rge_master_static_object)
            })
            .or_else(|| {
                r.rge_master_doppleganger_object
                    .as_deref()
                    .map(|d| &d.rge_master_static_object)
            })
            .or_else(|| r.rge_master_moving_object.as_deref().map(moving_master))
            .or_else(|| r.rge_master_action_object.as_deref().map(action_master))
            .or_else(|| {
                r.rge_master_combat_object
                    .as_deref()
                    .map(|c| action_master(&c.rge_master_action_object))
            })
            .or_else(|| {
                r.rge_master_missile_object
                    .as_deref()
                    .map(|m| action_master(&m.rge_master_combat_object.rge_master_action_object))
            })
    }
}

impl RgeMasterStaticObject {
    /// The master's type id: the save writes it first (a `readTemp` local).
    #[must_use]
    pub const fn id(&self) -> i16 {
        self.read_temp
    }

    /// The master's `copy_id` (`+0x158`): the save writes it second. A master an effect upgraded
    /// (`UPGRADE_UNIT`) names the master it copies; the `.dat` values the save does not hold
    /// (its tasks, its default task) are that master's.
    #[must_use]
    pub const fn copy_id(&self) -> i16 {
        self.read_temp_2
    }
}

const fn moving_master(m: &RgeMasterMovingObject) -> &RgeMasterStaticObject {
    &m.rge_master_animated_object.rge_master_static_object
}

const fn action_master(a: &RgeMasterActionObject) -> &RgeMasterStaticObject {
    moving_master(&a.rge_master_moving_object)
}

const fn tribe_combat_master(c: &TribeMasterCombatObject) -> &RgeMasterStaticObject {
    action_master(&c.rge_master_combat_object.rge_master_action_object)
}

impl RgeStaticObject {
    /// The static part of the object's own master (`has_own_master_object`), which the object
    /// uses instead of its player's.
    #[must_use]
    pub fn own_master(&self) -> Option<&OwnRgeMasterStaticObject> {
        None.or(self.rge_master_static_object.as_deref())
            .or_else(|| {
                self.rge_master_animated_object
                    .as_deref()
                    .map(|a| &a.own_rge_master_static_object)
            })
            .or_else(|| {
                self.rge_master_moving_object.as_deref().map(|m| {
                    &m.own_rge_master_animated_object
                        .own_rge_master_static_object
                })
            })
            .or_else(|| {
                self.rge_master_action_object
                    .as_deref()
                    .map(own_action_master)
            })
            .or_else(|| {
                self.rge_master_missile_object.as_deref().map(|m| {
                    own_action_master(&m.own_rge_master_combat_object.own_rge_master_action_object)
                })
            })
            .or_else(|| {
                self.tribe_master_combat_object
                    .as_deref()
                    .map(own_tribe_combat_master)
            })
            .or_else(|| {
                self.tribe_master_building_object
                    .as_deref()
                    .map(|b| own_tribe_combat_master(&b.own_tribe_master_combat_object))
            })
    }
}

impl RgeStaticObject {
    /// The animated part of the object's own master, which holds its speed (`mMaxSpeed`); None
    /// for an object without an own master or one without an animated part.
    #[must_use]
    pub fn own_moving_master(&self) -> Option<&OwnRgeMasterMovingObject> {
        None.or(self.rge_master_moving_object.as_deref())
            .or_else(|| {
                self.rge_master_action_object
                    .as_deref()
                    .map(own_action_moving)
            })
            .or_else(|| {
                self.rge_master_missile_object.as_deref().map(|m| {
                    own_action_moving(&m.own_rge_master_combat_object.own_rge_master_action_object)
                })
            })
            .or_else(|| {
                self.tribe_master_combat_object.as_deref().map(|c| {
                    own_action_moving(&c.own_rge_master_combat_object.own_rge_master_action_object)
                })
            })
            .or_else(|| {
                self.tribe_master_building_object.as_deref().map(|b| {
                    own_action_moving(
                        &b.own_tribe_master_combat_object
                            .own_rge_master_combat_object
                            .own_rge_master_action_object,
                    )
                })
            })
    }

    /// The animated part of an own master copy, whichever of the master types holds it.
    #[must_use]
    pub fn own_animated_master(&self) -> Option<&OwnRgeMasterAnimatedObject> {
        None.or(self.rge_master_animated_object.as_deref())
            .or_else(|| {
                self.rge_master_moving_object
                    .as_deref()
                    .map(|m| &m.own_rge_master_animated_object)
            })
            .or_else(|| {
                self.rge_master_action_object
                    .as_deref()
                    .map(own_action_animated)
            })
            .or_else(|| {
                self.rge_master_missile_object.as_deref().map(|m| {
                    own_action_animated(
                        &m.own_rge_master_combat_object.own_rge_master_action_object,
                    )
                })
            })
            .or_else(|| {
                self.tribe_master_combat_object
                    .as_deref()
                    .map(own_combat_animated)
            })
            .or_else(|| {
                self.tribe_master_building_object
                    .as_deref()
                    .map(|b| own_combat_animated(&b.own_tribe_master_combat_object))
            })
    }
}

const fn own_action_moving(a: &OwnRgeMasterActionObject) -> &OwnRgeMasterMovingObject {
    &a.own_rge_master_moving_object
}

const fn own_action_animated(a: &OwnRgeMasterActionObject) -> &OwnRgeMasterAnimatedObject {
    &a.own_rge_master_moving_object
        .own_rge_master_animated_object
}

const fn own_combat_animated(c: &OwnTribeMasterCombatObject) -> &OwnRgeMasterAnimatedObject {
    own_action_animated(&c.own_rge_master_combat_object.own_rge_master_action_object)
}

const fn own_action_master(a: &OwnRgeMasterActionObject) -> &OwnRgeMasterStaticObject {
    &a.own_rge_master_moving_object
        .own_rge_master_animated_object
        .own_rge_master_static_object
}

const fn own_tribe_combat_master(c: &OwnTribeMasterCombatObject) -> &OwnRgeMasterStaticObject {
    own_action_master(&c.own_rge_master_combat_object.own_rge_master_action_object)
}

impl ObjectList {
    /// The objects in list order, without the entry of type 0 that ends the list.
    pub fn objects(&self) -> impl Iterator<Item = &Object> {
        self.object_kind_list
            .iter()
            .filter_map(|e| e.object.as_deref())
    }
}

impl Object {
    /// The static object every class starts with.
    pub fn static_part(&self) -> Option<&RgeStaticObject> {
        if let Some(c) = self.combat() {
            return Some(combat_static(c));
        }
        self.rge_object.as_deref().and_then(RgeObject::static_part)
    }

    /// The moving object of the types 30 and up: its velocity, orientation and path data.
    #[must_use]
    pub fn moving_part(&self) -> Option<&RgeMovingObject> {
        if let Some(c) = self.combat() {
            return Some(&c.rge_combat_object.rge_action_object.rge_moving_object);
        }
        let r = self.rge_object.as_deref()?;
        None.or(r.rge_moving_object.as_deref())
            .or_else(|| r.rge_action_object.as_deref().map(|a| &a.rge_moving_object))
            .or_else(|| {
                r.rge_combat_object
                    .as_deref()
                    .map(|c| &c.rge_action_object.rge_moving_object)
            })
            .or_else(|| {
                r.rge_missile_object
                    .as_deref()
                    .map(|m| &m.rge_combat_object.rge_action_object.rge_moving_object)
            })
    }

    /// The animated object of the types 20 and up: its sprite's clock speed (`current_speed`, the
    /// game's `+0x7b8`), its sprite list's owner.
    #[must_use]
    pub fn animated_part(&self) -> Option<&RgeAnimatedObject> {
        if let Some(m) = self.moving_part() {
            return Some(&m.rge_animated_object);
        }
        let r = self.rge_object.as_deref()?;
        r.rge_animated_object.as_deref().or_else(|| {
            r.rge_doppleganger_object
                .as_deref()
                .map(|d| &d.rge_animated_object)
        })
    }

    /// The tribe combat object of a combat object (type 70) or a building (type 80).
    #[must_use]
    pub fn combat(&self) -> Option<&TribeCombatObject> {
        self.tribe_combat_object.as_deref().or_else(|| {
            self.tribe_building_object
                .as_deref()
                .map(|b| &b.tribe_combat_object)
        })
    }
}

impl RgeObject {
    /// The static object of the base factory's types (10 to 60).
    #[must_use]
    pub fn static_part(&self) -> Option<&RgeStaticObject> {
        self.rge_static_object
            .as_deref()
            .or_else(|| {
                self.rge_animated_object
                    .as_deref()
                    .map(|a| &a.rge_static_object)
            })
            .or_else(|| {
                self.rge_doppleganger_object
                    .as_deref()
                    .map(|d| &d.rge_animated_object.rge_static_object)
            })
            .or_else(|| self.rge_moving_object.as_deref().map(moving))
            .or_else(|| self.rge_action_object.as_deref().map(action))
            .or_else(|| {
                self.rge_combat_object
                    .as_deref()
                    .map(|c| action(&c.rge_action_object))
            })
            .or_else(|| {
                self.rge_missile_object
                    .as_deref()
                    .map(|m| action(&m.rge_combat_object.rge_action_object))
            })
    }
}

const fn moving(m: &RgeMovingObject) -> &RgeStaticObject {
    &m.rge_animated_object.rge_static_object
}

const fn action(a: &RgeActionObject) -> &RgeStaticObject {
    moving(&a.rge_moving_object)
}

const fn combat_static(c: &TribeCombatObject) -> &RgeStaticObject {
    action(&c.rge_combat_object.rge_action_object)
}

impl TribeCombatObject {
    /// The unit AI, whatever class its type made: the base module, or the elephant, tower or
    /// building module, which start with it.
    #[must_use]
    pub fn unit_ai(&self) -> Option<&UnitAiModule> {
        self.unit_ai_module
            .as_deref()
            .or(self.unit_ai_module_2.as_deref())
            .or_else(|| {
                self.tribe_elephant_unit_ai_module
                    .as_deref()
                    .map(|m| &m.unit_ai_module)
            })
            .or_else(|| {
                self.tribe_tower_unit_ai_module
                    .as_deref()
                    .map(|m| &m.unit_ai_module)
            })
            .or_else(|| {
                self.tribe_building_unit_ai_module
                    .as_deref()
                    .map(|m| &m.unit_ai_module)
            })
    }
}

impl Object {
    /// The action list of an action object (types 40 to 80): its actions, the top one first.
    #[must_use]
    pub fn actions(&self) -> Option<&ActionList> {
        if let Some(c) = self.combat() {
            return Some(&c.rge_combat_object.rge_action_object.actions);
        }
        let r = self.rge_object.as_deref()?;
        None.or(r.rge_action_object.as_deref())
            .or_else(|| r.rge_combat_object.as_deref().map(|c| &c.rge_action_object))
            .or_else(|| {
                r.rge_missile_object
                    .as_deref()
                    .map(|m| &m.rge_combat_object.rge_action_object)
            })
            .map(|a| &a.actions)
    }
}

impl ActionList {
    /// The actions with their types, without the entry of type 0 that ends the list.
    pub fn entries(&self) -> impl Iterator<Item = (i16, &Action)> {
        self.action_kind_list
            .iter()
            .filter_map(|e| e.action.as_deref().map(|a| (e.action_kind, a)))
    }
}

impl Action {
    /// A `MoveTo`'s range (`+0x68`), for an action of that class.
    #[must_use]
    pub fn move_to_range(&self) -> Option<f32> {
        self.rge_action_by_kind
            .as_deref()
            .and_then(|b| b.rge_action_move_to.as_deref())
            .map(|m| m.range)
    }

    /// The base every action class starts with (`RGE_Action`: state, targets, timer, sub-actions),
    /// whatever class the action's type made.
    #[must_use]
    pub fn base(&self) -> Option<&RgeAction> {
        macro_rules! first {
            ($($own:ident).+ ; $($field:ident),* ; $($plain:ident),*) => {
                None$(.or(self.$field.as_deref().map(|a| &a.rge_action)))*
                    $(.or(self.$plain.as_deref()))*
                    .or(self.$($own).+.as_deref().and_then(|b| {
                        None.or(b.rge_action_move_to.as_deref().map(|a| &a.rge_action))
                            .or(b.rge_action_enter.as_deref().map(|a| &a.rge_action))
                            .or(b.rge_action_gather.as_deref().map(|a| &a.rge_action))
                            .or(b.rge_action_missile.as_deref().map(|a| &a.rge_action))
                            .or(b.rge_action_attack.as_deref().map(|a| &a.rge_action))
                            .or(b.rge_action_make.as_deref().map(|a| &a.rge_action))
                            .or(b.action_hero_power.as_deref().map(|a| &a.rge_action))
                            .or(b.rge_action_explore.as_deref())
                            .or(b.rge_action_bird.as_deref())
                            .or(b.rge_action_guard.as_deref())
                    }))
            };
        }
        first!(rge_action_by_kind;
            rge_action_enter, tribe_action_build, tribe_action_make_obj, tribe_action_make_tech,
            tribe_action_convert, tribe_action_repair, tribe_action_artifact,
            tribe_action_discovery_artifact, tribe_action_hunt, tribe_action_trade,
            tribe_action_wonder, tribe_action_gather, tribe_action_housing, tribe_action_pack,
            tribe_action_unpack, tribe_action_offboard_trade, tribe_action_unit_transform,
            tribe_action_wait_solidify;
            rge_action_transport, rge_action_transport_over_wall, tribe_action_heal,
            tribe_action_farm, tribe_action_pickup_relic, tribe_action_charge,
            tribe_action_capture, tribe_action_deliver_relic, tribe_action_shepherd)
    }
}
