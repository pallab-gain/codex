use std::cell::RefCell;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use crate::render::renderable::Renderable;

use super::CancellationEvent;
use super::bottom_pane_view::BottomPaneView;
use super::popup_consts::standard_popup_hint_line;
use super::textarea::TextArea;
use super::textarea::TextAreaState;

pub(crate) type SecureInputSubmitted = Box<dyn Fn(String) + Send + Sync>;

pub(crate) struct BackgroundTerminalSecureInputView {
    title: String,
    subtitle: String,
    on_submit: SecureInputSubmitted,
    textarea: TextArea,
    textarea_state: RefCell<TextAreaState>,
    complete: bool,
}

impl BackgroundTerminalSecureInputView {
    pub(crate) fn new(title: String, subtitle: String, on_submit: SecureInputSubmitted) -> Self {
        Self {
            title,
            subtitle,
            on_submit,
            textarea: TextArea::new(),
            textarea_state: RefCell::new(TextAreaState::default()),
            complete: false,
        }
    }
}

impl BottomPaneView for BackgroundTerminalSecureInputView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.complete = true;
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let text = self.textarea.text().to_string();
                if !text.is_empty() {
                    (self.on_submit)(text);
                    self.complete = true;
                }
            }
            other => self.textarea.input(other),
        }
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn handle_paste(&mut self, pasted: String) -> bool {
        self.textarea.insert_str(&pasted);
        true
    }
}

impl Renderable for BackgroundTerminalSecureInputView {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        Clear.render(area, buf);

        let title_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        Paragraph::new(Line::from(self.title.clone().bold())).render(title_area, buf);

        let subtitle_area = Rect {
            x: area.x,
            y: area.y.saturating_add(1),
            width: area.width,
            height: 1,
        };
        Paragraph::new(Line::from(self.subtitle.clone().dim())).render(subtitle_area, buf);

        let input_area = Rect {
            x: area.x,
            y: area.y.saturating_add(3),
            width: area.width,
            height: 1,
        };
        self.textarea.render_ref_masked(
            input_area,
            buf,
            &mut self.textarea_state.borrow_mut(),
            '\u{2022}',
            Style::default(),
        );

        let hint_area = Rect {
            x: area.x,
            y: area.y.saturating_add(area.height.saturating_sub(2)),
            width: area.width,
            height: 2,
        };
        Paragraph::new(vec![
            Line::from("Enter to send securely, Esc to cancel.".dim()),
            standard_popup_hint_line(),
        ])
        .render(hint_area, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        6
    }
}
