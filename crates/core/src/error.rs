//! Agent control flow and error types.
//!
//! `AgentError` serves dual purpose: genuine errors AND structured control
//! flow signals (submission, limits, interruption). The stream pipeline's
//! `.take_while()` combinator inspects `is_terminal()` to decide when to stop.

use thiserror::Error;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

/// Exit status recorded in the trajectory when an agent run ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitStatus {
    Submitted,
    LimitsExceeded,
    UserInterruption,
    FormatError,
    ModelError,
    EnvironmentError,
    Uncaught,
}

impl std::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Submitted          => "submitted",
            Self::LimitsExceeded     => "limits_exceeded",
            Self::UserInterruption   => "user_interruption",
            Self::FormatError        => "format_error",
            Self::ModelError         => "model_error",
            Self::EnvironmentError   => "environment_error",
            Self::Uncaught           => "uncaught",
        };
        write!(f, "{s}")
    }
}

/// Central error / control-flow type.
///
/// Variants carrying a `submission` represent *successful* terminal states.
/// Others are genuine errors. The stream pipeline's `.take_while()` uses
/// `is_terminal()` to stop the loop.
#[derive(Debug, Error)]
pub enum AgentError {
    /// Agent completed and produced output. Happy path.
    #[error("submitted: {submission}")]
    Submitted { submission: String },

    /// Step or cost ceiling hit.
    #[error("limits exceeded: steps={steps}, cost=${cost:.4}")]
    LimitsExceeded { steps: u32, cost: f64 },

    /// User explicitly interrupted (interactive mode).
    #[error("user interruption: {message}")]
    UserInterruption { message: String },

    /// User injected a new task mid-run (interactive mode).
    #[error("user new task: {task}")]
    UserNewTask { task: String },

    /// Model response was not parseable into a `ToolCall`.
    #[error("format error: {message}")]
    FormatError { message: String },

    /// Unrecoverable model API error after retries exhausted.
    #[error("model error: {message}")]
    ModelError { message: String },

    /// Tool / environment execution failure.
    #[error("environment error: {message}")]
    EnvironmentError { message: String },

    /// JSON (de)serialization failure at the LLM boundary.
    #[error("serialization error: {source}")]
    Serialization {
        #[from]
        source: serde_json::Error,
    },

    /// Anything that doesn't fit the above.
    #[error("internal error: {message}")]
    Internal { message: String },
}

impl AgentError {
    /// True for variants that represent a clean terminal state.
    /// The stream pipeline calls `.take_while(|e| !e.is_terminal())`.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Submitted { .. }
                | Self::LimitsExceeded { .. }
                | Self::UserInterruption { .. }
        )
    }

    /// Map to `ExitStatus` for trajectory serialization.
    pub fn exit_status(&self) -> ExitStatus {
        match self {
            Self::Submitted { .. }            => ExitStatus::Submitted,
            Self::LimitsExceeded { .. }       => ExitStatus::LimitsExceeded,
            Self::UserInterruption { .. }
            | Self::UserNewTask { .. }        => ExitStatus::UserInterruption,
            Self::FormatError { .. }          => ExitStatus::FormatError,
            Self::ModelError { .. }           => ExitStatus::ModelError,
            Self::EnvironmentError { .. }     => ExitStatus::EnvironmentError,
            _                                 => ExitStatus::Uncaught,
        }
    }
}
