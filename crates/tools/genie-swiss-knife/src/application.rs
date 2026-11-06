//! `GenieSwissKnife` Abscissa Application

use crate::subscriber::Verbosity;
use crate::subscriber::install;
use crate::{commands::EntryPoint, config::GenieSwissKnifeConfig};
use abscissa_core::{
    Application, Component, FrameworkError, FrameworkErrorKind, StandardPaths,
    application::{self, AppCell},
    config::{self, CfgCell},
    terminal::component::Terminal,
};

/// Application state
pub static APP: AppCell<GenieSwissKnifeApp> = AppCell::new();

/// `GenieSwissKnife` Application
#[derive(Debug)]
pub struct GenieSwissKnifeApp {
    /// Application configuration.
    config: CfgCell<GenieSwissKnifeConfig>,

    /// Application state.
    state: application::State<Self>,
}

/// Initialize a new application instance.
///
/// By default no configuration is loaded, and the framework state is
/// initialized to a default, empty state (no components, threads, etc).
impl Default for GenieSwissKnifeApp {
    fn default() -> Self {
        Self {
            config: CfgCell::default(),
            state: application::State::default(),
        }
    }
}

impl Application for GenieSwissKnifeApp {
    /// Entrypoint command for this application.
    type Cmd = EntryPoint;

    /// Application configuration.
    type Cfg = GenieSwissKnifeConfig;

    /// Paths to resources within the application.
    type Paths = StandardPaths;

    /// Accessor for application configuration.
    fn config(&self) -> config::Reader<GenieSwissKnifeConfig> {
        self.config.read()
    }

    /// Borrow the application state immutably.
    fn state(&self) -> &application::State<Self> {
        &self.state
    }

    /// The terminal; the tracing subscriber is gsk's own: bunyan JSON lines on stderr, and only
    /// with `RUST_LOG` set or `--verbose` ([`crate::subscriber`]).
    fn framework_components(
        &mut self,
        command: &Self::Cmd,
    ) -> Result<Vec<Box<dyn Component<Self>>>, FrameworkError> {
        let verbosity = if command.verbose {
            Verbosity::Verbose
        } else {
            Verbosity::Quiet
        };
        install(verbosity).map_err(|error| FrameworkErrorKind::ComponentError.context(error))?;
        Ok(vec![Box::new(Terminal::new(self.term_colors(command)))])
    }

    /// Register all components used by this application.
    ///
    /// If you would like to add additional components to your application
    /// beyond the default ones provided by the framework, this is the place
    /// to do so.
    fn register_components(&mut self, command: &Self::Cmd) -> Result<(), FrameworkError> {
        let framework_components = self.framework_components(command)?;
        let mut app_components = self.state.components_mut();
        app_components.register(framework_components)
    }

    /// Post-configuration lifecycle callback.
    ///
    /// Called regardless of whether config is loaded to indicate this is the
    /// time in app lifecycle when configuration would be loaded if
    /// possible.
    fn after_config(&mut self, config: Self::Cfg) -> Result<(), FrameworkError> {
        // Configure components
        self.state.components_mut().after_config(&config)?;
        self.config.set_once(config);
        Ok(())
    }
}
