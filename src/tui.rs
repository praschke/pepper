use std::{cmp::Ordering, io::Write, iter, sync::mpsc, thread};

use crossterm::{
    cursor, event, handle_command,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal, ErrorKind, Result,
};

use crate::{
    application::UI,
    buffer_position::BufferPosition,
    client::Client,
    client_event::{Key, LocalEvent},
    config::Config,
    editor_operation::StatusMessageKind,
    mode::Mode,
    syntax::TokenKind,
    theme,
};

fn convert_event(event: event::Event) -> LocalEvent {
    match event {
        event::Event::Key(e) => match e.code {
            event::KeyCode::Backspace => LocalEvent::Key(Key::Backspace),
            event::KeyCode::Enter => LocalEvent::Key(Key::Enter),
            event::KeyCode::Left => LocalEvent::Key(Key::Left),
            event::KeyCode::Right => LocalEvent::Key(Key::Right),
            event::KeyCode::Up => LocalEvent::Key(Key::Up),
            event::KeyCode::Down => LocalEvent::Key(Key::Down),
            event::KeyCode::Home => LocalEvent::Key(Key::Home),
            event::KeyCode::End => LocalEvent::Key(Key::End),
            event::KeyCode::PageUp => LocalEvent::Key(Key::PageUp),
            event::KeyCode::PageDown => LocalEvent::Key(Key::PageDown),
            event::KeyCode::Tab => LocalEvent::Key(Key::Tab),
            event::KeyCode::Delete => LocalEvent::Key(Key::Delete),
            event::KeyCode::F(f) => LocalEvent::Key(Key::F(f)),
            event::KeyCode::Char('\0') => LocalEvent::None,
            event::KeyCode::Char(c) => match e.modifiers {
                event::KeyModifiers::CONTROL => LocalEvent::Key(Key::Ctrl(c)),
                event::KeyModifiers::ALT => LocalEvent::Key(Key::Alt(c)),
                _ => LocalEvent::Key(Key::Char(c)),
            },
            event::KeyCode::Esc => LocalEvent::Key(Key::Esc),
            _ => LocalEvent::None,
        },
        event::Event::Resize(w, h) => LocalEvent::Resize(w, h),
        _ => LocalEvent::None,
    }
}

const fn convert_color(color: theme::Color) -> Color {
    Color::Rgb {
        r: color.0,
        g: color.1,
        b: color.2,
    }
}

pub struct Tui<W>
where
    W: Write,
{
    write: W,
    text_scroll: usize,
    select_scroll: usize,
    width: u16,
    height: u16,
}

impl<W> Tui<W>
where
    W: Write,
{
    pub fn new(write: W) -> Self {
        Self {
            write,
            text_scroll: 0,
            select_scroll: 0,
            width: 0,
            height: 0,
        }
    }
}

impl<W> UI for Tui<W>
where
    W: Write,
{
    type Error = ErrorKind;

    fn run_event_loop_in_background(
        event_sender: mpsc::Sender<LocalEvent>,
    ) -> thread::JoinHandle<Result<()>> {
        thread::spawn(move || {
            while event_sender.send(convert_event(event::read()?)).is_ok() {}
            Ok(())
        })
    }

    fn init(&mut self) -> Result<()> {
        handle_command!(self.write, terminal::EnterAlternateScreen)?;
        handle_command!(self.write, cursor::Hide)?;
        self.write.flush()?;
        terminal::enable_raw_mode()?;

        let size = terminal::size()?;
        self.resize(size.0, size.1)
    }

    fn resize(&mut self, width: u16, height: u16) -> Result<()> {
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn draw(
        &mut self,
        config: &Config,
        client: &Client,
        status_message_kind: StatusMessageKind,
        status_message: &str,
    ) -> Result<()> {
        let text_height = self.height - 1;
        let select_entry_count = if client.has_focus {
            client.select_entries.len() as u16
        } else {
            0
        };
        let select_height = select_entry_count.min(text_height / 2);
        let text_height = text_height - select_height;

        let cursor_position = client.main_cursor.position;
        if cursor_position.line_index < self.text_scroll {
            self.text_scroll = cursor_position.line_index;
        } else if cursor_position.line_index >= self.text_scroll + text_height as usize {
            self.text_scroll = cursor_position.line_index + 1 - text_height as usize;
        }

        let selected_index = client.select_entries.selected_index;
        if selected_index < self.select_scroll {
            self.select_scroll = selected_index;
        } else if selected_index >= self.select_scroll + select_height as usize {
            self.select_scroll = selected_index + 1 - select_height as usize;
        }

        draw_text(
            &mut self.write,
            config,
            client,
            self.text_scroll,
            self.width,
            text_height,
        )?;
        draw_select(
            &mut self.write,
            config,
            client,
            self.select_scroll,
            self.width,
            select_height,
        )?;
        draw_statusbar(
            &mut self.write,
            config,
            client,
            self.width,
            status_message_kind,
            status_message,
        )?;

        handle_command!(self.write, ResetColor)?;
        self.write.flush()?;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        handle_command!(self.write, terminal::Clear(terminal::ClearType::All))?;
        handle_command!(self.write, terminal::LeaveAlternateScreen)?;
        handle_command!(self.write, ResetColor)?;
        handle_command!(self.write, cursor::Show)?;
        self.write.flush()?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}

fn draw_text<W>(
    write: &mut W,
    config: &Config,
    client: &Client,
    scroll: usize,
    width: u16,
    height: u16,
) -> Result<()>
where
    W: Write,
{
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum DrawState {
        Token(TokenKind),
        Selection(TokenKind),
        Highlight,
        Cursor,
    }

    let theme = &config.theme;

    handle_command!(write, cursor::Hide)?;

    let cursor_color = match client.mode {
        Mode::Select => convert_color(theme.cursor_select),
        Mode::Insert => convert_color(theme.cursor_insert),
        _ => convert_color(theme.cursor_normal),
    };

    let background_color = convert_color(theme.background);
    let token_whitespace_color = convert_color(theme.token_whitespace);
    let token_text_color = convert_color(theme.token_text);
    let token_comment_color = convert_color(theme.token_comment);
    let token_keyword_color = convert_color(theme.token_keyword);
    let token_modifier_color = convert_color(theme.token_type);
    let token_symbol_color = convert_color(theme.token_symbol);
    let token_string_color = convert_color(theme.token_string);
    let token_literal_color = convert_color(theme.token_literal);
    let highlight_color = convert_color(theme.highlight);

    let mut text_color = token_text_color;

    handle_command!(write, cursor::MoveTo(0, 0))?;
    handle_command!(write, SetBackgroundColor(background_color))?;
    handle_command!(write, SetForegroundColor(text_color))?;

    let mut line_index = scroll;
    let mut drawn_line_count = 0;

    'lines_loop: for line in client.buffer.lines_from(line_index) {
        let mut draw_state = DrawState::Token(TokenKind::Text);
        let mut column_index = 0;
        let mut x = 0;

        handle_command!(write, SetForegroundColor(token_text_color))?;

        for (raw_char_index, c) in line.text(..).char_indices().chain(iter::once((0, '\0'))) {
            if x >= width {
                handle_command!(write, cursor::MoveToNextLine(1))?;

                drawn_line_count += 1;
                x -= width;

                if drawn_line_count >= height {
                    break 'lines_loop;
                }
            }

            let char_position = BufferPosition::line_col(line_index, column_index);

            let token_kind = if c.is_ascii_whitespace() {
                TokenKind::Whitespace
            } else {
                client
                    .highlighted_buffer
                    .find_token_kind_at(line_index, raw_char_index)
            };

            text_color = match token_kind {
                TokenKind::Whitespace => token_whitespace_color,
                TokenKind::Text => token_text_color,
                TokenKind::Comment => token_comment_color,
                TokenKind::Keyword => token_keyword_color,
                TokenKind::Type => token_modifier_color,
                TokenKind::Symbol => token_symbol_color,
                TokenKind::String => token_string_color,
                TokenKind::Literal => token_literal_color,
            };

            if client.cursors[..]
                .binary_search_by_key(&char_position, |c| c.position)
                .is_ok()
            {
                if draw_state != DrawState::Cursor {
                    draw_state = DrawState::Cursor;
                    handle_command!(write, SetBackgroundColor(cursor_color))?;
                    handle_command!(write, SetForegroundColor(text_color))?;
                }
            } else if client.cursors[..]
                .binary_search_by(|c| {
                    let range = c.range();
                    if range.to < char_position {
                        Ordering::Less
                    } else if range.from > char_position {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                })
                .is_ok()
            {
                if draw_state != DrawState::Selection(token_kind) {
                    draw_state = DrawState::Selection(token_kind);
                    handle_command!(write, SetBackgroundColor(text_color))?;
                    handle_command!(write, SetForegroundColor(background_color))?;
                }
            } else if client
                .search_ranges
                .binary_search_by(|r| {
                    if r.to < char_position {
                        Ordering::Less
                    } else if r.from > char_position {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                })
                .is_ok()
            {
                if draw_state != DrawState::Highlight {
                    draw_state = DrawState::Highlight;
                    handle_command!(write, SetBackgroundColor(highlight_color))?;
                    handle_command!(write, SetForegroundColor(background_color))?;
                }
            } else if draw_state != DrawState::Token(token_kind) {
                draw_state = DrawState::Token(token_kind);
                handle_command!(write, SetBackgroundColor(background_color))?;
                handle_command!(write, SetForegroundColor(text_color))?;
            }

            match c {
                '\0' => {
                    handle_command!(write, Print(' '))?;
                    x += 1;
                }
                ' ' => {
                    handle_command!(write, Print(config.values.visual_space))?;
                    x += 1;
                }
                '\t' => {
                    handle_command!(write, Print(config.values.visual_tab_first))?;
                    let tab_size = config.values.tab_size.get() as u16;
                    let next_tab_stop = (tab_size - 1) - x % tab_size;
                    for _ in 0..next_tab_stop {
                        handle_command!(write, Print(config.values.visual_tab_repeat))?;
                    }
                    x += tab_size;
                }
                _ => {
                    handle_command!(write, Print(c))?;
                    x += 1;
                }
            }

            column_index += 1;
        }

        if x < width {
            handle_command!(write, SetBackgroundColor(background_color))?;
            handle_command!(write, terminal::Clear(terminal::ClearType::UntilNewLine))?;
        }

        handle_command!(write, cursor::MoveToNextLine(1))?;

        line_index += 1;
        drawn_line_count += 1;

        if drawn_line_count >= height {
            break;
        }
    }

    handle_command!(write, SetBackgroundColor(background_color))?;
    handle_command!(write, SetForegroundColor(token_whitespace_color))?;
    for _ in drawn_line_count..height {
        handle_command!(write, Print(config.values.visual_empty))?;
        handle_command!(write, terminal::Clear(terminal::ClearType::UntilNewLine))?;
        handle_command!(write, cursor::MoveToNextLine(1))?;
    }

    Ok(())
}

fn draw_select<W>(
    write: &mut W,
    config: &Config,
    client: &Client,
    scroll: usize,
    _width: u16,
    height: u16,
) -> Result<()>
where
    W: Write,
{
    let background_color = convert_color(config.theme.token_whitespace);
    let foreground_color = convert_color(config.theme.token_text);

    handle_command!(write, SetBackgroundColor(background_color))?;
    handle_command!(write, SetForegroundColor(foreground_color))?;

    for entry in client.select_entries.entries_from(scroll).take(height as _) {
        handle_command!(write, Print(&entry.name[..]))?;
        handle_command!(write, terminal::Clear(terminal::ClearType::UntilNewLine))?;
        handle_command!(write, cursor::MoveToNextLine(1))?;
    }

    Ok(())
}

fn draw_statusbar<W>(
    write: &mut W,
    config: &Config,
    client: &Client,
    width: u16,
    status_message_kind: StatusMessageKind,
    status_message: &str,
) -> Result<()>
where
    W: Write,
{
    fn draw_input<W>(
        write: &mut W,
        prefix: &str,
        input: &str,
        background_color: Color,
        cursor_color: Color,
    ) -> Result<usize>
    where
        W: Write,
    {
        handle_command!(write, Print(prefix))?;
        handle_command!(write, Print(input))?;
        handle_command!(write, SetBackgroundColor(cursor_color))?;
        handle_command!(write, Print(' '))?;
        handle_command!(write, SetBackgroundColor(background_color))?;
        Ok(prefix.len() + input.len() + 1)
    }

    fn find_digit_count(mut number: usize) -> usize {
        let mut count = 0;
        while number > 0 {
            number /= 10;
            count += 1;
        }
        count
    }

    let background_color = convert_color(config.theme.token_text);
    let foreground_color = convert_color(config.theme.background);
    let cursor_color = convert_color(config.theme.cursor_normal);

    if client.has_focus {
        handle_command!(write, SetBackgroundColor(background_color))?;
        handle_command!(write, SetForegroundColor(foreground_color))?;
    } else {
        handle_command!(write, SetBackgroundColor(foreground_color))?;
        handle_command!(write, SetForegroundColor(background_color))?;
    }

    let x = if !status_message.is_empty() {
        let prefix = match status_message_kind {
            StatusMessageKind::Info => "",
            StatusMessageKind::Error => "error:",
        };

        let line_count = status_message.lines().count();
        if line_count > 1 {
            handle_command!(write, cursor::MoveUp(line_count as _))?;
            handle_command!(write, Print(prefix))?;
            handle_command!(write, terminal::Clear(terminal::ClearType::UntilNewLine))?;

            for line in status_message.lines() {
                handle_command!(write, cursor::MoveToNextLine(1))?;
                handle_command!(write, terminal::Clear(terminal::ClearType::CurrentLine))?;
                handle_command!(write, Print(line))?;
            }
        } else {
            handle_command!(write, Print(prefix))?;
            handle_command!(write, Print(status_message))?;
        }

        None
    } else if client.has_focus {
        match client.mode {
            Mode::Select => {
                let text = "-- SELECT --";
                handle_command!(write, Print(text))?;
                Some(text.len())
            }
            Mode::Insert => {
                let text = "-- INSERT --";
                handle_command!(write, Print(text))?;
                Some(text.len())
            }
            Mode::Search(_) => Some(draw_input(
                write,
                "/",
                &client.input[..],
                background_color,
                cursor_color,
            )?),
            Mode::Script(_) => Some(draw_input(
                write,
                ":",
                &client.input[..],
                background_color,
                cursor_color,
            )?),
            _ => Some(0),
        }
    } else {
        Some(0)
    };

    if let Some(x) = x {
        if let Some(buffer_path) = client.path.as_os_str().to_str().filter(|s| !s.is_empty()) {
            let line_number = client.main_cursor.position.line_index + 1;
            let column_number = client.main_cursor.position.column_index + 1;
            let line_digit_count = find_digit_count(line_number);
            let column_digit_count = find_digit_count(column_number);
            let skip = (width as usize).saturating_sub(
                x + buffer_path.len() + 1 + line_digit_count + 1 + column_digit_count + 1,
            );
            for _ in 0..skip {
                handle_command!(write, Print(' '))?;
            }

            handle_command!(write, Print(buffer_path))?;
            handle_command!(write, Print(':'))?;
            handle_command!(write, Print(line_number))?;
            handle_command!(write, Print(','))?;
            handle_command!(write, Print(column_number))?;
        }
    }

    handle_command!(write, terminal::Clear(terminal::ClearType::UntilNewLine))?;
    Ok(())
}
