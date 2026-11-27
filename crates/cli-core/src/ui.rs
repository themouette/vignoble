use console::Style;
use std::io::{self, Write};

pub const UPSTREAM_ONLY_SYMBOL: &str = "* ";
pub const UP_TO_DATE_SYMBOL: &str = "✔ ";
pub const NEED_PUSH_SYMBOL: &str = "⬆ ";
pub const NEED_PULL_SYMBOL: &str = "⬇ ";
pub const DIVERGED_SYMBOL: &str = "⤮ ";
pub const UNKNOWN_UPSTREAM_SYMBOL: &str = "? ";
pub const GREEN_TICK_SYMBOL: &str = "✔ ";
pub const RED_CROSS_SYMBOL: &str = "✘ ";
pub const AGENT_WORKING_SYMBOL: &str = "🤖";
pub const AGENT_WAITING_SYMBOL: &str = "💬";

pub fn sync_symbols_help_section() -> String {
    format!(
        "Meaning of the symbols:\n\
         \x20 * {}: upstream only, (there might be a local branch as well)\n\
         \x20 * {}: the branch is up to date with upstream\n\
         \x20 * {}: upstream is behind local branch, you need to push\n\
         \x20 * {}: local branch is behind upstream, you need to pull\n\
         \x20 * {}: local branch and upstream have diverged\n\
         \x20 * {}: local only, there is no known upstream",
        color_str(UPSTREAM_ONLY_SYMBOL, "green"),
        color_str(UP_TO_DATE_SYMBOL, "green"),
        color_str(NEED_PUSH_SYMBOL, "orange"),
        color_str(NEED_PULL_SYMBOL, "orange"),
        color_str(DIVERGED_SYMBOL, "red"),
        color_str(UNKNOWN_UPSTREAM_SYMBOL, "red"),
    )
}

pub fn agent_symbols_help_section() -> String {
    format!(
        "Agent harness icons (set via `vignoble plugin install`):\n\
         \x20 * {} : coding agent is actively working in the worktree\n\
         \x20 * {} : coding agent is idle or waiting for user input",
        AGENT_WORKING_SYMBOL, AGENT_WAITING_SYMBOL,
    )
}

pub fn color_str(text: &str, color_name: &str) -> String {
    let style = match color_name {
        "red" => Style::new().red(),
        "green" => Style::new().green(),
        "orange" | "yellow" => Style::new().yellow(),
        "blue" => Style::new().blue(),
        "magenta" | "purple" => Style::new().magenta(),
        "cyan" => Style::new().cyan(),
        "grey" | "gray" => Style::new().dim(),
        "bold" => Style::new().bold(),
        _ => Style::new(),
    };
    style.apply_to(text).to_string()
}

/// Prompt the user with `<question> (yes/no/quit)` on stdout.
/// Returns `Ok(true)` for yes (or empty input), `Ok(false)` for no, or exits 0 on quit.
pub fn prompt_yes_no(question: &str) -> io::Result<bool> {
    loop {
        print!("{} (yes/no/quit) ", question);
        io::stdout().flush()?;

        let mut input = String::new();
        let bytes = io::stdin().read_line(&mut input)?;
        if bytes == 0 {
            return Ok(false);
        }

        let choice = input.trim().to_lowercase();
        match choice.as_str() {
            "" | "y" | "ye" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            "q" | "qu" | "qui" | "quit" => std::process::exit(0),
            _ => {
                println!("Please respond with 'yes', 'no' or 'quit'");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_symbols_help_section_includes_all_sync_symbols() {
        let section = sync_symbols_help_section();
        assert!(section.starts_with("Meaning of the symbols:"));
        for sym in [
            UPSTREAM_ONLY_SYMBOL.trim(),
            UP_TO_DATE_SYMBOL.trim(),
            NEED_PUSH_SYMBOL.trim(),
            NEED_PULL_SYMBOL.trim(),
            DIVERGED_SYMBOL.trim(),
            UNKNOWN_UPSTREAM_SYMBOL.trim(),
        ] {
            assert!(section.contains(sym), "missing symbol {}", sym);
        }
    }

    #[test]
    fn test_agent_symbols_help_section_includes_agent_icons() {
        let section = agent_symbols_help_section();
        assert!(section.starts_with("Agent harness icons"));
        assert!(section.contains(AGENT_WORKING_SYMBOL));
        assert!(section.contains(AGENT_WAITING_SYMBOL));
    }
}
