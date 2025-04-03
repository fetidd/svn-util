use std::{ffi::OsStr, path::PathBuf};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, List, Paragraph, Scrollbar, ScrollbarOrientation},
};

use crate::{
    app::App,
    svn::{self, Conflict, ParsedStatusLine, state::State},
};

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let has_conflicts: bool = self.file_list.has_conflicts();
        let mut constraints = vec![
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(1),
        ];
        if has_conflicts {
            constraints.insert(2, Constraint::Percentage(20));
        }
        let layout = Layout::vertical(constraints).split(frame.area());
        let mut i = 0;
        self.render_branch_box(frame, layout[i]);
        i += 1;
        self.render_file_list(frame, layout[i]);
        i += 1;
        if has_conflicts {
            self.render_conflicts(frame, layout[i]);
            i += 1;
        }
        self.render_help(frame, layout[i]);
    }

    fn render_conflicts(&mut self, frame: &mut Frame, area: Rect) {
        let conflicts = self.file_list.conflicts();
        let conflict_texts: Vec<Line> =
            conflicts
                .iter()
                .map(transform_conflict)
                .fold(vec![], |mut a, b| {
                    a.extend(b);
                    a
                });
        self.conflicts_scrollbar_state = self
            .conflicts_scrollbar_state
            .content_length(conflict_texts.len());
        let mut block = Block::bordered().title("Conflicts");
        if self.selected_section == Some(crate::app::AppSection::Conflicts) {
            block = block.style(Color::White);
        }
        let paragraph = Paragraph::new(conflict_texts)
            .block(block)
            .scroll((self.conflicts_scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                horizontal: 1,
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
        let max_width = area.width - 5; // 1 for state, 2 for gap, 1 each side for block borders
        let mut block = Block::bordered().title("Changes");
        if self.selected_section == Some(crate::app::AppSection::Changes) {
            block = block.style(Color::White);
        }
        let list = List::new(
            self.file_list
                .list()
                .iter()
                .filter(|(state, path)| {
                    *state != State::Clean
                        && !svn::is_conflict_part(path.to_str().expect("bad path"))
                }) // TODO if we ever care about the list showing status columns 2+, this filter may need rmeoving
                .map(|psl| create_file_list_item(psl, max_width, &self.cwd)),
        )
        .highlight_style(Style::new().italic())
        .scroll_padding(1)
        .block(block);
        self.changes_scrollbar_state = self.changes_scrollbar_state.content_length(list.len());
        frame.render_stateful_widget(list, area, &mut self.list_state);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                horizontal: 1,
                vertical: 1,
            }),
            &mut self.changes_scrollbar_state,
        );
        self.changes_area = Some(area);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help = Line::from(vec![
            Span::raw("Refresh <r|R> "),
            Span::raw("Quit <q|Q> "),
            Span::raw("Toggle section <Tab> "),
        ])
        .style(Color::Gray);
        frame.render_widget(help, area);
    }
}

fn transform_conflict<'a>(conflict: &'a svn::Conflict) -> Vec<Line<'a>> {
    let make_line =
        |p: &'a PathBuf| Line::raw(p.to_str().expect("bad path")).style(Color::DarkGray);
    match conflict {
        Conflict::Text {
            file,
            left,
            right,
            working,
        } => match (left, right, working) {
            (Some(l), Some(r), Some(w)) => vec![
                Line::raw(file.to_str().unwrap()).style(Color::Magenta),
                make_line(l),
                make_line(w),
                make_line(r),
                Line::raw(""),
            ],
            _ => panic!("can there even be a conflict without all 3 parts?"),
        },
    }
}

/// Errors from PathBuf transformations are shown inline in the list view
fn create_file_list_item<'a>(
    (state, path): &'a ParsedStatusLine,
    max_width: u16,
    cwd: &PathBuf,
) -> Line<'a> {
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
    if max_width < 100 {
        // with a really wide terminal space we can just show the whole paths!
        filename = path
            .file_name()
            .unwrap_or(OsStr::new(".")) // TODO this isn't necessarily always true
            .to_str()
            .unwrap_or(&format!("ui.create_list_item issue: {path:?}"))
            .to_string();
        if let Some(parent) = path.parent() {
            if parent != cwd {
                if let Some(parent) = parent.components().last() {
                    filename = parent
                        .as_os_str()
                        .to_str()
                        .unwrap_or(&format!("ui.create_list_item issue: {parent:?}"))
                        .to_string()
                        + "/"
                        + &filename;
                }
            }
        }
        if (state_span.width() + 3 + filename.len()) as u16 > max_width {
            filename = filename
                .split_at_checked((max_width - 3) as usize)
                .expect("we should always be able to split here")
                .0
                .to_string();
            filename.push_str("...");
        }
    }
    Line::from(vec![
        state_span,
        Span::raw("   "),
        Span::raw(filename).fg(Color::Reset),
    ])
}
