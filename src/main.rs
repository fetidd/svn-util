pub mod app;
pub mod config;
pub mod error;
pub mod event;
pub mod svn;
pub mod ui;

use config::Config;
use crossterm::{
    ExecutableCommand,
    event::{DisableMouseCapture, EnableMouseCapture},
};

use crate::app::App;

fn main() -> color_eyre::Result<()> {
    let mut config = Config::default();
    config.update_from_file().unwrap();
    config.update_from_env_args();
    std::io::stdout().execute(EnableMouseCapture).unwrap();
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().with_config(config).run(terminal);
    ratatui::restore();
    std::io::stdout().execute(DisableMouseCapture).unwrap();
    result
}
