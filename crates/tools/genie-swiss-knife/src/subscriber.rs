//! The subscriber for what genie's engine tells while it runs, for debugging.
//!
//! genie-core's `trace` module emits the update, player and object spans, the unmodelled cases
//! and the state changes; this subscriber prints them as bunyan JSON lines on stderr. It listens
//! only when `RUST_LOG` holds a filter (`RUST_LOG=genie_core::unsupported=warn`,
//! `RUST_LOG=genie_core=trace` …) or the command runs with `--verbose` (`debug`). Else none is
//! installed, and the engine's spans and events cost one comparison each.

use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt};

/// The filter the subscriber takes: `RUST_LOG`'s, else `debug` for `verbose`.
///
/// None means no subscriber. gsk's own spans come first at `info`, so an engine event says which
/// run it is from; a directive of `RUST_LOG` for `genie_swiss_knife` overrides it.
#[must_use]
pub fn filter(rust_log: Option<&str>, verbose: bool) -> Option<String> {
    let filter = match rust_log {
        Some(f) if !f.trim().is_empty() => f.to_owned(),
        _ if verbose => "debug".to_owned(),
        _ => return None,
    };
    Some(format!("genie_swiss_knife=info,{filter}"))
}

/// Install the bunyan subscriber as the global one, if [`filter`] gives a filter.
///
/// # Errors
///
/// When the filter does not parse or a global subscriber is installed already.
pub fn install(verbose: bool) -> Result<(), String> {
    let rust_log = std::env::var("RUST_LOG").ok();
    let Some(filter) = filter(rust_log.as_deref(), verbose) else {
        return Ok(());
    };
    let filter = EnvFilter::try_new(&filter).map_err(|e| format!("RUST_LOG `{filter}`: {e}"))?;
    let subscriber = Registry::default()
        .with(filter)
        .with(JsonStorageLayer)
        .with(BunyanFormattingLayer::new(
            "gsk".to_owned(),
            std::io::stderr,
        ));
    tracing::subscriber::set_global_default(subscriber).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_filter_or_verbose_installs_subscriber() {
        assert_eq!(filter(None, false), None);
        assert_eq!(filter(Some(" "), false), None);
        assert_eq!(
            filter(None, true).as_deref(),
            Some("genie_swiss_knife=info,debug")
        );
        assert_eq!(
            filter(Some("genie_core::unsupported=warn"), true).as_deref(),
            Some("genie_swiss_knife=info,genie_core::unsupported=warn")
        );
    }
}
