//! A serde deserializer over an evaluated Nickel value (FD-046-D16).
//!
//! An f32 field takes the rational Nickel holds, rounded once ([`round_to_f32`]); an integer
//! field the integer, checked against its type; an error names the field's path.

use crate::round::round_to_f32;
use crate::round::round_to_f64;
use genie_error::{GenieError, GenieResult};
use nickel_lang::Expr;
use serde::Deserializer;
use serde::de::DeserializeOwned;
use serde::de::DeserializeSeed;
use serde::de::EnumAccess;
use serde::de::Error as SerdeError;
use serde::de::IntoDeserializer;
use serde::de::MapAccess;
use serde::de::SeqAccess;
use serde::de::VariantAccess;
use serde::de::Visitor;
use serde::de::value::StrDeserializer;
use serde::forward_to_deserialize_any;
use std::fmt::Display;

/// `T` read from the evaluated value `expr`.
///
/// # Errors
///
/// A [`GenieError`] naming the field's path when the value does not fit `T`'s shape or a number
/// does not fit its field.
pub fn from_expr<T: DeserializeOwned>(expr: &Expr) -> GenieResult<T> {
    T::deserialize(ExprDeserializer {
        expr: expr.clone(),
        path: Vec::new(),
    })
    .map_err(|error| {
        if has_path(&error) {
            error
        } else {
            error.with("path", "(root)")
        }
    })
}

/// Whether `error` names its field's path.
fn has_path(error: &GenieError) -> bool {
    error.context().any(|(key, _)| key == "path")
}

/// A value and its path from the root.
struct ExprDeserializer {
    expr: Expr,
    path: Vec<String>,
}

impl ExprDeserializer {
    /// The error `why` at this value's path.
    fn error(&self, why: impl Display) -> GenieError {
        self.at(GenieError::custom(why))
    }

    /// An error from below that names no path yet takes this one; the root names none.
    fn at(&self, error: GenieError) -> GenieError {
        if self.path.is_empty() || has_path(&error) {
            error
        } else {
            error.with("path", self.path.join("."))
        }
    }

    /// The number's rational, the numerator signed (nickel-lang's `as_rational` drops the sign).
    fn rational(&self) -> GenieResult<(String, String)> {
        let number = self
            .expr
            .as_number()
            .ok_or_else(|| self.error("not a number"))?;
        let (numerator, denominator) = number.as_rational();
        let sign = if number.as_f64().is_sign_negative() {
            "-"
        } else {
            ""
        };
        Ok((format!("{sign}{numerator}"), denominator))
    }

    /// The number as an integer, if it is one (beyond i64 too).
    fn integer(&self) -> GenieResult<i128> {
        let (numerator, denominator) = self.rational()?;
        if denominator != "1" {
            return Err(self.error(format!("{numerator}/{denominator} is no integer")));
        }
        numerator
            .parse::<i128>()
            .map_err(|_| self.error(format!("{numerator} is out of range")))
    }

    /// A number of a field of any type: an integer as i64, any other through its f64 rounding.
    fn visit_number<'de, V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        if let Some(integer) = self.expr.as_i64() {
            return visitor.visit_i64(integer).map_err(|x| self.at(x));
        }
        let (numerator, denominator) = self.rational()?;
        let rounded = round_to_f64(&numerator, &denominator).map_err(|x| self.error(x))?;
        visitor.visit_f64(rounded).map_err(|x| self.at(x))
    }

    /// A bool, a string or an enum tag (as its name) of a field of any type.
    fn visit_scalar<'de, V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        if let Some(boolean) = self.expr.as_bool() {
            return visitor.visit_bool(boolean).map_err(|x| self.at(x));
        }
        if let Some(text) = self.expr.as_str() {
            return visitor
                .visit_string(text.to_owned())
                .map_err(|x| self.at(x));
        }
        if let Some(tag) = self.expr.as_enum_tag() {
            return visitor.visit_string(tag.to_owned()).map_err(|x| self.at(x));
        }
        Err(self.error("a value of no serde shape (an enum variant needs an enum field)"))
    }

    /// The child at `key` of this value.
    fn child(&self, expr: Expr, key: String) -> Self {
        let mut path = self.path.clone();
        path.push(key);
        Self { expr, path }
    }
}

/// An integer field: the number checked against the type.
macro_rules! integer {
    ($method:ident, $visit:ident, $ty:ty) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
            let value = self.integer()?;
            let fits = <$ty>::try_from(value)
                .map_err(|_| self.error(format!("{value} does not fit {}", stringify!($ty))))?;
            visitor.$visit(fits).map_err(|error| self.at(error))
        }
    };
}

impl<'de> Deserializer<'de> for ExprDeserializer {
    type Error = GenieError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        if self.expr.is_null() {
            visitor.visit_none().map_err(|x| self.at(x))
        } else if self.expr.is_record() {
            self.deserialize_map(visitor)
        } else if self.expr.is_array() {
            self.deserialize_seq(visitor)
        } else if self.expr.is_number() {
            self.visit_number(visitor)
        } else {
            self.visit_scalar(visitor)
        }
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        let (numerator, denominator) = self.rational()?;
        let rounded = round_to_f32(&numerator, &denominator).map_err(|x| self.error(x))?;
        visitor.visit_f32(rounded).map_err(|x| self.at(x))
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        let (numerator, denominator) = self.rational()?;
        let rounded = round_to_f64(&numerator, &denominator).map_err(|x| self.error(x))?;
        visitor.visit_f64(rounded).map_err(|x| self.at(x))
    }

    integer!(deserialize_i8, visit_i8, i8);
    integer!(deserialize_i16, visit_i16, i16);
    integer!(deserialize_i32, visit_i32, i32);
    integer!(deserialize_i64, visit_i64, i64);
    integer!(deserialize_u8, visit_u8, u8);
    integer!(deserialize_u16, visit_u16, u16);
    integer!(deserialize_u32, visit_u32, u32);
    integer!(deserialize_u64, visit_u64, u64);

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        if self.expr.is_null() {
            visitor.visit_none().map_err(|x| self.at(x))
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> GenieResult<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        let items: Vec<Expr> = match self.expr.as_array() {
            Some(array) => array.iter().collect(),
            None => return Err(self.error("not an array")),
        };
        let access = Items {
            items: items.into_iter().enumerate(),
            parent: &self,
        };
        visitor.visit_seq(access).map_err(|x| self.at(x))
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> GenieResult<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> GenieResult<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        let fields: Vec<(String, Expr)> = match self.expr.as_record() {
            Some(record) => record
                .iter()
                .filter_map(|(text, value)| value.map(|expr| (text.to_owned(), expr)))
                .collect(),
            None => return Err(self.error("not a record")),
        };
        let access = Record {
            fields: fields.into_iter(),
            value: None,
            parent: &self,
        };
        visitor.visit_map(access).map_err(|x| self.at(x))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> GenieResult<V::Value> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> GenieResult<V::Value> {
        if let Some(tag) = self.expr.as_enum_tag() {
            let tag = tag.to_owned();
            return visitor.visit_enum(Variant {
                tag,
                payload: None,
                parent: &self,
            });
        }
        if let Some((tag, payload)) = self.expr.as_enum_variant() {
            let tag = tag.to_owned();
            return visitor.visit_enum(Variant {
                tag,
                payload: Some(payload),
                parent: &self,
            });
        }
        // a string names a unit variant too (JSON's form)
        if let Some(tag) = self.expr.as_str() {
            let tag = tag.to_owned();
            return visitor.visit_enum(Variant {
                tag,
                payload: None,
                parent: &self,
            });
        }
        Err(self.error("not an enum tag"))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> GenieResult<V::Value> {
        visitor.visit_unit()
    }

    forward_to_deserialize_any! {
        bool i128 u128 char str string bytes byte_buf unit unit_struct identifier
    }
}

/// An array's items, each with its index on the path.
struct Items<'parent, I> {
    items: I,
    parent: &'parent ExprDeserializer,
}

impl<'de, I: Iterator<Item = (usize, Expr)>> SeqAccess<'de> for Items<'_, I> {
    type Error = GenieError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> GenieResult<Option<T::Value>> {
        match self.items.next() {
            Some((index, expr)) => seed
                .deserialize(self.parent.child(expr, index.to_string()))
                .map(Some),
            None => Ok(None),
        }
    }
}

/// A record's fields, each with its name on the path.
struct Record<'parent, I> {
    fields: I,
    value: Option<(String, Expr)>,
    parent: &'parent ExprDeserializer,
}

impl<'de, I: Iterator<Item = (String, Expr)>> MapAccess<'de> for Record<'_, I> {
    type Error = GenieError;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> GenieResult<Option<K::Value>> {
        match self.fields.next() {
            Some((key, expr)) => {
                let key_deserializer: StrDeserializer<'_, GenieError> =
                    key.as_str().into_deserializer();
                let out = seed.deserialize(key_deserializer)?;
                self.value = Some((key, expr));
                Ok(Some(out))
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> GenieResult<T::Value> {
        let (key, expr) = self
            .value
            .take()
            .ok_or_else(|| self.parent.error("a value without its key"))?;
        seed.deserialize(self.parent.child(expr, key))
    }
}

/// An enum's variant: its tag and the payload a newtype variant holds.
struct Variant<'parent> {
    tag: String,
    payload: Option<Expr>,
    parent: &'parent ExprDeserializer,
}

impl<'de> EnumAccess<'de> for Variant<'_> {
    type Error = GenieError;
    type Variant = Self;

    fn variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> GenieResult<(T::Value, Self)> {
        let key: StrDeserializer<'_, GenieError> = self.tag.as_str().into_deserializer();
        let value = seed
            .deserialize(key)
            .map_err(|error| self.parent.at(error))?;
        Ok((value, self))
    }
}

impl<'de> VariantAccess<'de> for Variant<'_> {
    type Error = GenieError;

    fn unit_variant(self) -> GenieResult<()> {
        match self.payload {
            None => Ok(()),
            Some(_) => Err(self.parent.error(format!("'{} holds a value", self.tag))),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> GenieResult<T::Value> {
        match self.payload {
            Some(expr) => seed.deserialize(self.parent.child(expr, self.tag.clone())),
            None => Err(self.parent.error(format!("'{} holds no value", self.tag))),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> GenieResult<V::Value> {
        match self.payload {
            Some(expr) => self
                .parent
                .child(expr, self.tag.clone())
                .deserialize_seq(visitor),
            None => Err(self.parent.error(format!("'{} holds no array", self.tag))),
        }
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> GenieResult<V::Value> {
        match self.payload {
            Some(expr) => self
                .parent
                .child(expr, self.tag.clone())
                .deserialize_map(visitor),
            None => Err(self.parent.error(format!("'{} holds no record", self.tag))),
        }
    }
}

#[cfg(test)]
mod tests;
