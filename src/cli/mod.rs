pub mod args;
pub mod generate_docs;
pub mod output;
pub mod style;

/// How a failed command should be reported.
pub struct ErrorReport {
    /// The message, exactly as the CLI prints it to stderr (minus ANSI styling).
    pub message: String,
    /// The process exit code the CLI uses for this class of failure.
    pub exit_code: i32,
}

/// Classify a command failure into the message and exit code the CLI uses.
///
/// Shared by both frontends so they cannot drift: the CLI prints the message
/// and exits with the code, the MCP bridge puts the message in a tool error.
///
/// The codes let a caller tell a bad invocation (2) and a missing `ty`
/// capability (3) apart from a clean "not found", which is a success.
pub fn classify_error(error: &anyhow::Error) -> ErrorReport {
    if let Some(usage) = error.downcast_ref::<crate::commands::UsageError>() {
        return ErrorReport { message: usage.0.clone(), exit_code: 2 };
    }
    #[cfg(unix)]
    if let Some(unsupported) = error.downcast_ref::<crate::daemon::protocol::UnsupportedByTy>() {
        return ErrorReport { message: unsupported.to_string(), exit_code: 3 };
    }
    ErrorReport { message: format!("Error: {}", format_error_chain(error)), exit_code: 1 }
}

/// Format the full anyhow error chain for display.
pub fn format_error_chain(error: &anyhow::Error) -> String {
    use std::fmt::Write as _;

    let mut chain = error.chain();
    let mut msg = chain.next().expect("error chain is never empty").to_string();
    for cause in chain {
        let _ = write!(msg, "\n  Caused by: {cause}");
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::{classify_error, format_error_chain};
    use crate::commands::UsageError;

    #[test]
    fn single_error_has_no_cause_lines() {
        assert_eq!(format_error_chain(&anyhow::anyhow!("boom")), "boom");
    }

    #[test]
    fn usage_errors_get_their_own_exit_code_and_verbatim_message() {
        let report = classify_error(&anyhow::Error::from(UsageError("tyf: nope".into())));
        assert_eq!(report.message, "tyf: nope");
        assert_eq!(report.exit_code, 2);
    }

    #[test]
    fn runtime_errors_get_the_error_prefix_and_exit_one() {
        let report = classify_error(&anyhow::anyhow!("inner").context("outer"));
        assert_eq!(report.message, "Error: outer\n  Caused by: inner");
        assert_eq!(report.exit_code, 1);
    }

    #[test]
    fn nested_causes_are_indented_under_the_head() {
        let error = anyhow::anyhow!("inner").context("middle").context("outer");
        assert_eq!(format_error_chain(&error), "outer\n  Caused by: middle\n  Caused by: inner");
    }
}
