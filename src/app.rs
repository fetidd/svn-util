use std::path::PathBuf;

use crate::{
    event::{AppEvent, Direction, Event, EventHandler},
    svn,
};
use chrono::{DateTime, Utc};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    DefaultTerminal,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::Rect,
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
    // UI areas mainly used for mouse clicks etc.
    pub changes_area: Option<Rect>,
    pub conflicts_area: Option<Rect>,
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
            changes_area: None,
            conflicts_area: None,
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

    fn handle_events(&mut self) -> color_eyre::Result<()> {
        match self.events.next()? {
            Event::Tick => self.tick(),
            Event::Crossterm(event) => match event {
                crossterm::event::Event::Key(key_event) => self.handle_key_event(key_event)?,
                crossterm::event::Event::Mouse(mouse_event) => {
                    self.handle_mouse_event(mouse_event)?
                }
                _ => {}
            },
            Event::App(app_event) => match app_event {
                AppEvent::Quit => self.quit(),
                AppEvent::UpdateRequest => {
                    self.update_branch_name();
                    self.update_svn_status();
                }
                AppEvent::ConflictsScroll(dir) => handle_scroll(
                    &dir,
                    &mut self.conflicts_scroll_offset,
                    &mut self.conflicts_scrollbar_state,
                ),
                AppEvent::ChangesScroll(dir) => handle_scroll(
                    &dir,
                    self.list_state.offset_mut(),
                    &mut self.changes_scrollbar_state,
                ),
                AppEvent::ToggleSelectedSection => match self.selected_section {
                    Some(AppSection::Changes) => {
                        self.selected_section = Some(AppSection::Conflicts)
                    }
                    Some(AppSection::Conflicts) | None => {
                        self.selected_section = Some(AppSection::Changes)
                    }
                },
                AppEvent::DeselectSection => self.selected_section = None,
                AppEvent::Click { button, col, row } => self.handle_click(button, col, row),
                AppEvent::Scroll { dir, col, row } => self.handle_mouse_scroll(dir, col, row),
            },
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn handle_key_event(&mut self, key_event: KeyEvent) -> color_eyre::Result<()> {
        match key_event.code {
            KeyCode::Esc if self.selected_section.is_some() => {
                self.events.send(AppEvent::DeselectSection)
            }
            KeyCode::Esc | KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.events.send(AppEvent::Quit)
            }
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
            KeyCode::Tab => self.events.send(AppEvent::ToggleSelectedSection),
            _ => {}
        }
        Ok(())
    }

    fn handle_mouse_event(&mut self, mouse_event: MouseEvent) -> color_eyre::Result<()> {
        match mouse_event.kind {
            MouseEventKind::Down(btn) => self.events.send(AppEvent::Click {
                button: btn,
                col: mouse_event.column,
                row: mouse_event.row,
            }),
            MouseEventKind::ScrollDown => self.events.send(AppEvent::Scroll {
                dir: Direction::Down,
                col: mouse_event.column,
                row: mouse_event.row,
            }),
            MouseEventKind::ScrollUp => self.events.send(AppEvent::Scroll {
                dir: Direction::Up,
                col: mouse_event.column,
                row: mouse_event.row,
            }),
            _ => {}
        }
        Ok(())
    }

    /// Handles the tick event of the terminal.
    ///
    /// The tick event is where you can update the state of your application with any logic that
    /// needs to be updated at a fixed frame rate. E.g. polling a server, updating an animation.
    fn tick(&mut self) {
        if is_time_for_update(self.last_updated) {
            self.events.send(AppEvent::UpdateRequest);
        }
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
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

    fn handle_click(&mut self, button: MouseButton, col: u16, row: u16) {
        match button {
            MouseButton::Left => {
                self.selected_section = self.locate_mouse((row, col));
            }
            _ => {}
        }
    }

    fn handle_mouse_scroll(&mut self, dir: Direction, col: u16, row: u16) {
        match self.locate_mouse((row, col)) {
            Some(AppSection::Changes) => handle_scroll(
                &dir,
                self.list_state.offset_mut(),
                &mut self.changes_scrollbar_state,
            ),
            Some(AppSection::Conflicts) => handle_scroll(
                &dir,
                &mut self.conflicts_scroll_offset,
                &mut self.conflicts_scrollbar_state,
            ),
            None => {}
        }
    }

    fn locate_mouse(&self, (row, col): (u16, u16)) -> Option<AppSection> {
        for (area, app_section) in [
            (self.changes_area, AppSection::Changes),
            (self.conflicts_area, AppSection::Conflicts),
        ] {
            if let Some(area) = area {
                if area.contains((col, row).into()) {
                    return Some(app_section);
                }
            }
        }
        None
    }
}

fn handle_scroll(dir: &Direction, offset: &mut usize, bar_state: &mut ScrollbarState) {
    let operation = match dir {
        Direction::Up => usize::saturating_sub,
        Direction::Down => usize::saturating_add,
    };
    *offset = operation(*offset, 1);
    *bar_state = bar_state.position(*offset);
}

fn is_time_for_update(last_updated: DateTime<Utc>) -> bool {
    Utc::now().signed_duration_since(last_updated).num_seconds() > SVN_STATUS_TIMEOUT
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppSection {
    Changes,
    Conflicts,
}
