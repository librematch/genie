//! The subscriber of the examples that run the engine (`examples/support/mod.rs`): what a bug
//! report gets from `RUST_LOG`.

#[path = "../examples/support/mod.rs"]
mod support;

use std::io::Write;
use std::sync::{Arc, Mutex};

use genie_core::trace::{STATE, UNSUPPORTED};
use std::io::Error;
use tracing::subscriber::with_default;
use tracing_subscriber::fmt::MakeWriter;

/// The lines a subscriber writes, kept for the test to read.
#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<u8>>>);

impl Lines {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Lines {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

impl<'writer> MakeWriter<'writer> for Lines {
    type Writer = Self;

    fn make_writer(&'writer self) -> Self {
        self.clone()
    }
}

#[test]
fn without_rust_log_no_subscriber_is_installed() {
    assert!(support::filter(None).unwrap().is_none());
    assert!(support::filter(Some(" ")).unwrap().is_none());
    support::install(None).unwrap();
}

#[test]
fn rust_log_that_does_not_parse_is_an_error_that_names_it() {
    let text = support::filter(Some("genie_core=loud")).unwrap_err();
    assert!(text.contains("RUST_LOG"), "{text}");
    assert!(text.contains("genie_core=loud"), "{text}");
}

#[test]
fn an_unsupported_case_prints_as_one_plain_line_without_time() {
    let filter = support::filter(Some("genie_core::unsupported=warn"))
        .unwrap()
        .unwrap();
    let lines = Lines::default();
    let subscriber = support::subscriber(filter, lines.clone());
    with_default(subscriber, || {
        tracing::warn!(
            target: UNSUPPORTED,
            tick = 1154,
            player = 2,
            why = "a command the slice does not model: TribeGiveAttributes",
            "a player stops"
        );
        tracing::debug!(
            target: STATE,
            tick = 1154,
            player = 2,
            object = 9175,
            from = 2,
            to = 3,
            "object_state"
        );
    });
    assert_eq!(
        lines.text(),
        " WARN genie_core::unsupported: a player stops tick=1154 player=2 \
         why=\"a command the slice does not model: TribeGiveAttributes\"\n"
    );
}
