use std::fmt;

use pepper::{
    command::CommandError,
    serialization::{DeserializeError, Deserializer, Serialize, Serializer},
};

// Remedybg's protocol
// https://gist.github.com/x13pixels/d1f0f99b108f34b6c461be9f9249e8e8

pub enum ProtocolError {
    DeserializeError(DeserializeError),
    RemedybgCommandResult(RemedybgCommandResult),
    CommandError(CommandError),
}
impl From<DeserializeError> for ProtocolError {
    fn from(other: DeserializeError) -> Self {
        Self::DeserializeError(other)
    }
}
impl From<CommandError> for ProtocolError {
    fn from(other: CommandError) -> Self {
        Self::CommandError(other)
    }
}
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::DeserializeError(DeserializeError::InvalidData) => {
                f.write_str("deserialize error: invalid data")
            }
            Self::DeserializeError(DeserializeError::InsufficientData) => {
                f.write_str("deserialize error: insufficient data")
            }
            Self::RemedybgCommandResult(result) => {
                f.write_fmt(format_args!("remedybg result: {}", result))
            }
            Self::CommandError(error) => error.fmt(f),
        }
    }
}

pub enum RemedybgCommandResult {
    Unknown,

    Ok,

    // Generic failure
    Fail,

    // Result if the command is aborted due to a specified behavior and
    // condition including RDBG_IF_DEBUGGING_TARGET_ABORT_COMMAND or
    // RDBG_IF_SESSION_IS_MODIFIED_ABORT_COMMAND. The result can also be returned
    // if an unnamed session is saved, prompts for a filename, and the user
    // cancels this operation.
    Aborted,

    // Result if the given command buffer given is less than 2 bytes or if the
    // command is not one of the enumerated commands in rdbg_Command.
    InvalidCommand,

    // Result if the response generated is too large to fit in the buffer.
    BufferTooSmall,

    // Result if an opening a file (i.e., a session, text file).
    FailedOpeningFile,

    // Result if saving a session fails.
    FailedSavingSession,

    // Result if the given ID is invalid.
    InvalidId,

    // Result if a command expects the target to be in a particular state (not
    // debugging, debugging and suspended, or debugging and executing) and is
    // not.
    InvalidTargetState,

    // Result if an active configuration does not exist
    NoActiveConfig,

    // Result if the command does not apply to given breakpoint's kind
    InvalidBreakpointKind,
}
impl fmt::Display for RemedybgCommandResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Unknown => "unknown",
            Self::Ok => "ok",
            Self::Fail => "fail",
            Self::Aborted => "aborted",
            Self::InvalidCommand => "invalid command",
            Self::BufferTooSmall => "buffer too small",
            Self::FailedOpeningFile => "failed opening file",
            Self::FailedSavingSession => "failed saving session",
            Self::InvalidId => "invalid id",
            Self::InvalidTargetState => "invalid target state",
            Self::NoActiveConfig => "no active config",
            Self::InvalidBreakpointKind => "invalid breakpoint kind",
        };
        write!(f, "{}", name)
    }
}
impl RemedybgCommandResult {
    pub fn deserialize(deserializer: &mut dyn Deserializer) -> Result<Self, DeserializeError> {
        let discriminant = u16::deserialize(deserializer)?;
        match discriminant {
            0 => Ok(RemedybgCommandResult::Unknown),
            1 => Ok(RemedybgCommandResult::Ok),
            2 => Ok(RemedybgCommandResult::Fail),
            3 => Ok(RemedybgCommandResult::Aborted),
            4 => Ok(RemedybgCommandResult::InvalidCommand),
            5 => Ok(RemedybgCommandResult::BufferTooSmall),
            6 => Ok(RemedybgCommandResult::FailedOpeningFile),
            7 => Ok(RemedybgCommandResult::FailedSavingSession),
            8 => Ok(RemedybgCommandResult::InvalidId),
            9 => Ok(RemedybgCommandResult::InvalidTargetState),
            10 => Ok(RemedybgCommandResult::NoActiveConfig),
            11 => Ok(RemedybgCommandResult::InvalidBreakpointKind),
            _ => Err(DeserializeError::InvalidData),
        }
    }
}

#[derive(Clone, Copy)]
pub struct RemedybgBool(pub bool);
impl<'de> Serialize<'de> for RemedybgBool {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        let b = self.0 as u8;
        b.serialize(serializer);
    }

    fn deserialize(deserializer: &mut dyn Deserializer<'de>) -> Result<Self, DeserializeError> {
        let b = u8::deserialize(deserializer)?;
        Ok(Self(b != 0))
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RemedybgId(pub u32);
impl<'de> Serialize<'de> for RemedybgId {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        self.0.serialize(serializer);
    }

    fn deserialize(deserializer: &mut dyn Deserializer<'de>) -> Result<Self, DeserializeError> {
        match Serialize::deserialize(deserializer)? {
            0 => Err(DeserializeError::InvalidData),
            id => Ok(Self(id)),
        }
    }
}

pub fn deserialize_remedybg_bytes<'de>(
    deserializer: &mut dyn Deserializer<'de>,
) -> Result<&'de [u8], DeserializeError> {
    let len = u16::deserialize(deserializer)?;
    let bytes = deserializer.read(len as _)?;
    Ok(bytes)
}

#[derive(Clone, Copy)]
pub struct RemedybgStr<'a>(pub &'a [u8]);
impl<'de> Serialize<'de> for RemedybgStr<'de> {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        let len = self.0.len() as u16;
        len.serialize(serializer);
        serializer.write(self.0);
    }

    fn deserialize(deserializer: &mut dyn Deserializer<'de>) -> Result<Self, DeserializeError> {
        let len = u16::deserialize(deserializer)?;
        let bytes = deserializer.read(len as _)?;
        Ok(Self(bytes))
    }
}

pub enum RemedybgProcessorBreakpointAccessKind {
    Write,
    ReadWrite,
    Execute,
}

pub enum RemedybgProtocolBreakpoint<'a> {
    FunctionName {
        function_name: RemedybgStr<'a>,
        overload_id: u32,
    },
    FilenameLine {
        filename: RemedybgStr<'a>,
        line_num: u32,
    },
    Address {
        address: u64,
    },
    Processor {
        addr_expression: RemedybgStr<'a>,
        num_bytes: u8,
        access_kind: RemedybgProcessorBreakpointAccessKind,
    },
}
impl<'a> RemedybgProtocolBreakpoint<'a> {
    pub fn deserialize(deserializer: &mut dyn Deserializer<'a>) -> Result<Self, DeserializeError> {
        match u8::deserialize(deserializer)? {
            1 => {
                let function_name = Serialize::deserialize(deserializer)?;
                let overload_id = Serialize::deserialize(deserializer)?;
                Ok(Self::FunctionName {
                    function_name,
                    overload_id,
                })
            }
            2 => {
                let filename = Serialize::deserialize(deserializer)?;
                let line_num = Serialize::deserialize(deserializer)?;
                Ok(Self::FilenameLine { filename, line_num })
            }
            3 => {
                let address = Serialize::deserialize(deserializer)?;
                Ok(Self::Address { address })
            }
            4 => {
                let addr_expression = Serialize::deserialize(deserializer)?;
                let num_bytes = Serialize::deserialize(deserializer)?;
                let access_kind = match u8::deserialize(deserializer)? {
                    1 => RemedybgProcessorBreakpointAccessKind::Write,
                    2 => RemedybgProcessorBreakpointAccessKind::ReadWrite,
                    3 => RemedybgProcessorBreakpointAccessKind::Execute,
                    _ => return Err(DeserializeError::InvalidData),
                };
                Ok(Self::Processor {
                    addr_expression,
                    num_bytes,
                    access_kind,
                })
            }
            _ => Err(DeserializeError::InvalidData),
        }
    }
}

pub const RDBG_IF_DEBUGGING_TARGET_STOP_DEBUGGING: u8 = 1;
//pub const RDBG_IF_DEBUGGING_TARGET_ABORT_COMMAND: u8 = 2;

#[derive(Clone, Copy)]
pub enum RemedybgCommandKind {
    // Bring the RemedyBG window to the foreground and activate it. No additional
    // arguments follow the command. Returns RDBG_COMMAND_RESULT_OK or
    // RDBG_COMMAND_RESULT_FAIL.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //BringDebuggerToForeground = 50,

    // Exit the RemedyBG application.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [dtb :: rdbg_DebuggingTargetBehavior (uint8_t)]
    // [msb :: rdbg_ModifiedSessionBehavior (uint8_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //ExitDebugger = 75,

    //
    // Session

    // Returns whether the current session is modified, or "dirty".
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [modified :: rdbg_Bool (uint8_t)]
    //GetIsSessionModified = 100,
    // Returns the current session's filename. If the filename has not been set
    // for the session then the result will be
    // RDBG_COMMAND_RESULT_UNNAMED_SESSION and the length of |filename| will be
    // zero.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [filename :: rdbg_String]
    //GetSessionFilename = 101,

    // Creates a new session. All configurations are cleared and reset.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [dtb :: rdbg_DebuggingTargetBehavior (uint8_t)]
    // [msb :: rdbg_ModifiedSessionBehavior (uint8_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //NewSession = 102,

    // Open a session with the given filename.
    //
    // [command :: rdbg_Command (uint16_t)]
    // [dtb :: rdbg_DebuggingTargetBehavior (uint8_t)]
    // [msb :: rdbg_ModifiedSessionBehavior (uint8_t)]
    // [filename :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //OpenSession = 103,

    // Save session with its current filename. If the filename is has not been
    // specified for the session the user will be prompted. To save with a
    // filename see RDBG_COMMAND_SAVE_AS_SESSION, instead.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //SaveSession = 104,

    // Save session with a given filename.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [filename :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //SaveAsSession = 105,

    // Retrieve a list of configurations for the current session.
    //
    // [cmd :: rdbg_Command (uint16_t)
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [num_configs :: uint16_t]
    // .FOR(num_configs) {
    //   [uid :: rdbg_Id (uint32_t)]
    //   [command :: rdbg_String]
    //   [command_args :: rdbg_String]
    //   [working_dir :: rdbg_String]
    //   [environment_vars :: rdbg_String]
    //   [inherit_environment_vars_from_parent :: rdbg_Bool]
    //   [break_at_nominal_entry_point :: rdbg_Bool]
    //   [name :: rdbg_String]
    // }
    //GetSessionConfigs = 106,

    // Add a new session configuration to the current session. All string
    // parameters accept zero length strings. Multiple environment variables
    // should be newline, '\n', separated. Returns the a unique ID for the
    // configuration.
    //
    // Note that 'name' is currently optional.
    //
    // [cmd :: rdbg_Command (uint16_t)
    // [command :: rdbg_String]
    // [command_args :: rdbg_String]
    // [working_dir :: rdbg_String]
    // [environment_vars :: rdbg_String]
    // [inherit_environment_vars_from_parent :: rdbg_Bool]
    // [break_at_nominal_entry_point :: rdbg_Bool]
    // [name :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [uid :: rdbg_Id]
    //AddSessionConfig = 107,

    // Sets the active configuration for a session by configuration ID. If the
    // ID is not valid for the current session
    // RDBG_COMMAND_RESULT_INVALID_ID is returned.
    //
    // [cmd :: rdbg_Command (uint16_t)
    // [id  :: rdbg_Id]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //SetActiveSessionConfig = 108,

    // Deletes a session configuration by ID. If the ID is not valid for the
    // current session RDBG_COMMAND_REMOVE_SESSION_CONFIG is returned.
    //
    // [cmd :: rdbg_Command (uint16_t)
    // [id  :: rdbg_Id]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //DeleteSessionConfig = 109,

    // Deletes all session configurations in the current session.
    //
    // [cmd :: rdbg_Command (uint16_t)
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //DeleteAllSessionConfig = 110,

    // Source Files
    //

    // Opens the given file, if not already opened, and navigates to the
    // specified line number. The line number is optional and can be elided from
    // the command buffer. Returns result along with an ID for the file.
    //
    // [cmd :: rdbg_Command (uint16_t)
    // [filename :: rdbg_String]
    // [line_num :: uint32_t]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [id :: rdbg_Id]
    //GotoFileAtLine = 200,

    // Close the file with the given ID.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [id :: rdbg_Id]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //CloseFile = 201,

    // Close all open files
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //CloseAllFiles = 202,

    // Returns the current file. If no file is open, returns a zero ID,
    // zero-length filename, and zero line number.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [id :: rdbg_Id]
    // [filename :: rdbg_String]
    // [line_num :: uint32_t]
    //GetCurrentFile = 203,

    // Retrieve a list of open files.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [num_files :: uint16_t]
    // .FOR(num_files) {
    //   [id :: rdbg_Id]
    //   [filename :: rdbg_String]
    //   [line_num :: uint32_t]
    // }
    //GetOpenFiles = 204,

    //
    // Debugger Control

    // Returns the target state for the current session.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [staste :: rdbg_TargetState (uint16_t)]
    //GetTargetState = 300,

    // If the target is stopped, i.e., not currently being debugged, then start
    // debugging the active configuration. Setting break_at_entry to true will
    // stop execution at the at entry point specified in the configuration:
    // either the nominal entry point, such as "main" or "WinMain" or the entry
    // point function as described in the PE header. If the target is already
    // being debugged, this will return RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [break_at_entry_point :: rdbg_Bool (uint8_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    StartDebugging = 301,

    // Stop debugging the target. If the target is not executing this will return
    // RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    StopDebugging = 302,

    // Restart debugging if the target is being debugging (either suspended or
    // executing) and the target was not attached to a process. Otherwise,
    // returns RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //RestartDebugging = 303,

    // Attach to a process by the given process-id. The value of
    // |continue_execution| indicates whether the process should resume execution
    // after attached.  The debugger target behavior specifies what should happen
    // in the case when the target is being debugged (suspended or executing).
    // Can return: RDBG_COMMAND_RESULT_OK, RDBG_COMMAND_RESULT_FAIL, or
    // RDBG_COMMAND_RESULT_ABORT.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [process_id :: uint32_t]
    // [continue_execution :: rdbg_Bool]
    // [dtb :: rdbg_DebuggingTargetBehavior (uint8_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    AttachToProcessByPid = 304,

    // Attach to a process by the given name. The first process found, in the
    // case there are more than one with the same name, is used. The value of
    // |continue_execution| indicates whether the process should resume execution
    // after attached.  The debugger target behavior specifies what should happen
    // in the case when the target is being debugged (suspended or executing).
    // Can return: RDBG_COMMAND_RESULT_OK, RDBG_COMMAND_RESULT_FAIL, or
    // RDBG_COMMAND_RESULT_ABORT.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [process_name :: rdbg_String]
    // [continue_execution :: rdbg_Bool]
    // [dtb :: rdbg_DebuggingTargetBehavior (uint8_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    AttachToProcessByName = 305,

    // Detach from a target that is being debugged. Can return
    // RDBG_COMMAND_RESULT_OK or RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //DetachFromProcess = 306,

    // With the target suspended, step into by line. If a function call occurs,
    // this command will enter the function.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    StepIntoByLine = 307,

    // With the target suspended, step into by instruction. If a function call
    // occurs, this command will enter the function. Can return
    // RDBG_COMMAND_RESULT_OK or RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //StepIntoByInstruction = 308,

    // With the target suspended, step into by line. If a function call occurs,
    // this command step over that function and not enter it. Can return
    // return RDBG_COMMAND_RESULT_OK or RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    StepOverByLine = 309,

    // With the target suspended, step into by instruction. If a function call
    // occurs, this command will step over that function and not enter it. Can
    // return RDBG_COMMAND_RESULT_OK or RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //StepOverByInstruction = 310,

    // With the target suspended, continue running to the call site of the
    // current function, i.e., step out.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    StepOut = 311,

    // With the target suspended, continue execution. Can return
    // RDBG_COMMAND_RESULT_OK or RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //ContinueExecution
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    ContinueExecution = 312,

    // When the target is not being debugged or is suspended, run to the given
    // filename and line number.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [filename :: rdbg_String]
    // [line_num :: uint32_t]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    RunToFileAtLine = 313,

    // Halt the execution of a target that is in the executing state. Can return
    // RDBG_COMMAND_RESULT_OK or RDBG_COMMAND_RESULT_INVALID_TARGET_STATE.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    BreakExecution = 314,

    //
    // Breakpoints

    // Return the current list of breakpoints. These are the user requested
    // breakpoints. Resolved breakpoint locations, if any, for a requested
    // breakpoint can be obtained using RDBG_COMMAND_GET_BREAKPOINT_LOCATIONS.
    //
    //  * Presently, module name is not used and will always be a zero length
    //  string.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [num_bps :: uint16_t]
    // .FOR(num_bps) {
    //   [uid :: rdbg_Id]
    //   [enabled :: rdbg_Bool]
    //   [module_name :: rdbg_String]
    //   [condition_expr :: rdbg_String]
    //   [kind :: rdbg_BreakpointKind (uint8_t)]
    //   .SWITCH(kind) {
    //     .CASE(BreakpointKind_FunctionName):
    //       [function_name :: rdbg_String]
    //       [overload_id :: uint32_t]
    //     .CASE(BreakpointKind_FilenameLine):
    //       [filename :: rdbg_String]
    //       [line_num :: uint32_t]
    //     .CASE(BreakpointKind_Address):
    //       [address :: uint64_t]
    //     .CASE(BreakpointKind_Processor):
    //       [addr_expression :: rdbg_String]
    //       [num_bytes :: uint8_t]
    //       [access_kind :: rdbg_ProcessorBreakpointAccessKind (uint8_t)]
    //   }
    // }
    GetBreakpoints = 600,

    // Return the list of resolved locations for a particular breakpoint. If the
    // ID is not valid for the current session RDBG_COMMAND_RESULT_INVALID_ID is
    // returned.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [bp_id :: rdbg_Id]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [num_locs :: uint16_t]
    // .FOR(num_locs) {
    //   [address :: uint64_t]
    //   [module_name :: rdbg_String]
    //   [filename :: rdbg_String]
    //   [actual_line_num :: uint32_t]
    // }
    //GetBreakpointLocations = 601,

    // Return a list of function overloads for the given function name. If the
    // target is being debugged (suspended or executing) then returns a list of
    // function overloads for the given function name, otherwise
    // RDBG_COMMAND_RESULT_INVALID_TARGET_STATE is returned. Note that,
    // presently, all modules are searched for the given function.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [function_name :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [num_overloads :: uint8_t]
    // .FOR(num_overloads) {
    //   [overload_id :: rdbg_Id]
    //   [signature :: rdbg_String]
    // }
    //GetFunctionOverloads = 602,

    // Request a breakpoint at the given function name and overload. Pass an
    // overload ID of zero to add requested breakpoints for all functions with
    // the given name.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [function_name :: rdbg_String]
    // [overload_id :: rdbg_Id]
    // [condition_expr :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [bp_id :: rdbg_Id]
    //AddBreakpointAtFunction = 603,

    // Request a breakpoint at the given source file and line number.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [filename :: rdbg_String]
    // [line_num :: uint32_t]
    // [condition_expr :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [bp_id :: rdbg_Id]
    AddBreakpointAtFilenameLine = 604,

    // Request a breakpoint at the given address.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [address :: uint64_t]
    // [condition_expr :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [bp_id :: rdbg_Id]
    //AddBreakpointAtAddress = 605,

    // Add a processor (hardware) breakpoint.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [addr_expression :: rdbg_String]
    // [num_bytes :: uint8_t]
    // [access_kind :: rdbg_ProcessorBreakpointAccessKind (uint8_t)]
    // [condition_expr :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [bp_id :: rdbg_Id]
    //AddProcessorBreakpoint = 606,

    // Sets the conditional expression for the given breakpoint. Can pass in a
    // zero-length string for none.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [bp_id :: rdbg_Id]
    // [condition_expr :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //SetBreakpointCondition = 607,

    // Given an existing breakpoint of type RDBG_BREAKPOINT_KIND_FILENAME_LINE,
    // update its line number to the given one-based value.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [bp_id :: rdbg_Id]
    // [line_num :: uint32_t]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    UpdateBreakpointLine = 608,

    // Enable or disable an existing breakpoint.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [bp_id :: rdbg_Id]
    // [enable :: rdbg_Bool]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //EnableBreakpoint = 609,

    // Delete an existing breakpoint.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [bp_id :: rdbg_Id]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    DeleteBreakpoint = 610,

    // Delete all existing breakpoints.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //DeleteAllBreakpoints = 611,

    // Return information about a specific user requested breakpoint.
    //
    //  * Presently, module name is not used and will always be a zero length
    //  string.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [bp_id :: rdbg_Id]
    // =>
    // [uid :: rdbg_Id]
    // [enabled :: rdbg_Bool]
    // [module_name :: rdbg_String]
    // [condition_expr :: rdbg_String]
    // [kind :: rdbg_BreakpointKind (uint8_t)]
    // .SWITCH(kind) {
    //   .CASE(BreakpointKind_FunctionName):
    //     [function_name :: rdbg_String]
    //     [overload_id :: uint32_t]
    //   .CASE(BreakpointKind_FilenameLine):
    //     [filename :: rdbg_String]
    //     [line_num :: uint32_t]
    //   .CASE(BreakpointKind_Address):
    //     [address :: uint64_t]
    //   .CASE(BreakpointKind_Processor):
    //     [addr_expression :: rdbg_String]
    //     [num_bytes :: uint8_t]
    //     [access_kind :: rdbg_ProcessorBreakpointAccessKind (uint8_t)]
    // }
    GetBreakpoint = 612,
    //
    // Watch Window Expressions

    // Return a list of watch expressions for the given, one-based watch window,
    // presently ranging in [1,8].
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [window_num :: uint8_t]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [num_watches :: uint16_t]
    // .FOR(num_watches) {
    //   [uid :: rdbg_Id]
    //   [expr :: rdbg_String]
    //   [comment :: rdbg_String]
    // }
    //GetWatches = 700,

    // Add a watch expresion to the given, one-based watch window. Presently,
    // only single line comments are supported. Spaces will replace any newlines
    // found in a comment.
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [window_num :: uint8_t]
    // [expr :: rdbg_String]
    // [comment :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    // [uid :: rdbg_Id]
    //AddWatch = 701,

    // Updates the expression for a given watch
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [uid :: rdbg_Id]
    // [expr :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //UpdateWatchExpression = 702,

    // Updates the comment for a given watch
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [uid :: rdbg_Id]
    // [comment :: rdbg_String]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //UpdateWatchComment = 703,

    // Delete the given watch
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [uid :: rdbg_Id]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //DeleteWatch = 704,

    // Delete all watches in the given watch window
    //
    // [cmd :: rdbg_Command (uint16_t)]
    // [window_num :: uint8_t]
    // ->
    // [result :: rdbg_CommandResult (uint16_t)]
    //DeleteAllWatches = 705,
}
impl RemedybgCommandKind {
    pub fn serialize(self, serializer: &mut dyn Serializer) {
        let discriminant = self as u16;
        discriminant.serialize(serializer);
    }
}

#[derive(Clone, Copy)]
pub enum RemedybgSourceLocationChangedReason {
    Unspecified = 0,
    ByCommandLine = 1, // An open-file from the command-line updated the source location
    ByDriver = 2, // A RDBG_COMMAND_GOTO_FILE_AT_LINE from a named-pipes driver updated the source location
    BreakpointSelected = 3, // A selection of a breakpoint in breakpoints pane updated the source location
    CurrentFrameChanged = 4, // The current stack frame was changed in the callstack pane and updated the source location
    ActiveThreadChanged = 5, // The active thread was changed in the threads pane and updated the source location

    // The process was suspended and updated the source location
    BreakpointHit = 6,
    ExceptionHit = 7,
    StepOver = 8,
    StepIn = 9,
    StepOut = 10,
    NonUserBreakpoint = 11,
    DebugBreak = 12,
}
impl RemedybgSourceLocationChangedReason {
    pub fn deserialize(deserializer: &mut dyn Deserializer) -> Result<Self, DeserializeError> {
        let discriminant = u16::deserialize(deserializer)?;
        match discriminant {
            0 => Ok(Self::Unspecified),
            1 => Ok(Self::ByCommandLine),
            2 => Ok(Self::ByDriver),
            3 => Ok(Self::BreakpointSelected),
            4 => Ok(Self::CurrentFrameChanged),
            5 => Ok(Self::ActiveThreadChanged),
            6 => Ok(Self::BreakpointHit),
            7 => Ok(Self::ExceptionHit),
            8 => Ok(Self::StepOver),
            9 => Ok(Self::StepIn),
            10 => Ok(Self::StepOut),
            11 => Ok(Self::NonUserBreakpoint),
            12 => Ok(Self::DebugBreak),
            _ => Err(DeserializeError::InvalidData),
        }
    }
}

pub enum RemedybgEvent<'a> {
    // A target being debugged has exited.
    //
    // [kind :: rdbg_DebugEventKind (uint16_t)]
    // [exit_code :: uint32_t]
    ExitProcess {
        exit_code: u32,
    },

    // The source location changed due to an event in the debugger.
    //
    // [kind :: rdbg_DebugEventKind (uint16_t)]
    // [filename :: rdbg_String]
    // [line_num :: uint32_t]
    // [reason :: rdbg_SourceLocChangedReason (uint16_t) ]
    SourceLocationChanged {
        filename: RemedybgStr<'a>,
        line_num: u32,
        reason: RemedybgSourceLocationChangedReason,
    },

    // A user breakpoint was hit
    //
    // [kind :: rdbg_DebugEventKind (uint16_t)]
    // [bp_id :: rdbg_Id]
    BreakpointHit {
        breakpoint_id: RemedybgId,
    },

    // The breakpoint with the given ID has been resolved (has a valid location).
    // This can happen if the breakpoint was set in module that became loaded,
    // for instance.
    //
    // [kind :: rdbg_DebugEventKind (uint16_t)]
    // [bp_id :: rdbg_Id]
    BreakpointResolved {
        breakpoint_id: RemedybgId,
    },

    // A new user breakpoint was added.
    //
    // [kind :: rdbg_DebugEventKind (uint16_t)]
    // [bp_id :: rdbg_Id]
    BreakpointAdded {
        breakpoint_id: RemedybgId,
    },

    // A user breakpoint was modified.
    //
    // [kind :: rdbg_DebugEventKind (uint16_t)]
    // [bp_id :: rdbg_Id]
    BreakpointModified {
        breakpoint_id: RemedybgId,
    },

    // A user breakpoint was removed.
    //
    // [kind :: rdbg_DebugEventKind (uint16_t)]
    // [bp_id :: rdbg_Id]
    BreakpointRemoved {
        breakpoint_id: RemedybgId,
    },

    // An OutputDebugString was received by the debugger. The given string will
    // be UTF-8 encoded.
    //
    // [kind :: rdbg_DebugEventKind (uint16_t)]
    // [str :: rdbg_String]
    OutputDebugString {
        string: RemedybgStr<'a>,
    },
}
impl<'a> RemedybgEvent<'a> {
    pub fn deserialize(deserializer: &mut dyn Deserializer<'a>) -> Result<Self, DeserializeError> {
        let discriminant = u16::deserialize(deserializer)?;
        match discriminant {
            100 => {
                let exit_code = Serialize::deserialize(deserializer)?;
                Ok(Self::ExitProcess { exit_code })
            }
            200 => {
                let filename = Serialize::deserialize(deserializer)?;
                let line_num = Serialize::deserialize(deserializer)?;
                let reason = RemedybgSourceLocationChangedReason::deserialize(deserializer)?;
                Ok(Self::SourceLocationChanged {
                    filename,
                    line_num,
                    reason,
                })
            }
            600 => {
                let breakpoint_id = Serialize::deserialize(deserializer)?;
                Ok(Self::BreakpointHit { breakpoint_id })
            }
            601 => {
                let breakpoint_id = Serialize::deserialize(deserializer)?;
                Ok(Self::BreakpointResolved { breakpoint_id })
            }
            602 => {
                let breakpoint_id = Serialize::deserialize(deserializer)?;
                Ok(Self::BreakpointAdded { breakpoint_id })
            }
            603 => {
                let breakpoint_id = Serialize::deserialize(deserializer)?;
                Ok(Self::BreakpointModified { breakpoint_id })
            }
            604 => {
                let breakpoint_id = Serialize::deserialize(deserializer)?;
                Ok(Self::BreakpointRemoved { breakpoint_id })
            }
            800 => {
                let string = Serialize::deserialize(deserializer)?;
                Ok(Self::OutputDebugString { string })
            }
            _ => Err(DeserializeError::InvalidData),
        }
    }
}
impl<'a> fmt::Display for RemedybgEvent<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ExitProcess { .. } => f.write_str("exit process"),
            Self::SourceLocationChanged { .. } => f.write_str("source location changed"),
            Self::BreakpointHit { .. } => f.write_str("breakpoint hit"),
            Self::BreakpointResolved { .. } => f.write_str("breakpoint resolved"),
            Self::BreakpointAdded { .. } => f.write_str("breakpoint added"),
            Self::BreakpointModified { .. } => f.write_str("breakpoint modified"),
            Self::BreakpointRemoved { .. } => f.write_str("breakpoint removed"),
            Self::OutputDebugString { .. } => f.write_str("output debug string"),
        }
    }
}
