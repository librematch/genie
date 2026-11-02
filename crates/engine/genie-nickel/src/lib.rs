//! Nickel into the genie engine's model (FD-046): a source evaluated ([`evaluate`]), each number
//! read as the exact rational Nickel holds and rounded once to its field's float type
//! ([`round`]), and a serde deserializer over the evaluated value ([`de::from_expr`]); the
//! model's contracts, written from its types' JSON Schema ([`contracts`]).

pub mod contracts;
pub mod de;
pub mod round;

use genie_error::GenieError;
use nickel_lang::Context;
use nickel_lang::ErrorFormat;
use nickel_lang::Expr;

pub use self::de::from_expr;

/// The value of the Nickel source `source`, evaluated deeply, `name` naming it in an error.
///
/// # Errors
///
/// A [`GenieError`] with Nickel's message when the source does not evaluate.
pub fn evaluate(source: &str, name: &str) -> Result<Expr, GenieError> {
    let mut context = Context::new().with_source_name(name.to_owned());
    context.eval_deep(source).map_err(|error| {
        let mut text = Vec::new();
        let message = match error.format(&mut text, ErrorFormat::Text) {
            Ok(()) => String::from_utf8_lossy(&text).into_owned(),
            Err(_) => format!("{error:?}"),
        };
        GenieError::invalid_input("a Nickel source that does not evaluate")
            .with("source", name)
            .with("message", message.trim_end())
    })
}
