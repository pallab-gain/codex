use crate::key_hint;
use crate::key_hint::KeyBinding;
use crate::render::renderable::Renderable;
use codex_app_server_protocol::BackgroundTerminal;
use codex_app_server_protocol::TerminalInputRedactionKind;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

pub(crate) const ATTACHED_TERMINAL_DETACH_KEY: KeyBinding = key_hint::ctrl(KeyCode::Char(']'));
pub(crate) const ATTACHED_TERMINAL_SECURE_INPUT_KEY: KeyBinding =
    key_hint::ctrl(KeyCode::Char('s'));

const HEADER_ROWS: u16 = 1;
const TERMINAL_SCROLLBACK_ROWS: usize = 10_000;

pub(crate) struct AttachedTerminal {
    pub(crate) process_id: String,
    command: String,
    parser: vt100::Parser,
    secure_input_prompt: Option<TerminalInputRedactionKind>,
    secure_prompt_auto_opened: bool,
    last_viewport_size: Option<(u16, u16)>,
}

impl AttachedTerminal {
    pub(crate) fn new(
        terminal: BackgroundTerminal,
        initial_output: &[u8],
        rows: u16,
        cols: u16,
    ) -> Self {
        let mut parser = vt100::Parser::new(rows.max(1), cols.max(1), TERMINAL_SCROLLBACK_ROWS);
        parser.process(initial_output);
        Self {
            process_id: terminal.process_id,
            command: terminal.command,
            parser,
            secure_input_prompt: terminal.secure_input_prompt,
            secure_prompt_auto_opened: false,
            last_viewport_size: None,
        }
    }

    pub(crate) fn apply_output(&mut self, delta: &[u8]) {
        self.parser.process(delta);
    }

    pub(crate) fn set_secure_input_prompt(
        &mut self,
        secure_input_prompt: Option<TerminalInputRedactionKind>,
    ) {
        if self.secure_input_prompt != secure_input_prompt {
            self.secure_prompt_auto_opened = false;
        }
        self.secure_input_prompt = secure_input_prompt;
    }

    pub(crate) fn take_secure_input_prompt_to_open(
        &mut self,
    ) -> Option<TerminalInputRedactionKind> {
        match self.secure_input_prompt {
            Some(kind) if !self.secure_prompt_auto_opened => {
                self.secure_prompt_auto_opened = true;
                Some(kind)
            }
            Some(_) | None => None,
        }
    }

    pub(crate) fn secure_input_prompt(&self) -> Option<TerminalInputRedactionKind> {
        self.secure_input_prompt
    }

    pub(crate) fn viewport_size_changed(&mut self, rows: u16, cols: u16) -> Option<(u16, u16)> {
        let next = (rows.max(1), cols.max(1));
        if self.last_viewport_size == Some(next) {
            return None;
        }
        self.last_viewport_size = Some(next);
        self.parser.screen_mut().set_size(next.0, next.1);
        Some(next)
    }

    pub(crate) fn encode_key_event(key_event: KeyEvent) -> Option<String> {
        let modifiers = key_event.modifiers;
        match key_event.code {
            KeyCode::Char(c)
                if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT =>
            {
                Some(c.to_string())
            }
            KeyCode::Char(c) if modifiers == KeyModifiers::CONTROL => {
                encode_ctrl_char(c).map(|byte| (byte as char).to_string())
            }
            KeyCode::Enter => Some("\r".to_string()),
            KeyCode::Tab => Some("\t".to_string()),
            KeyCode::BackTab => Some("\u{1b}[Z".to_string()),
            KeyCode::Backspace => Some("\u{7f}".to_string()),
            KeyCode::Delete => Some("\u{1b}[3~".to_string()),
            KeyCode::Insert => Some("\u{1b}[2~".to_string()),
            KeyCode::Up => Some("\u{1b}[A".to_string()),
            KeyCode::Down => Some("\u{1b}[B".to_string()),
            KeyCode::Right => Some("\u{1b}[C".to_string()),
            KeyCode::Home => Some("\u{1b}[H".to_string()),
            KeyCode::End => Some("\u{1b}[F".to_string()),
            KeyCode::PageUp => Some("\u{1b}[5~".to_string()),
            KeyCode::PageDown => Some("\u{1b}[6~".to_string()),
            _ => None,
        }
    }
}

fn encode_ctrl_char(c: char) -> Option<u8> {
    match c {
        '@' | ' ' => Some(0x00),
        'a'..='z' => Some((c as u8) - b'a' + 1),
        'A'..='Z' => Some((c as u8) - b'A' + 1),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        _ => None,
    }
}

impl Renderable for AttachedTerminal {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let header = if let Some(kind) = self.secure_input_prompt {
            format!(
                "Attached to terminal {} · {} · {} prompt detected · ctrl+s securely · ctrl+] detach",
                self.process_id,
                self.command,
                secure_input_prompt_label(kind),
            )
        } else {
            format!(
                "Attached to terminal {} · {} · ctrl+s securely · ctrl+] detach",
                self.process_id, self.command,
            )
        };
        Paragraph::new(Line::from(vec![header.bold()]))
            .wrap(Wrap { trim: false })
            .render(
                Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: HEADER_ROWS.min(area.height),
                },
                buf,
            );

        let body_area = Rect {
            x: area.x,
            y: area.y.saturating_add(HEADER_ROWS),
            width: area.width,
            height: area.height.saturating_sub(HEADER_ROWS),
        };
        if body_area.is_empty() {
            return;
        }

        let lines: Vec<Line<'static>> = self
            .parser
            .screen()
            .rows(0, body_area.width)
            .take(body_area.height as usize)
            .map(Line::from)
            .collect();
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(body_area, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        0
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        if area.height <= HEADER_ROWS {
            return None;
        }
        let (row, col) = self.parser.screen().cursor_position();
        let max_row = area.height.saturating_sub(HEADER_ROWS).saturating_sub(1);
        let max_col = area.width.saturating_sub(1);
        Some((
            area.x.saturating_add(col.min(max_col)),
            area.y
                .saturating_add(HEADER_ROWS)
                .saturating_add(row.min(max_row)),
        ))
    }
}

fn secure_input_prompt_label(kind: TerminalInputRedactionKind) -> &'static str {
    match kind {
        TerminalInputRedactionKind::Password => "password",
        TerminalInputRedactionKind::Passphrase => "passphrase",
        TerminalInputRedactionKind::Pin => "PIN",
        TerminalInputRedactionKind::Unknown => "secret",
    }
}
