use crossterm::{
    ExecutableCommand,
    event::{DisableMouseCapture, EnableMouseCapture},
};

use crate::app::App;

pub mod app;
pub mod error;
pub mod event;
pub mod svn;
pub mod ui;

fn main() -> color_eyre::Result<()> {
    std::io::stdout().execute(EnableMouseCapture).unwrap();
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().run(terminal);
    ratatui::restore();
    std::io::stdout().execute(DisableMouseCapture).unwrap();
    result
}
