//! The build layer (FD-046-D08, D32): what a build's executable hardcodes about the game's ids,
//! their roles and the slots, and its tuning values.
//!
//! genie loads it from the build's Nickel source `builds/<build>.ncl` (genie-nickel), and genie's
//! tests build the same values from genie-core's fixture. The role sets, the slots and the
//! tuning groups are generated from the build's symbol table (`generated.rs`,
//! `harness image layer`); a model takes a layer with [`Model::with_layer`].
//!
//! [`Model::with_layer`]: crate::model::Model::with_layer

pub mod generated;

use crate::layer::generated::AttributeRoles;
use crate::layer::generated::ClassRoles;
use crate::layer::generated::MasterRoles;
use crate::layer::generated::Slots;
use crate::layer::generated::TechRoles;
use crate::layer::generated::Tuning;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::players::attribute::AttributeId;
use crate::technology::TechId;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

/// Each id space's roles by id (FD-046-D04): a test is one load and one AND. An id beyond a
/// table holds no role.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Roles {
    /// By class.
    #[serde(with = "by_id")]
    #[cfg_attr(feature = "schema", schemars(with = "by_id::Schema<ClassRoles>"))]
    pub classes: Arc<Vec<ClassRoles>>,
    /// By master.
    #[serde(with = "by_id")]
    #[cfg_attr(feature = "schema", schemars(with = "by_id::Schema<MasterRoles>"))]
    pub masters: Arc<Vec<MasterRoles>>,
    /// By technology.
    #[serde(with = "by_id")]
    #[cfg_attr(feature = "schema", schemars(with = "by_id::Schema<TechRoles>"))]
    pub techs: Arc<Vec<TechRoles>>,
    /// By player attribute.
    #[serde(with = "by_id")]
    #[cfg_attr(feature = "schema", schemars(with = "by_id::Schema<AttributeRoles>"))]
    pub attributes: Arc<Vec<AttributeRoles>>,
}

impl Roles {
    /// The roles of the ids each list names, the ids of a space in any order (a later entry of
    /// one id adds to it).
    #[must_use]
    pub fn from_ids(
        classes: &[(usize, ClassRoles)],
        masters: &[(usize, MasterRoles)],
        techs: &[(usize, TechRoles)],
        attributes: &[(usize, AttributeRoles)],
    ) -> Self {
        Self {
            classes: Arc::new(by_id::table(classes)),
            masters: Arc::new(by_id::table(masters)),
            techs: Arc::new(by_id::table(techs)),
            attributes: Arc::new(by_id::table(attributes)),
        }
    }

    /// The roles of a class; none for no class or one beyond the table.
    #[must_use]
    pub fn of_class(&self, class: ClassId) -> ClassRoles {
        class
            .index()
            .and_then(|i| self.classes.get(i))
            .copied()
            .unwrap_or_default()
    }

    /// The roles of a master; none for one beyond the table.
    #[must_use]
    pub fn of_master(&self, master: MasterId) -> MasterRoles {
        self.masters
            .get(master.index())
            .copied()
            .unwrap_or_default()
    }

    /// The roles of a technology; none for one beyond the table.
    #[must_use]
    pub fn of_tech(&self, tech: TechId) -> TechRoles {
        self.techs.get(tech.index()).copied().unwrap_or_default()
    }

    /// The roles of a player attribute; none for one beyond the table.
    #[must_use]
    pub fn of_attribute(&self, attribute: AttributeId) -> AttributeRoles {
        self.attributes
            .get(attribute.index())
            .copied()
            .unwrap_or_default()
    }
}

/// The build layer (FD-046-D08, D32): what the exe hardcodes about ids, and its tuning values.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BuildLayer {
    /// The build (`de.build`, 158041).
    pub build: u32,
    /// The roles by id.
    pub roles: Roles,
    /// The slots.
    pub slots: Slots,
    /// The tuning values per port module.
    pub tuning: Tuning,
}

mod by_id;

#[cfg(test)]
mod tests;
