use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Clear, List, Paragraph, Scrollbar, ScrollbarOrientation},
};
use std::ffi::OsStr;

use crate::{
    app::{App, AppState},
    svn::{self, ParsedStatusLine, state::State},
};

const MINIMUM_UI_WIDTH: u16 = 15;

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        if frame.area().width < MINIMUM_UI_WIDTH {
            // guard against the ui being too narrow
            frame.render_widget(Span::raw("too small"), frame.area());
            return;
        }
        let should_render_change_popup = self.state == AppState::ChangePopup;
        let constraints = vec![
            Constraint::Length(4),
            Constraint::Fill(1),
            Constraint::Length(1),
        ];
        let layout = Layout::vertical(constraints).split(frame.area());
        let mut i = 0;
        self.render_branch_box(frame, layout[i]);
        i += 1;
        self.render_file_list(frame, layout[i]);
        i += 1;
        if should_render_change_popup {
            self.render_change_popup(frame);
        }
        self.render_message_box(frame, layout[i]);
    }

    fn calculate_popup_rect(&self, buttons: &[Text], allowed_area: Rect) -> Rect {
        let (row, mut col) = self.mouse_loc;
        let width = (buttons
            .iter()
            .map(|b| b.to_string().len()) // TODO this allocates String for each button, maybe have the list items know their lengths?
            .max()
            .expect("buttons was somehow empty?")
            + 6) as u16;
        let height = buttons.len() as u16;
        if col + width >= allowed_area.width {
            col = col.saturating_sub((col + width) - allowed_area.width);
        }
        Rect {
            x: col,
            y: row,
            width,
            height,
        }
    }

    fn render_change_popup(&mut self, frame: &mut Frame) {
        let (state, _) = self
            .get_selected_change()
            .expect("Somehow opened a changed popup without a selected change?!");
        let popup = Block::new().bg(Color::DarkGray);
        let button = |title: &'static str, color: Color| Text::raw(title).style(color);
        let mut btn_widgets = vec![button("Open", Color::LightBlue)];
        let mut btn_funcs = vec![App::open_change_file as fn(&mut App)];
        if state.is_deletable() {
            btn_widgets.push(button("Delete", Color::LightRed));
            btn_funcs.push(App::delete_change_file);
        }
        if state.is_revertable() {
            btn_widgets.push(button("Revert", Color::LightYellow));
            btn_funcs.push(App::revert_change_file);
        }
        if state.is_commitable() {
            btn_widgets.push(button("Commit", Color::LightGreen));
            btn_funcs.push(App::commit_change_file);
        }
        if state.is_addable() {
            btn_widgets.push(button("Add", Color::LightGreen));
            btn_funcs.push(App::add_change_file);
        }
        let constraints = vec![Constraint::Length(3); btn_widgets.len()];
        let popup_area = self
            .change_popup_area
            .unwrap_or(self.calculate_popup_rect(&btn_widgets, frame.area()));
        // RENDERING STARTS HERE
        frame.render_widget(Clear, popup_area); // clear the popup area
        let layout = Layout::vertical(constraints).split(popup_area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        }));
        let buttons = btn_widgets.into_iter().zip(btn_funcs);
        for (i, (widget, func)) in buttons.into_iter().enumerate() {
            let area = layout.get(i).expect("layout cannot fit the buttons");
            frame.render_widget(widget, *area);
            self.buttons.push((*area, func));
        }
        frame.render_widget(popup, popup_area);
        self.change_popup_area = Some(popup_area);
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
        let block = Block::bordered().title("Changes");
        let list = List::new(
            self.file_list
                .list()
                .iter()
                .filter(|(_, path)| !svn::is_conflict_part(path.to_str().expect("bad path")))
                .map(|psl| create_file_list_item(psl, max_width)),
        )
        .highlight_style(
            Style::new()
                .fg(Color::from_u32(0x00222222))
                .bg(Color::Gray)
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

// fn transform_conflict<'a>(conflict: &'a Conflict, max_width: u16) -> Vec<Line<'a>> {
//     let make_line = |p: &'a PathBuf, color: Color| {
//         let mut text = p.to_str().expect("bad path").to_string();
//         if text.len() as u16 > max_width {
//             text = text.split_at(max_width as usize - 3).0.to_string();
//             text.push_str("...");
//         }
//         Line::raw(text).style(color)
//     };
//     match conflict {
//         Conflict::Text {
//             file,
//             left,
//             right,
//             working,
//         } => match (left, right, working) {
//             (Some(l), Some(r), Some(w)) => vec![
//                 make_line(file, Color::Magenta),
//                 make_line(l, Color::DarkGray),
//                 make_line(w, Color::DarkGray),
//                 make_line(r, Color::DarkGray),
//                 Line::raw(""),
//             ],
//             _ => panic!("can there even be a conflict without all 3 parts?"),
//         },
//     }
// }

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
                .unwrap_or(("", ""))
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

#[cfg(test)]
mod tests {
    use super::*;
    use State::*;
    use rstest::*;

    #[rstest]
    #[case(Modified, "path/to/file.txt", 20, "M", "file.txt", Color::Yellow)]
    #[case(Added, "path/to/file.txt", 20, "A", "file.txt", Color::Green)]
    #[case(Deleted, "path/to/file.txt", 20, "D", "file.txt", Color::Red)]
    // #[case(Missing, "path/to/file.txt", 20, "!", "file.txt", Color::Red)]
    #[case(
        Conflicting,
        "path/to/file.txt",
        20,
        "C",
        "file.txt",
        Color::LightMagenta
    )]
    #[case(Replaced, "path/to/file.txt", 20, "R", "file.txt", Color::Cyan)]
    // #[case(Clean, "path/to/file.txt", 20, " ", "file.txt", Color::DarkGray)]
    #[case(Unversioned, "path/to/file.txt", 20, "?", "file.txt", Color::White)]
    #[case(Modified, "path/to/file.txt", 10, "M", "file.tx...", Color::Yellow)]
    #[case(
        Modified,
        "path/to/file.txt",
        100,
        "M",
        "path/to/file.txt",
        Color::Yellow
    )]
    fn test_create_file_list_item(
        #[case] state: State,
        #[case] path: &str,
        #[case] max_width: u16,
        #[case] exp_state: &str,
        #[case] exp_path: &str,
        #[case] exp_color: Color,
    ) {
        let psl = (state, path.into());
        let actual = create_file_list_item(&psl, max_width);
        let expected = Line {
            style: Style::new(),
            alignment: None,
            spans: vec![
                Span::from(exp_state).style(exp_color),
                Span::from("   "),
                Span::from(exp_path).fg(Color::Reset),
            ],
        };
        assert_eq!(expected, actual);
    }
}
