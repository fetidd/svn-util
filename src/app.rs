mod ui;
use crate::{
    command::{CmdResult, run_command},
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
use std::path::PathBuf;

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
    // UI areas mainly used for mouse clicks etc.
    changes_area: Option<Rect>,
    change_popup_area: Option<Rect>,
    config: Config,
    mouse_loc: (u16, u16), // row, col
    state: AppState,
    has_focus: bool,
    last_message: String,
    buttons: Vec<(Rect, fn(&mut App))>,
    _multiselection: Option<Vec<usize>>,
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
        let file_list = svn::FileList::empty();
        let list_state = ListState::default();
        let changes_scrollbar_state = ScrollbarState::default();
        Self {
            running: true,
            events: EventHandler::new(),
            current_branch: String::new(),
            file_list,
            last_updated: Utc::now(),
            cwd: PathBuf::new(),
            list_state,
            changes_scrollbar_state,
            changes_area: None,
            config: Config::default(),
            mouse_loc: (0, 0),
            state: AppState::Main,
            change_popup_area: None,
            last_message: String::new(),
            has_focus: true,
            buttons: vec![],
            _multiselection: None,
        }
    }

    pub fn with_config(self, config: Config) -> Self {
        Self { config, ..self }
    }

    /// Run the application's main loop.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        let cwd = std::env::current_dir()
            .expect("does this directory exist? do you have permissions on this dir?");
        self.current_branch = match svn::get_branch_name(&cwd) {
            Ok(branch) => branch,
            Err(e) => panic!("Issue in App creation: {e}"),
        };
        if let Ok(status) = svn::get_svn_status(&cwd) {
            *self.file_list.list_mut() = status;
        }
        self.cwd = cwd;
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
                CtEvent::FocusLost => {
                    self.close_change_popup();
                    *self.list_state.selected_mut() = None;
                    self.has_focus = false;
                }
                CtEvent::FocusGained => {
                    self.update_branch_name();
                    self.update_svn_status();
                    self.has_focus = true;
                }
                _ => {}
            },
            Event::App(app_event) => match app_event {
                AppEvent::Quit => self.quit(),
                AppEvent::UpdateRequest => {
                    self.update_branch_name();
                    self.update_svn_status();
                }
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
            _ => {}
        }
        Ok(())
    }

    fn handle_mouse_event(&mut self, mouse_event: MouseEvent) -> color_eyre::Result<()> {
        self.mouse_loc = (mouse_event.row, mouse_event.column);
        match mouse_event.kind {
            MouseEventKind::Down(btn) => self.handle_click(btn),
            MouseEventKind::ScrollDown => self.handle_mouse_scroll(Direction::Down),
            MouseEventKind::ScrollUp => self.handle_mouse_scroll(Direction::Up),
            MouseEventKind::Moved => self.handle_mouse_move(),
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
        match svn::get_svn_status(&self.cwd) {
            Ok(status) => *self.file_list.list_mut() = status,
            Err(error) => self.events.send(AppEvent::Message(error.to_string())),
        }
        self.last_updated = Utc::now();
    }

    fn update_branch_name(&mut self) {
        self.current_branch = match svn::get_branch_name(&self.cwd) {
            Ok(branch) => branch,
            Err(e) => e.to_string(),
        };
    }

    /// Handles any mouse clicks within the UI.
    fn handle_click(&mut self, button: MouseButton) {
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
                        self.close_change_popup();
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
            Some(AppSection::ChangePopup) => {
                let pos = Position {
                    // TODO make App.mouse_loc a Position
                    x: self.mouse_loc.1,
                    y: self.mouse_loc.0,
                };
                if let Some(func) = self.buttons.iter().fold(None, |mut a, (rect, func)| {
                    if rect.contains(pos) {
                        a = Some(func);
                    }
                    a
                }) {
                    func(self);
                }
                self.close_change_popup();
            }
            _ => {
                *self.list_state.selected_mut() = None;
                self.close_change_popup();
            }
        }
    }

    fn close_change_popup(&mut self) {
        self.state = AppState::Main;
        self.change_popup_area = None;
    }

    fn get_selected_changes(&self) -> Option<Vec<&ParsedStatusLine>> {
        if let Some(index) = self.list_state.selected() {
            if let Some(change) = self.file_list.get(index) {
                Some(vec![change])
            } else {
                None
            }
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
            _ => {}
        }
    }

    fn current_mouse_section(&self) -> Option<AppSection> {
        for (area, app_section) in [
            // this needs to be in the order that popups/dialogs sit above section in Main,
            // as the rects for each section are still Some(_) even wh en popups are above them
            (self.change_popup_area, AppSection::ChangePopup),
            (self.changes_area, AppSection::Changes),
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

    fn handle_mouse_move(&mut self) {}

    fn perform_svn_function(&mut self, func: fn(&[&str]) -> svn::error::Result<CmdResult>) {
        if let Some(selected) = self.get_selected_changes() {
            let paths = selected.into_iter().fold(vec![], |mut a, b| {
                a.push(b.1.to_string_lossy().to_string());
                a
            });
            let path_strs: Vec<&str> = paths.iter().map(|s| s.as_ref()).collect();
            match func(path_strs.as_slice()) {
                Ok(res) if res.success() => self.update_svn_status(),
                Ok(res) => self
                    .events
                    .send(AppEvent::Message(res.output().to_string())), // TODO delete reaches here when the file has modification, as svn requires --force to be passed, this could be used to have a "are you sure?" dialog
                Err(e) => self.events.send(AppEvent::Message(e.to_string())),
            }
        }
    }

    fn delete_change_file(&mut self) {
        self.perform_svn_function(svn::svn_delete);
    }

    fn add_change_file(&mut self) {
        self.perform_svn_function(svn::svn_add);
    }

    fn revert_change_file(&mut self) {
        self.perform_svn_function(svn::svn_revert);
    }

    fn commit_change_file(&mut self) {
        self.perform_svn_function(svn::svn_commit);
    }

    fn open_change_file(&mut self) {
        if let Some(selected) = self.get_selected_changes() {
            if let Some((_, path)) = selected.first() {
                match run_command(
                    "zellij",
                    vec![
                        "edit",
                        "-f",
                        "--height",
                        "70%",
                        "--width",
                        "70%",
                        "-x",
                        "15%",
                        "-y",
                        "15%",
                        path.to_string_lossy().as_ref(),
                    ]
                    .as_slice(),
                ) {
                    Ok(res) => {
                        if !res.success() {
                            self.events
                                .send(AppEvent::Message(res.output().to_string()))
                        }
                    }
                    Err(e) => self.events.send(AppEvent::Message(e.to_string())),
                }
            }
        }
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
    ChangePopup,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use rstest::*;
    use svn::state::State;

    fn rect(loc: u16) -> Rect {
        Rect {
            x: loc,
            y: loc,
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn test_handle_click() {
        let mut a = App::new();
        a.changes_area = Some(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 5,
        });
        let file_list = vec![
            (State::Modified, PathBuf::from("path1")),
            (State::Modified, PathBuf::from("path2")),
            (State::Modified, PathBuf::from("path3")),
        ];
        *a.file_list.list_mut() = file_list.clone();
        a.list_state = ListState::default();

        a.mouse_loc = (3, 0);
        a.handle_click(MouseButton::Left);
        a.handle_events().unwrap();

        assert_eq!(a.state, AppState::Main);
        assert_eq!(a.change_popup_area, None);
        assert_eq!(a.list_state.selected(), Some(2));
        assert_eq!(a.get_selected_changes(), Some(vec![&file_list[2]]))
    }

    #[rstest]
    #[case(Direction::Down, 0, 0, Some(0), 0, 0, Some(1), 1)]
    #[case(Direction::Down, 1, 0, Some(0), 1, 0, Some(1), 1)]
    #[case(Direction::Down, 1, 0, Some(1), 0, 0, Some(2), 2)]
    fn test_handle_mouse_scroll_changes_section(
        #[case] dir: Direction,
        #[case] cont_length: usize,
        #[case] offset: usize,
        #[case] selected: Option<usize>,
        #[case] position: usize,
        #[case] exp_offset: usize,
        #[case] exp_selected: Option<usize>,
        #[case] exp_position: usize,
    ) {
        let mut a = App::new();
        a.mouse_loc = (0, 0);
        a.changes_area = Some(rect(0));
        a.list_state = a.list_state.with_offset(offset).with_selected(selected);
        a.changes_scrollbar_state = a
            .changes_scrollbar_state
            .content_length(cont_length)
            .position(position);
        let exp_list_state = ListState::default()
            .with_offset(exp_offset)
            .with_selected(exp_selected);
        let exp_scroll_state = ScrollbarState::new(cont_length).position(exp_position);
        a.handle_mouse_scroll(dir);
        assert_eq!(exp_list_state, a.list_state);
        assert_eq!(exp_scroll_state, a.changes_scrollbar_state);
    }

    #[rstest]
    #[case(Some(rect(0)), None, (0, 0), Some(AppSection::Changes))]
    #[case(None, Some(rect(2)), (2, 2), Some(AppSection::ChangePopup))]
    #[case(Some(rect(0)), Some(rect(0)), (0, 0), Some(AppSection::ChangePopup))]
    #[case(Some(rect(0)), Some(rect(2)), (3, 3), None)]
    fn test_current_mouse_section(
        #[case] changes: Option<Rect>,
        #[case] change_popup: Option<Rect>,
        #[case] loc: (u16, u16),
        #[case] expected: Option<AppSection>,
    ) {
        let mut a = App::new();
        a.changes_area = changes;
        a.change_popup_area = change_popup;
        a.mouse_loc = loc;
        assert_eq!(expected, a.current_mouse_section());
    }

    #[rstest]
    #[case(-3, 5, false)]
    #[case(-4, 5, false)]
    #[case(-5, 5, false)]
    #[case(-6, 5, true)]
    fn test_time_for_update(
        #[case] last_updated: i64,
        #[case] timeout: u8,
        #[case] expected: bool,
    ) {
        let last_updated = Utc::now().checked_add_signed(TimeDelta::seconds(last_updated));
        assert_eq!(expected, time_for_update(last_updated.unwrap(), timeout));
    }

    #[rstest]
    #[case(1, 2, Direction::Up, 1)]
    #[case(1, 1, Direction::Up, 0)]
    #[case(1, 0, Direction::Up, 0)]
    #[case(1, 2, Direction::Down, 3)]
    #[case(1, 1, Direction::Down, 2)]
    #[case(1, 0, Direction::Down, 1)]
    #[case(2, 2, Direction::Down, 3)]
    #[case(2, 1, Direction::Down, 2)]
    #[case(2, 0, Direction::Down, 1)]
    fn test_handle_scroll(
        #[case] start_pos: usize,
        #[case] offset: usize,
        #[case] dir: Direction,
        #[case] exp_offset: usize,
    ) {
        let mut scroll_state = ScrollbarState::new(3).position(start_pos);
        let mut offset = offset;
        handle_scroll(&dir, &mut offset, &mut scroll_state);
        assert_eq!(exp_offset, offset, "offset = {offset:?}");
        assert_eq!(
            ScrollbarState::new(3).position(offset),
            scroll_state,
            "state = {scroll_state:?}"
        );
    }
}
