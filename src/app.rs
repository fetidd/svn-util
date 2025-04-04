mod ui;

use std::path::PathBuf;

use crate::{
    config::Config,
    event::{AppEvent, Direction, Event, EventHandler},
    svn::{self, ParsedStatusLine},
};
use chrono::{DateTime, Utc};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    DefaultTerminal,
    crossterm::event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers},
    layout::{Position, Rect},
    widgets::{ListState, ScrollbarState},
};

#[derive(Debug)]
pub struct App {
    /// Is the app running, used to decide if we should quit
    running: bool,
    /// Event handler in a background thread
    events: EventHandler,
    /// The name of the current branch
    current_branch: String,
    /// The output from 'svn status'
    file_list: svn::FileList,
    /// The state of the displayed changes list
    list_state: ListState,
    /// The last time 'svn status' was run
    last_updated: DateTime<Utc>,
    /// The current working directory
    cwd: PathBuf,
    changes_scrollbar_state: ScrollbarState,
    conflicts_scrollbar_state: ScrollbarState,
    conflicts_scroll_offset: usize,
    selected_section: Option<AppSection>,
    // UI areas mainly used for mouse clicks etc.
    changes_area: Option<Rect>,
    conflicts_area: Option<Rect>,
    change_popup_area: Option<Rect>,
    config: Config,
    mouse_loc: (u16, u16), // row, col
    state: AppState,

    last_message: String,
}

#[derive(Debug, PartialEq)]
pub enum AppState {
    Main,        // The main screen
    ChangePopup, // A popup caused by a change is shown over the main screen
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
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
        let list_state = ListState::default().with_selected(Some(0));
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
            config: Config::default(),
            mouse_loc: (0, 0),
            state: AppState::Main,
            change_popup_area: None,
            last_message: String::new(),
        }
    }

    pub fn with_config(self, config: Config) -> Self {
        Self { config, ..self }
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
                CtEvent::Key(key_event) => self.handle_key_event(key_event)?,
                CtEvent::Mouse(mouse_event) => self.handle_mouse_event(mouse_event)?,
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
                    _ => {}
                },
                AppEvent::DeselectSection => self.selected_section = None,
                AppEvent::NextChange => self.list_state.select_next(),
                AppEvent::PrevChange => self.list_state.select_previous(),
                AppEvent::SelectChange => self.state = AppState::ChangePopup,
                AppEvent::Message(msg) => self.last_message = msg,
            },
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    fn handle_key_event(&mut self, key_event: KeyEvent) -> color_eyre::Result<()> {
        match key_event.code {
            KeyCode::Esc if self.state != AppState::Main => self.state = AppState::Main,
            KeyCode::Esc | KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                self.events.send(AppEvent::Quit)
            }
            KeyCode::Char('r' | 'R') => self.events.send(AppEvent::UpdateRequest),
            KeyCode::Up => match self.selected_section {
                Some(AppSection::Conflicts) => {
                    self.events.send(AppEvent::ConflictsScroll(Direction::Up))
                }
                Some(AppSection::Changes) => self.events.send(AppEvent::PrevChange),
                _ => {}
            },
            KeyCode::Down => match self.selected_section {
                Some(AppSection::Conflicts) => {
                    self.events.send(AppEvent::ConflictsScroll(Direction::Down))
                }
                Some(AppSection::Changes) => self.events.send(AppEvent::NextChange),
                _ => {}
            },
            KeyCode::Char('c') => self.events.send(AppEvent::ToggleSelectedSection),
            _ => {}
        }
        Ok(())
    }

    fn handle_mouse_event(&mut self, mouse_event: MouseEvent) -> color_eyre::Result<()> {
        match mouse_event.kind {
            MouseEventKind::Down(btn) => self.handle_click(btn),
            MouseEventKind::ScrollDown => self.handle_mouse_scroll(Direction::Down),
            MouseEventKind::ScrollUp => self.handle_mouse_scroll(Direction::Up),
            MouseEventKind::Moved => self.handle_mouse_move((mouse_event.row, mouse_event.column)),
            _ => {}
        }
        Ok(())
    }

    /// Handles the tick event of the terminal.
    ///
    /// The tick event is where you can update the state of your application with any logic that
    /// needs to be updated at a fixed frame rate. E.g. polling a server, updating an animation.
    fn tick(&mut self) {
        if time_for_update(self.last_updated, self.config.svn_status_timeout) {
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

    /// Handles any mouse clicks within the UI.
    fn handle_click(&mut self, button: MouseButton) {
        self.events.send(AppEvent::Message(format!(
            "{:?} {:?} {:?}",
            self.state, self.selected_section, self.mouse_loc
        )));
        let section = self.current_mouse_section();
        match section {
            Some(AppSection::Changes) => {
                if let Some(rect) = self.changes_area {
                    let offset = self.mouse_loc.0 - rect.y;
                    let index = (offset as usize + self.list_state.offset()).saturating_sub(1);
                    if button == MouseButton::Right {
                        if index <= self.file_list.renderable().len() {
                            *self.list_state.selected_mut() = Some(index);
                            self.change_popup_area = None;
                            self.state = AppState::ChangePopup;
                        }
                    } else {
                        self.state = AppState::Main;
                        self.change_popup_area = None;
                    }
                    if button == MouseButton::Left {
                        if index <= self.file_list.renderable().len() {
                            *self.list_state.selected_mut() = Some(index);
                        } else {
                            *self.list_state.selected_mut() = None;
                        }
                    }
                }
            }
            Some(AppSection::ChangePopup) => {}
            _ => {
                *self.list_state.selected_mut() = None;
                self.state = AppState::Main;
                self.change_popup_area = None;
            }
        }
        self.selected_section = section;
    }

    fn get_selected_change(&self) -> Option<&ParsedStatusLine> {
        if let Some(index) = self.list_state.selected() {
            self.file_list.get(index)
        } else {
            None
        }
    }

    fn handle_mouse_scroll(&mut self, dir: Direction) {
        match self.current_mouse_section() {
            Some(AppSection::Changes) => {
                if let Some(selected) = self.list_state.selected_mut() {
                    handle_scroll(&dir, selected, &mut self.changes_scrollbar_state)
                }
            }
            Some(AppSection::Conflicts) => handle_scroll(
                &dir,
                &mut self.conflicts_scroll_offset,
                &mut self.conflicts_scrollbar_state,
            ),
            _ => {}
        }
    }

    fn current_mouse_section(&self) -> Option<AppSection> {
        for (area, app_section) in [
            // this needs to be in the order that popups/dialogs sit above section in Main, as the rects for each section are still Some(_) even wh en popups are above them
            (self.change_popup_area, AppSection::ChangePopup),
            (self.changes_area, AppSection::Changes),
            (self.conflicts_area, AppSection::Conflicts),
        ] {
            if let Some(area) = area {
                let pos = Position {
                    x: self.mouse_loc.1,
                    y: self.mouse_loc.0,
                };
                if area.contains(pos) {
                    return Some(app_section);
                }
            }
        }
        None
    }

    fn handle_mouse_move(&mut self, pos: (u16, u16)) {
        self.mouse_loc = pos;
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

fn time_for_update(last_updated: DateTime<Utc>, timeout: u8) -> bool {
    Utc::now().signed_duration_since(last_updated).num_seconds() > timeout.into()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppSection {
    Changes,
    Conflicts,
    ChangePopup,
}
