pub mod args;
pub mod generate_docs;
pub mod output;
pub mod style;

/// Format the full anyhow error chain for display.
///
/// Shared by both frontends: the CLI prints it to stderr, the MCP bridge puts
/// it in the tool error's text content.
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
    use super::format_error_chain;

    #[test]
    fn single_error_has_no_cause_lines() {
        assert_eq!(format_error_chain(&anyhow::anyhow!("boom")), "boom");
    }

    #[test]
    fn nested_causes_are_indented_under_the_head() {
        let error = anyhow::anyhow!("inner").context("middle").context("outer");
        assert_eq!(format_error_chain(&error), "outer\n  Caused by: middle\n  Caused by: inner");
    }
}
