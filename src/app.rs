use std::path::PathBuf;

use crate::{
    event::{AppEvent, Direction, Event, EventHandler},
    svn,
};
use chrono::{DateTime, Utc};
use ratatui::{
    DefaultTerminal,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    widgets::{ListState, ScrollbarState},
};

const SVN_STATUS_TIMEOUT: i64 = 2;

#[derive(Debug)]
pub struct App {
    /// Is the app running, used to decide if we should quit
    pub running: bool,
    /// Event handler in a background thread
    pub events: EventHandler,
    /// The name of the current branch
    pub current_branch: String,
    /// The output from 'svn status'
    pub file_list: svn::FileList,
    /// The state of the displayed changes list
    pub list_state: ListState,
    /// The last time 'svn status' was run
    pub last_updated: DateTime<Utc>,
    /// The current working directory
    pub cwd: PathBuf,
    pub changes_scrollbar_state: ScrollbarState,
    pub conflicts_scrollbar_state: ScrollbarState,
    pub conflicts_scroll_offset: usize,
    pub selected_section: Option<AppSection>,
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .expect("does this directory exist? do you have permissions on this dir?");
        let current_branch = match svn::get_branch_name(&cwd) {
            Ok(branch) => branch,
            Err(e) => panic!("Issue in App creation: {e}"),
        };
        let mut file_list = svn::FileList::empty();
        if let Ok(status) = svn::get_svn_status(&cwd) {
            file_list
                .populate_from_svn_status(&status)
                .expect("failed to populate from svn status");
        }
        let list_state = ListState::default();
        let changes_scrollbar_state = ScrollbarState::default();
        let conflicts_scrollbar_state = ScrollbarState::default();
        Self {
            running: true,
            events: EventHandler::new(),
            current_branch,
            file_list,
            last_updated: Utc::now(),
            cwd,
            list_state,
            changes_scrollbar_state,
            conflicts_scrollbar_state,
            conflicts_scroll_offset: 0,
            selected_section: None,
        }
    }

    /// Run the application's main loop.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn update_svn_status(&mut self) {
        // TODO error popup here?
        if let Ok(status) = svn::get_svn_status(&self.cwd) {
            let _ = self.file_list.populate_from_svn_status(&status);
        }
        self.last_updated = Utc::now();
    }

    fn update_branch_name(&mut self) {
        self.current_branch = match svn::get_branch_name(&self.cwd) {
            Ok(branch) => branch,
            Err(e) => e.message,
        };
    }

    fn handle_events(&mut self) -> color_eyre::Result<()> {
        match self.events.next()? {
            Event::Tick => self.tick(),
            Event::Crossterm(event) => match event {
                crossterm::event::Event::Key(key_event) => self.handle_key_event(key_event)?,
                _ => {}
            },
            Event::App(app_event) => match app_event {
                AppEvent::Quit => self.quit(),
                AppEvent::UpdateRequest => {
                    self.update_branch_name();
                    self.update_svn_status();
                }
                AppEvent::ConflictsScroll(dir) => match dir {
                    Direction::Up => {
                        self.conflicts_scroll_offset =
                            self.conflicts_scroll_offset.saturating_sub(1);
                        self.conflicts_scrollbar_state = self
                            .conflicts_scrollbar_state
                            .position(self.conflicts_scroll_offset);
                    }
                    Direction::Down => {
                        self.conflicts_scroll_offset =
                            self.conflicts_scroll_offset.saturating_add(1);
                        self.conflicts_scrollbar_state = self
                            .conflicts_scrollbar_state
                            .position(self.conflicts_scroll_offset);
                    }
                },
                AppEvent::ChangesScroll(dir) => todo!(),
            },
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn handle_key_event(&mut self, key_event: KeyEvent) -> color_eyre::Result<()> {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.events.send(AppEvent::Quit)
            }
            // Other handlers you could add here.
            KeyCode::Char('r' | 'R') => self.events.send(AppEvent::UpdateRequest),
            KeyCode::Up => match self.selected_section {
                Some(AppSection::Conflicts) => {
                    self.events.send(AppEvent::ConflictsScroll(Direction::Up))
                }
                Some(AppSection::Changes) => {
                    self.events.send(AppEvent::ChangesScroll(Direction::Up))
                }
                None => {}
            },
            KeyCode::Down => match self.selected_section {
                Some(AppSection::Conflicts) => {
                    self.events.send(AppEvent::ConflictsScroll(Direction::Down))
                }
                Some(AppSection::Changes) => {
                    self.events.send(AppEvent::ChangesScroll(Direction::Down))
                }
                None => {}
            },
            _ => {}
        }
        Ok(())
    }

    /// Handles the tick event of the terminal.
    ///
    /// The tick event is where you can update the state of your application with any logic that
    /// needs to be updated at a fixed frame rate. E.g. polling a server, updating an animation.
    fn tick(&mut self) {
        if self.is_time_for_update() {
            self.events.send(AppEvent::UpdateRequest);
        }
    }

    fn is_time_for_update(&self) -> bool {
        Utc::now()
            .signed_duration_since(self.last_updated)
            .num_seconds()
            > SVN_STATUS_TIMEOUT
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppSection {
    Changes,
    Conflicts,
}
