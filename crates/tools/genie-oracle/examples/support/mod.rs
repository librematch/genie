//! The subscriber of the examples that run the engine: what genie-core logs while it runs
//! (`genie_core::trace`), as text lines on stderr, once `RUST_LOG` holds a filter.
//!
//! Without a filter no subscriber is installed, and the examples print what they printed before.
//! The lines carry no time and no colours: two runs of one recording give the same log, so two
//! logs can be compared line by line, and a log attached to a bug report reads as plain text.

use std::io::stderr;
use tracing::Subscriber;
use tracing::subscriber::set_global_default;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

/// The filter of `RUST_LOG`'s value, or none when it is unset or blank.
///
/// # Errors
///
/// When the value does not parse as a filter: the message names `RUST_LOG` and the value.
pub fn filter(rust_log: Option<&str>) -> Result<Option<EnvFilter>, String> {
    let Some(value) = rust_log.filter(|text| !text.trim().is_empty()) else {
        return Ok(None);
    };
    EnvFilter::try_new(value)
        .map(Some)
        .map_err(|parse_error| format!("RUST_LOG `{value}`: {parse_error}"))
}

/// The subscriber that writes the events and spans `filter` lets through to `writer`.
#[must_use]
pub fn subscriber<W>(filter: EnvFilter, writer: W) -> impl Subscriber + Send + Sync
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish()
}

/// Install the subscriber on stderr as the global one, if `rust_log` (`RUST_LOG`'s value) holds
/// a filter.
///
/// # Errors
///
/// When the filter does not parse, or a global subscriber is installed already.
pub fn install(rust_log: Option<&str>) -> Result<(), String> {
    let Some(filter) = filter(rust_log)? else {
        return Ok(());
    };
    set_global_default(subscriber(filter, stderr)).map_err(|error| error.to_string())
}
