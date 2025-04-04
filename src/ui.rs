use std::{ffi::OsStr, path::PathBuf};

use color_eyre::owo_colors::OwoColorize;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Clear, List, Paragraph, Scrollbar, ScrollbarOrientation},
};

use crate::{
    app::{App, AppSection, AppState},
    svn::{self, Conflict, ParsedStatusLine, state::State},
};

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let should_render_conflicts: bool = self.file_list.has_conflicts()
            && self
                .get_selected_change()
                .as_ref()
                .is_some_and(|change| change.0 == State::Conflicting);
        let should_render_change_popup = self.state == AppState::ChangePopup;
        let mut constraints = vec![
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(1),
        ];
        if should_render_conflicts {
            constraints.insert(2, Constraint::Percentage(20));
        }
        let layout = Layout::vertical(constraints).split(frame.area());
        let mut i = 0;
        self.render_branch_box(frame, layout[i]);
        i += 1;
        self.render_file_list(frame, layout[i]);
        i += 1;
        if should_render_conflicts {
            self.render_conflicts(frame, layout[i]);
            i += 1;
        }
        if should_render_change_popup {
            self.render_change_popup(frame);
        }
        self.render_message_box(frame, layout[i]);
    }

    fn render_change_popup<'a>(&'a mut self, frame: &mut Frame) {
        self.selected_section = Some(AppSection::ChangePopup);
        let (state, path_buf) = self
            .get_selected_change()
            .expect("Somehow opened a changed popup without a selected change?!");
        let path = path_buf.file_name().unwrap().to_str().unwrap();
        let popup = Block::bordered().title(path);
        let button = |title: &'a str, color: Color| {
            Paragraph::new(title)
                .centered()
                .block(Block::bordered())
                .fg(color)
        };
        let mut buttons = vec![button("Open", Color::Reset)];
        if state.is_deletable() {
            buttons.push(button("Delete", Color::Red));
        }
        if state.is_revertable() {
            buttons.push(button("Revert", Color::Yellow));
        }
        if state.is_commitable() {
            buttons.push(button("Commit", Color::Green));
        }
        let constraints = vec![Constraint::Length(3); buttons.len()];
        let area = self.change_popup_area.unwrap_or({
            let (row, col) = self.mouse_loc;
            let width = std::cmp::min(40, frame.area().width - col - 2);
            let height = std::cmp::min(buttons.len() as u16 * 3 + 2, frame.area().height - row - 2);
            Rect {
                x: col,
                y: row,
                width,
                height,
            }
        });
        frame.render_widget(Clear, area); // clear the popup area
        let layout = Layout::vertical(constraints).split(area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        }));
        for i in 0..buttons.len() {
            frame.render_widget(
                buttons.pop().expect("We somehow ran out of buttons?"),
                layout[i],
            );
        }
        frame.render_widget(popup, area);
        self.change_popup_area = Some(area);
    }

    fn render_conflicts(&mut self, frame: &mut Frame, area: Rect) {
        let conflicts = self.file_list.conflicts();
        let max_width = area.width - 1 - 1 - 1; // 1 for state, 1 each side for block borders, 1 for scrollbar
        let conflict_texts: Vec<Line> = conflicts
            .iter()
            .map(|c| transform_conflict(c, max_width))
            .fold(vec![], |mut a, b| {
                a.extend(b);
                a
            });
        self.conflicts_scrollbar_state = self
            .conflicts_scrollbar_state
            .content_length(conflict_texts.len());
        let mut block = Block::bordered().title("Conflicts");
        if self.selected_section == Some(AppSection::Conflicts) {
            block = block.style(Color::Yellow);
        }
        let paragraph = Paragraph::new(conflict_texts)
            .block(block)
            .scroll((self.conflicts_scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                horizontal: 0,
                vertical: 1,
            }),
            &mut self.conflicts_scrollbar_state,
        );
        self.conflicts_area = Some(area);
    }

    fn render_branch_box(&self, frame: &mut Frame, area: Rect) {
        let branch_box = Block::bordered().title("Branch");
        let branch = Paragraph::new(Text::from(vec![
            Line::raw(&*self.current_branch).style(Color::Cyan),
            Line::raw(self.cwd.to_str().unwrap()).style(Color::DarkGray),
        ]))
        .block(branch_box);
        frame.render_widget(branch, area);
    }

    fn render_file_list(&mut self, frame: &mut Frame, area: Rect) {
        let max_width = area.width - 3; // 1 each side for block borders, 1 for scrollbar
        let mut block = Block::bordered().title("Changes");
        if self.selected_section == Some(AppSection::Changes) {
            block = block.style(Color::Yellow);
        }
        let list = List::new(
            self.file_list
                .list()
                .iter()
                .filter(|(_, path)| !svn::is_conflict_part(path.to_str().expect("bad path")))
                .map(|psl| create_file_list_item(psl, max_width)),
        )
        .highlight_style(
            Style::new()
                .bg(Color::from_u32(0x00222222))
                .add_modifier(Modifier::BOLD),
        )
        .scroll_padding(1)
        .block(block);
        self.changes_scrollbar_state = self.changes_scrollbar_state.content_length(list.len());
        let list_length = list.len() as u16;
        frame.render_stateful_widget(list, area, &mut self.list_state);
        if area.height - 2 < list_length as u16 {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            frame.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    horizontal: 0,
                    vertical: 1,
                }),
                &mut self.changes_scrollbar_state,
            );
        }
        self.changes_area = Some(area);
    }

    fn render_message_box(&self, frame: &mut Frame, area: Rect) {
        let help = Line::from(vec![Span::raw(&self.last_message)]).style(Color::Gray);
        frame.render_widget(help, area);
    }
}

fn transform_conflict<'a>(conflict: &'a Conflict, max_width: u16) -> Vec<Line<'a>> {
    let make_line = |p: &'a PathBuf, color: Color| {
        let mut text = p.to_str().expect("bad path").to_string();
        if text.len() as u16 > max_width {
            text = text.split_at(max_width as usize - 3).0.to_string();
            text.push_str("...");
        }
        Line::raw(text).style(color)
    };
    match conflict {
        Conflict::Text {
            file,
            left,
            right,
            working,
        } => match (left, right, working) {
            (Some(l), Some(r), Some(w)) => vec![
                make_line(file, Color::Magenta),
                make_line(l, Color::DarkGray),
                make_line(w, Color::DarkGray),
                make_line(r, Color::DarkGray),
                Line::raw(""),
            ],
            _ => panic!("can there even be a conflict without all 3 parts?"),
        },
    }
}

/// Errors from PathBuf transformations are shown inline in the list view
fn create_file_list_item<'a>((state, path): &'a ParsedStatusLine, max_width: u16) -> Line<'a> {
    let state_span = match state {
        State::Modified => Span::from(state.to_string()).style(Color::Yellow),
        State::Added => Span::from(state.to_string()).style(Color::Green),
        State::Deleted => Span::from(state.to_string()).style(Color::Red),
        State::Missing => Span::from(state.to_string()).style(
            Style::new()
                .fg(Color::Red)
                .add_modifier(Modifier::RAPID_BLINK),
        ),
        State::Replaced => Span::from(state.to_string()).style(Color::Cyan),
        State::Unversioned => Span::from(state.to_string()).style(Color::White),
        State::Conflicting => Span::from(state.to_string()).style(Color::LightMagenta),
        State::Clean => Span::from(state.to_string()).style(Color::DarkGray),
    };
    let mut filename = path
        .to_str()
        .unwrap_or(&format!("ui.create_list_item issue: {path:?}"))
        .to_string();
    let spacer = "   ";
    if max_width < 100 {
        // with a really wide terminal space we can just show the whole paths!
        filename = path
            .file_name()
            .unwrap_or(OsStr::new(".")) // TODO this isn't necessarily always true
            .to_str()
            .unwrap_or(&format!("ui.create_list_item issue: {path:?}"))
            .to_string();
        if (state_span.width() + spacer.len() + filename.len()) as u16 >= max_width {
            filename = filename
                .split_at_checked((max_width - 3) as usize)
                .expect("we should always be able to split here")
                .0
                .to_string();
            filename.push_str("...");
        }
    }
    let path_color = match state {
        State::Clean => Color::DarkGray,
        _ => Color::Reset,
    };
    Line::from(vec![
        state_span,
        Span::raw(spacer),
        Span::raw(filename).fg(path_color),
    ])
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn popup_area(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]);
    let horizontal = Layout::horizontal([Constraint::Length(width)]);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}
