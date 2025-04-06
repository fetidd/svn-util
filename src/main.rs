pub mod app;
pub mod command;
pub mod config;
pub mod error;
pub mod event;
pub mod svn;

use config::Config;
use crossterm::{
    ExecutableCommand,
    event::{DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture},
};

use crate::app::App;

fn main() -> color_eyre::Result<()> {
    let mut config = Config::default();
    config.update_from_file().unwrap();
    config.update_from_env_args();
    std::io::stdout().execute(EnableMouseCapture).unwrap();
    std::io::stdout().execute(EnableFocusChange).unwrap();
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().with_config(config).run(terminal);
    ratatui::restore();
    std::io::stdout().execute(DisableMouseCapture).unwrap();
    std::io::stdout().execute(DisableFocusChange).unwrap();
    result
}
