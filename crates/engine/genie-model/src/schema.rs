//! The JSON Schema of the model's bit sets (FD-046-D15, D40): bitflags' text form, the set's
//! flag names joined by ` | ` with any unnamed bits as one hexadecimal number, which its serde
//! support reads and writes in a readable format.

use crate::combat::missile::SmartMode;
use crate::combat::missile::VanishMode;
use crate::layer::generated::AttributeRoles;
use crate::layer::generated::ClassRoles;
use crate::layer::generated::MasterRoles;
use crate::layer::generated::TechRoles;
use crate::movement::MoveAlgorithm;
use crate::objects::SpriteFlags;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::TaskFlags;
use bitflags::Flags;
use schemars::JsonSchema;
use schemars::Schema;
use schemars::SchemaGenerator;
use schemars::json_schema;
use std::borrow::Cow;

/// What a bit set's schema says of its form.
const BIT_SET: &str =
    "The set's flag names joined by ` | `, unnamed bits as one hexadecimal number (`0x10`).";

/// Each bit set's schema: a string of bitflags' text form.
macro_rules! bit_set_schemas {
    ($($set:ident),* $(,)?) => {
        $(
            impl JsonSchema for $set {
                fn schema_name() -> Cow<'static, str> {
                    Cow::Borrowed(stringify!($set))
                }

                fn json_schema(_: &mut SchemaGenerator) -> Schema {
                    json_schema!({ "type": "string", "description": BIT_SET })
                }
            }
        )*
    };
}

bit_set_schemas!(
    BlastLevel,
    CombatAbility,
    GarrisonKinds,
    HeroFlags,
    MoveAlgorithm,
    ObjectFlags,
    SmartMode,
    SpriteFlags,
    TaskFlags,
    VanishMode,
);

/// Each role set's schema: one role's name, which a role table lists per id (FD-046-D08).
macro_rules! role_set_schemas {
    ($($set:ident),* $(,)?) => {
        $(
            impl JsonSchema for $set {
                fn schema_name() -> Cow<'static, str> {
                    Cow::Borrowed(stringify!($set))
                }

                fn json_schema(_: &mut SchemaGenerator) -> Schema {
                    let names: Vec<&str> = <$set as Flags>::FLAGS.iter().map(|f| f.name()).collect();
                    json_schema!({ "type": "string", "enum": names })
                }
            }
        )*
    };
}

role_set_schemas!(AttributeRoles, ClassRoles, MasterRoles, TechRoles);
