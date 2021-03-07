use std::{any, collections::VecDeque, fmt, str::FromStr};

use crate::{
    buffer::{Buffer, BufferCollection, BufferError, BufferHandle},
    buffer_view::BufferViewHandle,
    client::{Client, ClientHandle, ClientManager},
    editor::Editor,
    events::KeyParseError,
    pattern::PatternError,
    platform::Platform,
};

mod builtin;

pub const PARAMETERS_CAPACITY: usize = 8;
pub const HISTORY_CAPACITY: usize = 10;

pub enum CommandParseError<'command> {
    InvalidCommandName(&'command str),
    CommandNotFound(&'command str),
    CommandDoesNotAcceptBang(&'command str),
    UnterminatedArgument(&'command str),
    InvalidArgument(&'command str),
    TooFewArguments(&'command str, u8),
    TooManyArguments(&'command str, u8),
}

pub enum CommandError<'command> {
    Aborted,
    ParseError(CommandParseError<'command>),
    CommandNotFound(&'command str),
    UnsavedChanges,
    NoBufferOpened,
    InvalidBufferHandle(BufferHandle),
    InvalidPath(&'command str),
    ParseArgError {
        arg: &'command str,
        type_name: &'static str,
    },
    BufferError(BufferHandle, BufferError),
    ConfigNotFound(&'command str),
    InvalidConfigValue {
        key: &'command str,
        value: &'command str,
    },
    ColorNotFound(&'command str),
    InvalidColorValue {
        key: &'command str,
        value: &'command str,
    },
    InvalidGlob(&'command str),
    PatternError(&'command str, PatternError),
    InvalidModeError(&'command str),
    KeyParseError(&'command str, KeyParseError),
    InvalidRegisterKey(&'command str),
    LspServerNotRunning,
}
impl<'command> CommandError<'command> {
    pub fn display<'error>(
        &'error self,
        command: &'command str,
        buffers: &'error BufferCollection,
    ) -> CommandErrorDisplay<'command, 'error> {
        CommandErrorDisplay {
            command,
            buffers,
            error: self,
        }
    }
}

pub struct CommandErrorDisplay<'command, 'error> {
    command: &'command str,
    buffers: &'error BufferCollection,
    error: &'error CommandError<'command>,
}
impl<'command, 'error> fmt::Display for CommandErrorDisplay<'command, 'error> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn write(
            this: &CommandErrorDisplay,
            f: &mut fmt::Formatter,
            error_token: &str,
            message: fmt::Arguments,
        ) -> fmt::Result {
            let error_offset = error_token.as_ptr() as usize - this.command.as_ptr() as usize;
            let error_len = error_token.len();
            write!(
                f,
                "{}\n{: >offset$}{:^<len$}\n",
                this.command,
                "",
                "",
                offset = error_offset,
                len = error_len
            )?;
            f.write_fmt(message)?;
            Ok(())
        }

        match self.error {
            CommandError::Aborted => Ok(()),
            CommandError::ParseError(ref error) => match error {
                CommandParseError::InvalidCommandName(token) => write(
                    self,
                    f,
                    token,
                    format_args!("invalid command name '{}'", token),
                ),
                CommandParseError::CommandNotFound(token) => {
                    write(self, f, token, format_args!("no such command '{}'", token))
                }
                CommandParseError::CommandDoesNotAcceptBang(token) => write(
                    self,
                    f,
                    token,
                    format_args!("command '{}' does not accept bang", token),
                ),
                CommandParseError::UnterminatedArgument(token) => {
                    write(self, f, token, format_args!("unterminated argument"))
                }
                CommandParseError::InvalidArgument(token) => {
                    write(self, f, token, format_args!("invalid argument '{}'", token))
                }
                CommandParseError::TooFewArguments(token, len) => write(
                    self,
                    f,
                    token,
                    format_args!("command expects {} parameters", len),
                ),
                CommandParseError::TooManyArguments(token, len) => write(
                    self,
                    f,
                    token,
                    format_args!("command expects {} parameters", len),
                ),
            },
            CommandError::CommandNotFound(command) => {
                f.write_fmt(format_args!("no such command '{}'", command))
            }
            CommandError::UnsavedChanges => f.write_str(
                "there are unsaved changes. try appending a '!' to command name to force execute",
            ),
            CommandError::NoBufferOpened => f.write_str("no buffer opened"),
            CommandError::InvalidBufferHandle(handle) => {
                f.write_fmt(format_args!("invalid buffer handle {}", handle))
            }
            CommandError::InvalidPath(path) => {
                write(self, f, path, format_args!("invalid path '{}'", path))
            }
            CommandError::ParseArgError { arg, type_name } => write(
                self,
                f,
                arg,
                format_args!("could not parse '{}' as {}", arg, type_name),
            ),
            CommandError::BufferError(handle, error) => match self.buffers.get(*handle) {
                Some(buffer) => f.write_fmt(format_args!("{}", error.display(buffer))),
                None => Ok(()),
            },
            CommandError::ConfigNotFound(key) => {
                write(self, f, key, format_args!("no such config '{}'", key))
            }
            CommandError::InvalidConfigValue { key, value } => write(
                self,
                f,
                value,
                format_args!("invalid value '{}' for config '{}'", value, key),
            ),
            CommandError::ColorNotFound(key) => {
                write(self, f, key, format_args!("no such theme color '{}'", key))
            }
            CommandError::InvalidColorValue { key, value } => write(
                self,
                f,
                value,
                format_args!("invalid value '{}' for theme color '{}'", value, key),
            ),
            CommandError::InvalidGlob(glob) => {
                write(self, f, glob, format_args!("invalid glob '{}'", glob))
            }
            CommandError::PatternError(pattern, error) => {
                write(self, f, pattern, format_args!("{}", error))
            }
            CommandError::InvalidModeError(mode) => {
                write(self, f, mode, format_args!("no such mode '{}'", mode))
            }
            CommandError::KeyParseError(keys, error) => {
                write(self, f, keys, format_args!("{}", error))
            }
            CommandError::InvalidRegisterKey(key) => {
                write(self, f, key, format_args!("invalid register key '{}'", key))
            }
            CommandError::LspServerNotRunning => f.write_str("lsp server not running"),
        }
    }
}

type CommandFn =
    for<'state, 'command> fn(
        &mut CommandContext<'state, 'command>,
    ) -> Result<Option<CommandOperation>, CommandError<'command>>;

pub enum CommandOperation {
    Quit,
    QuitAll,
}

pub enum CompletionSource {
    Files,
    Buffers,
    Commands,
    Custom(&'static [&'static str]),
}

pub struct CommandContext<'state, 'command> {
    pub editor: &'state mut Editor,
    pub platform: &'state mut Platform,
    pub clients: &'state mut ClientManager,
    pub client_handle: Option<ClientHandle>,
    pub bang: bool,
    pub args: [&'command str; PARAMETERS_CAPACITY],
    pub output: &'state mut String,
}
impl<'state, 'command> CommandContext<'state, 'command> {
    pub fn parse_arg<T>(&self, index: usize) -> Result<T, CommandError<'command>>
    where
        T: 'static + FromStr,
    {
        let arg = self.args[index];
        match arg.parse() {
            Ok(arg) => Ok(arg),
            Err(_) => Err(CommandError::ParseArgError {
                arg,
                type_name: any::type_name::<T>(),
            }),
        }
    }

    pub fn current_buffer_view_handle(&self) -> Result<BufferViewHandle, CommandError<'command>> {
        match self
            .client_handle
            .and_then(|h| self.clients.get(h))
            .and_then(Client::buffer_view_handle)
        {
            Some(handle) => Ok(handle),
            None => Err(CommandError::NoBufferOpened),
        }
    }

    pub fn current_buffer_handle(&self) -> Result<BufferHandle, CommandError<'command>> {
        let buffer_view_handle = self.current_buffer_view_handle()?;
        match self
            .editor
            .buffer_views
            .get(buffer_view_handle)
            .map(|v| v.buffer_handle)
        {
            Some(handle) => Ok(handle),
            None => Err(CommandError::NoBufferOpened),
        }
    }

    pub fn assert_can_discard_all_buffers(&self) -> Result<(), CommandError<'command>> {
        if self.bang || !self.editor.buffers.iter().any(Buffer::needs_save) {
            Ok(())
        } else {
            Err(CommandError::UnsavedChanges)
        }
    }

    pub fn assert_can_discard_buffer(
        &self,
        handle: BufferHandle,
    ) -> Result<(), CommandError<'command>> {
        let buffer = self
            .editor
            .buffers
            .get(handle)
            .ok_or(CommandError::InvalidBufferHandle(handle))?;
        if self.bang || !buffer.needs_save() {
            Ok(())
        } else {
            Err(CommandError::UnsavedChanges)
        }
    }
}

pub struct CommandIter<'a>(&'a str);
impl<'a> CommandIter<'a> {
    pub fn new(commands: &'a str) -> Self {
        CommandIter(commands)
    }
}
impl<'a> Iterator for CommandIter<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.0 = self.0.trim_start();
            if self.0.is_empty() {
                return None;
            }

            let bytes = self.0.as_bytes();
            let mut i = 0;
            loop {
                if i == bytes.len() {
                    let command = self.0;
                    self.0 = "";
                    return Some(command);
                }

                match bytes[i] {
                    b'\n' => {
                        let (command, rest) = self.0.split_at(i);
                        self.0 = rest;
                        if command.is_empty() {
                            break;
                        } else {
                            return Some(command);
                        }
                    }
                    b'\\' => i += 1,
                    b'#' => {
                        let command = &self.0[..i];
                        while i < bytes.len() && bytes[i] != b'\n' {
                            i += 1;
                        }
                        self.0 = &self.0[i..];
                        if command.is_empty() {
                            break;
                        } else {
                            return Some(command);
                        }
                    }
                    _ => (),
                }
                i += 1;
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum CommandTokenKind {
    Text,
    Bang,
    Unterminated,
}
pub struct CommandTokenIter<'a> {
    pub rest: &'a str,
}
impl<'a> Iterator for CommandTokenIter<'a> {
    type Item = (CommandTokenKind, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        fn next_token(mut rest: &str) -> Option<(CommandTokenKind, &str, &str)> {
            rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\\');
            if rest.is_empty() {
                return None;
            }

            match rest.as_bytes()[0] {
                delim @ b'"' | delim @ b'\'' => {
                    rest = &rest[1..];
                    match rest.find(delim as char) {
                        Some(i) => Some((CommandTokenKind::Text, &rest[..i], &rest[(i + 1)..])),
                        None => Some((CommandTokenKind::Unterminated, rest, "")),
                    }
                }
                b'[' => {
                    rest = &rest[1..];
                    match rest.find(']') {
                        Some(i) => Some((CommandTokenKind::Text, &rest[..i], &rest[(i + 1)..])),
                        None => Some((CommandTokenKind::Unterminated, rest, "")),
                    }
                }
                b'!' => {
                    let (token, rest) = rest.split_at(1);
                    Some((CommandTokenKind::Bang, token, rest))
                }
                _ => match rest
                    .find(|c: char| c.is_ascii_whitespace() || matches!(c, '!' | '"' | '\'' | '['))
                {
                    Some(i) => {
                        let (token, rest) = rest.split_at(i);
                        Some((CommandTokenKind::Text, token, rest))
                    }
                    None => Some((CommandTokenKind::Text, rest, "")),
                },
            }
        }

        match next_token(self.rest) {
            Some((kind, token, rest)) => {
                self.rest = rest;
                Some((kind, token))
            }
            None => None,
        }
    }
}

pub enum CommandSource {
    Builtin(usize),
}

pub struct BuiltinCommand {
    pub names: &'static [&'static str],
    pub description: &'static str,
    pub bang_usage: Option<&'static str>,
    pub params: &'static [(&'static str, Option<CompletionSource>)],
    pub func: CommandFn,
}

pub struct CommandManager {
    builtin_commands: &'static [BuiltinCommand],
    history: VecDeque<String>,
}

impl CommandManager {
    pub fn new() -> Self {
        Self {
            builtin_commands: builtin::COMMANDS,
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }

    pub fn find_command(&self, name: &str) -> Option<CommandSource> {
        match self
            .builtin_commands
            .iter()
            .position(|c| c.names.contains(&name))
        {
            Some(i) => Some(CommandSource::Builtin(i)),
            None => None,
        }
    }

    pub fn builtin_commands(&self) -> &[BuiltinCommand] {
        &self.builtin_commands
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn history_entry(&self, index: usize) -> &str {
        match self.history.get(index) {
            Some(e) => e.as_str(),
            None => "",
        }
    }

    pub fn add_to_history(&mut self, entry: &str) {
        if entry.is_empty() {
            return;
        }

        let mut s = if self.history.len() == self.history.capacity() {
            self.history.pop_front().unwrap()
        } else {
            String::new()
        };

        s.clear();
        s.push_str(entry);
        self.history.push_back(s);
    }

    pub fn eval_command<'command>(
        editor: &mut Editor,
        platform: &mut Platform,
        clients: &mut ClientManager,
        client_handle: Option<ClientHandle>,
        command: &'command str,
        output: &mut String,
    ) -> Result<Option<CommandOperation>, CommandError<'command>> {
        match editor.commands.parse(command) {
            Ok((source, bang, args)) => {
                let command = match source {
                    CommandSource::Builtin(i) => editor.commands.builtin_commands[i].func,
                };
                let mut ctx = CommandContext {
                    editor,
                    platform,
                    clients,
                    client_handle,
                    bang,
                    args,
                    output,
                };
                command(&mut ctx)
            }
            Err(error) => Err(CommandError::ParseError(error)),
        }
    }

    fn parse<'a>(
        &self,
        text: &'a str,
    ) -> Result<(CommandSource, bool, [&'a str; PARAMETERS_CAPACITY]), CommandParseError<'a>> {
        let mut arg_count = 0;
        let mut args = [""; PARAMETERS_CAPACITY];
        let mut tokens = CommandTokenIter { rest: text };
        let mut peeked_token = None;

        let command_name = match tokens.next() {
            Some((CommandTokenKind::Text, s)) => s,
            Some((_, s)) => return Err(CommandParseError::InvalidCommandName(s)),
            None => return Err(CommandParseError::InvalidCommandName(text.trim_start())),
        };

        let bang = match tokens.next() {
            Some((CommandTokenKind::Bang, _)) => true,
            token => {
                peeked_token = token;
                false
            }
        };

        let source = match self.find_command(command_name) {
            Some(source) => source,
            None => return Err(CommandParseError::CommandNotFound(command_name)),
        };
        let param_count = match source {
            CommandSource::Builtin(i) => {
                let command = &self.builtin_commands[i];
                if bang && command.bang_usage.is_none() {
                    return Err(CommandParseError::CommandDoesNotAcceptBang(command_name));
                }
                command.params.len() as _
            }
        };

        loop {
            let token = match peeked_token.take() {
                Some(token) => token,
                None => match tokens.next() {
                    Some(token) => token,
                    None => break,
                },
            };

            match token {
                (CommandTokenKind::Text, s) => {
                    if arg_count == param_count {
                        return Err(CommandParseError::TooManyArguments(s, param_count));
                    }
                    args[arg_count as usize] = s;
                    arg_count += 1;
                }
                (CommandTokenKind::Bang, s) => return Err(CommandParseError::InvalidArgument(s)),
                (CommandTokenKind::Unterminated, s) => {
                    return Err(CommandParseError::UnterminatedArgument(s))
                }
            }
        }

        if arg_count < param_count {
            let token = if arg_count > 0 {
                args[arg_count as usize - 1]
            } else {
                command_name
            };
            return Err(CommandParseError::TooFewArguments(token, param_count));
        }

        Ok((source, bang, args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_commands() -> CommandManager {
        let builtin_commands = &[
            BuiltinCommand {
                names: &["cmd0"],
                description: "",
                bang_usage: Some(""),
                params: &[],
                func: |_| Ok(None),
            },
            BuiltinCommand {
                names: &["command-name", "c"],
                description: "",
                bang_usage: Some(""),
                params: &[("", None), ("", None), ("", None)],
                func: |_| Ok(None),
            },
        ];

        CommandManager {
            builtin_commands,
            history: Default::default(),
        }
    }

    #[test]
    fn command_parsing() {
        fn assert_bang(commands: &CommandManager, command: &str, expect_bang: bool) {
            let (source, bang, _) = match commands.parse(command) {
                Ok(result) => result,
                Err(_) => panic!("command parse error at '{}'", command),
            };
            assert!(matches!(source, CommandSource::Builtin(0)));
            assert_eq!(expect_bang, bang);
        }

        let commands = create_commands();
        assert_bang(&commands, "cmd0", false);
        assert_bang(&commands, "  cmd0  ", false);
        assert_bang(&commands, "  cmd0!  ", true);
        assert_bang(&commands, "  cmd0!", true);
    }

    #[test]
    fn arg_parsing() {
        fn parse_args<'a>(
            commands: &CommandManager,
            command: &'a str,
        ) -> [&'a str; PARAMETERS_CAPACITY] {
            match commands.parse(command) {
                Ok((_, _, args)) => args,
                Err(_) => panic!("command '{}' parse error", command),
            }
        }

        fn collect<'a>(args: &[&'a str]) -> Vec<&'a str> {
            let mut values = Vec::new();
            for value in args {
                if value.is_empty() {
                    break;
                }
                values.push(*value);
            }
            values
        }

        let commands = create_commands();
        let args = parse_args(&commands, "c  aaa  bbb  ccc  ");
        assert_eq!(["aaa", "bbb", "ccc"], &collect(&args)[..]);
        let args = parse_args(&commands, "c  'aaa'  \"bbb\"  ccc  ");
        assert_eq!(["aaa", "bbb", "ccc"], &collect(&args)[..]);
        let args = parse_args(&commands, "c  \"aaa\"\"bbb\"ccc  ");
        assert_eq!(["aaa", "bbb", "ccc"], &collect(&args)[..]);
        let args = parse_args(&commands, "c  [aaa][bbb]ccc  ");
        assert_eq!(["aaa", "bbb", "ccc"], &collect(&args)[..]);
    }

    #[test]
    fn command_parsing_fail() {
        let commands = create_commands();

        macro_rules! assert_fail {
            ($command:expr, $error_pattern:pat => $value:ident == $expect:expr) => {
                match commands.parse($command) {
                    Ok(_) => panic!("command parsed successfully"),
                    Err($error_pattern) => assert_eq!($expect, $value),
                    Err(_) => panic!("other error occurred"),
                }
            };
        }

        assert_fail!("", CommandParseError::InvalidCommandName(s) => s == "");
        assert_fail!("   ", CommandParseError::InvalidCommandName(s) => s == "");
        assert_fail!(" !", CommandParseError::InvalidCommandName(s) => s == "!");
        assert_fail!("!  'aa'", CommandParseError::InvalidCommandName(s) => s == "!");
        assert_fail!("  a \"aa\"", CommandParseError::CommandNotFound(s) => s == "a");

        assert_fail!("c 0 1 'abc", CommandParseError::UnterminatedArgument(s) => s == "abc");
        assert_fail!("c 0 1 '", CommandParseError::UnterminatedArgument(s) => s == "");
        assert_fail!("c 0 1 \"'", CommandParseError::UnterminatedArgument(s) => s == "'");

        const MAX_VALUES_LEN: u8 = 3;
        let mut too_many_values_command = String::new();
        too_many_values_command.push('c');
        for _ in 0..MAX_VALUES_LEN {
            too_many_values_command.push_str(" a");
        }
        too_many_values_command.push_str(" b");
        assert_fail!(&too_many_values_command, CommandParseError::TooManyArguments(s, MAX_VALUES_LEN) => s == "b");
    }

    #[test]
    fn multi_command_line_parsing() {
        let mut commands = CommandIter::new("command0\ncommand1");
        assert_eq!(Some("command0"), commands.next());
        assert_eq!(Some("command1"), commands.next());
        assert_eq!(None, commands.next());

        let mut commands = CommandIter::new("command0\n\n\ncommand1");
        assert_eq!(Some("command0"), commands.next());
        assert_eq!(Some("command1"), commands.next());
        assert_eq!(None, commands.next());

        let mut commands = CommandIter::new("command0\\\n still command0\ncommand1");
        assert_eq!(Some("command0\\\n still command0"), commands.next());
        assert_eq!(Some("command1"), commands.next());
        assert_eq!(None, commands.next());

        let mut commands = CommandIter::new("   #command0");
        assert_eq!(None, commands.next());

        let mut commands = CommandIter::new("command0 # command1");
        assert_eq!(Some("command0 "), commands.next());
        assert_eq!(None, commands.next());

        let mut commands = CommandIter::new("    # command0\ncommand1");
        assert_eq!(Some("command1"), commands.next());
        assert_eq!(None, commands.next());

        let mut commands =
            CommandIter::new("command0# comment\n\n# more comment\n\n# one more comment\ncommand1");
        assert_eq!(Some("command0"), commands.next());
        assert_eq!(Some("command1"), commands.next());
        assert_eq!(None, commands.next());
    }
}
