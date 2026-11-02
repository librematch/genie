//! A test's world with its parts: fixtures that file a test object's parts beside its row
//! ([`World::with_parts`], [`World::split_layers`]) and the whole objects tests push.

use crate::movement::Mover;
use crate::objects::ObjectId;
use crate::objects::layers::Building;
use crate::objects::layers::Combat;
use crate::objects::layers::MotionLayer;
use crate::objects::masters::Master;
use crate::objects::unit::Unit;
use crate::unit_ai::UnitAi;
use crate::world::World;
use crate::world::parts::{Object, ObjectParts, Part, PartTable, Parts};

/// No own masters: a test's master lookups without a world.
pub(crate) fn no_own_masters() -> &'static PartTable<Master> {
    &no_parts().own_master
}

/// No parts: a test's reads by id without a world.
pub(crate) fn no_parts() -> &'static Parts {
    static NONE: std::sync::LazyLock<Parts> = std::sync::LazyLock::new(Parts::default);
    &NONE
}

impl Parts {
    /// A test's unit AI `ai` of object `id` (none removes it), a borrow apart from the lists.
    pub(crate) fn set_unit_ai(&mut self, id: ObjectId, ai: Option<UnitAi>) {
        match ai {
            Some(ai) => {
                let _ = self.unit_ai.insert(id, ai);
            }
            None => {
                let _ = self.unit_ai.remove(id);
            }
        }
    }
}

/// A test's table of the parts it lists, in their order.
impl<T> FromIterator<(ObjectId, T)> for PartTable<T> {
    fn from_iter<I: IntoIterator<Item = (ObjectId, T)>>(parts: I) -> Self {
        let mut t = Self::default();
        for (id, part) in parts {
            let _ = t.insert(id, part);
        }
        t
    }
}

impl Object {
    /// A test's object: row `unit` with the unit AI `ai` (a unit AI, or an `Option` of one).
    pub(crate) fn with_ai(unit: Unit, ai: impl Into<Option<UnitAi>>) -> Self {
        Self {
            unit,
            parts: ObjectParts {
                unit_ai: ai.into(),
                ..ObjectParts::default()
            },
        }
    }

    /// A test's object: row `unit` with the combat layer `layer`.
    pub(crate) fn with_combat(unit: Unit, layer: Combat) -> Self {
        Self {
            unit,
            parts: ObjectParts {
                combat: layer,
                ..ObjectParts::default()
            },
        }
    }

    /// A test's object: row `unit` with the moving layers of `mover` (its motion part).
    pub(crate) fn moving(unit: Unit, mover: Mover) -> Self {
        Self::with_motion(unit, MotionLayer::Moving(mover))
    }

    /// A test's object: row `unit` with the motion part `motion`.
    pub(crate) fn with_motion(unit: Unit, motion: MotionLayer) -> Self {
        Self {
            unit,
            parts: ObjectParts {
                motion: Some(motion),
                ..ObjectParts::default()
            },
        }
    }

    /// A test's object: row `unit` with the building layer `layer`.
    pub(crate) fn with_building(unit: Unit, layer: Building) -> Self {
        Self {
            unit,
            parts: ObjectParts {
                building: layer,
                ..ObjectParts::default()
            },
        }
    }
}

impl World {
    /// A test's world with the unit AIs `ais` of the objects it holds ([`World::set_part`]).
    ///
    /// # Panics
    ///
    /// An object the world does not hold.
    #[must_use]
    pub(crate) fn with_ais(mut self, ais: impl IntoIterator<Item = (ObjectId, UnitAi)>) -> Self {
        for (id, ai) in ais {
            self.set_part(id, Part::UnitAi(Some(Box::new(ai))))
                .expect("a unit AI of an object the world holds");
        }
        self
    }

    /// A test's live list of player `p`: `objects` with their parts; the old list's objects
    /// leave the world with theirs.
    ///
    /// # Panics
    ///
    /// A player the world does not hold.
    pub(crate) fn set_live_for_test(
        &mut self,
        p: crate::players::PlayerId,
        objects: impl IntoIterator<Item = Object>,
    ) {
        let list = &mut self
            .players
            .get_mut(p.index())
            .expect("a player the world holds")
            .objects;
        let old: Vec<ObjectId> = list.iter().map(|u| u.id).collect();
        let mut units = Vec::new();
        for id in old {
            let _ = self.parts.take(id);
        }
        let mut parts = Vec::new();
        for o in objects {
            parts.push((o.unit.id, o.parts));
            units.push(o.unit);
        }
        *list = units.into();
        for (id, part) in parts {
            self.parts.file(id, part);
        }
        self.reindex();
    }

    /// A test's objects `objects` apart: their units (a player's list) and their parts
    /// ([`World::with_parts`]).
    pub(crate) fn split_layers(
        objects: impl IntoIterator<Item = Object>,
    ) -> (Vec<Unit>, Vec<(ObjectId, ObjectParts)>) {
        objects
            .into_iter()
            .map(|o| {
                let id = o.unit.id;
                (o.unit, (id, o.parts))
            })
            .unzip()
    }

    /// A test's world with the parts `parts` filed for the objects it holds.
    ///
    /// # Panics
    ///
    /// An object the world does not hold.
    #[must_use]
    pub(crate) fn with_parts(
        mut self,
        parts: impl IntoIterator<Item = (ObjectId, ObjectParts)>,
    ) -> Self {
        for (id, parts) in parts {
            self.load_parts(id, parts)
                .expect("the parts of an object the world holds");
        }
        self
    }

    /// A test's world with the combat layers `layers` set on the objects it holds.
    ///
    /// # Panics
    ///
    /// An object the world does not hold.
    #[must_use]
    pub(crate) fn with_combats(
        mut self,
        layers: impl IntoIterator<Item = (ObjectId, Combat)>,
    ) -> Self {
        for (id, layer) in layers {
            self.set_part(id, Part::Combat(Box::new(layer)))
                .expect("a combat layer of an object the world holds");
        }
        self
    }

    /// A test's object `id` takes the motion part `motion` in place of its own (none: a
    /// static object).
    ///
    /// # Panics
    ///
    /// An object the world does not hold.
    pub(crate) fn set_motion(&mut self, id: ObjectId, motion: Option<MotionLayer>) {
        self.set_part(id, Part::Motion(motion.map(Box::new)))
            .expect("a motion part of an object the world holds");
    }

    /// A test's world with the motion parts `layers` set on the objects it holds.
    ///
    /// # Panics
    ///
    /// An object the world does not hold.
    #[must_use]
    pub(crate) fn with_motions(
        mut self,
        layers: impl IntoIterator<Item = (ObjectId, MotionLayer)>,
    ) -> Self {
        for (id, layer) in layers {
            self.set_part(id, Part::Motion(Some(Box::new(layer))))
                .expect("a motion part of an object the world holds");
        }
        self
    }

    /// A test's world with the building layers `layers` set on the objects it holds.
    ///
    /// # Panics
    ///
    /// An object the world does not hold.
    #[must_use]
    pub(crate) fn with_buildings(
        mut self,
        layers: impl IntoIterator<Item = (ObjectId, Building)>,
    ) -> Self {
        for (id, layer) in layers {
            self.set_part(id, Part::Building(Box::new(layer)))
                .expect("a building layer of an object the world holds");
        }
        self
    }
}
