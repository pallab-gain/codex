use codex_protocol::exec_output::ExecToolCallOutput;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnifiedExecError {
    #[error("Failed to create unified exec process: {message}")]
    CreateProcess { message: String },
    #[error("Unified exec process failed: {message}")]
    ProcessFailed { message: String },
    // The model is trained on `session_id`, but internally we track a `process_id`.
    #[error("Unknown process id {process_id}")]
    UnknownProcessId { process_id: i32 },
    #[error("failed to write to stdin")]
    WriteToStdin,
    #[error(
        "stdin is closed for this session; rerun exec_command with tty=true to keep stdin open"
    )]
    StdinClosed,
    #[error("background terminal {process_id} is currently attached by a user")]
    ProcessAttachedByUser { process_id: i32 },
    #[error("background terminal {process_id} is not attached by owner {owner_id}")]
    ProcessNotAttachedByOwner { process_id: i32, owner_id: String },
    #[error("background terminal {process_id} does not support PTY resize")]
    ResizeUnsupported { process_id: i32 },
    #[error("missing command line for unified exec request")]
    MissingCommandLine,
    #[error("Command denied by sandbox: {message}")]
    SandboxDenied {
        message: String,
        output: ExecToolCallOutput,
    },
}

impl UnifiedExecError {
    pub(crate) fn create_process(message: String) -> Self {
        Self::CreateProcess { message }
    }

    pub(crate) fn process_failed(message: String) -> Self {
        Self::ProcessFailed { message }
    }

    pub(crate) fn sandbox_denied(message: String, output: ExecToolCallOutput) -> Self {
        Self::SandboxDenied { message, output }
    }
}
