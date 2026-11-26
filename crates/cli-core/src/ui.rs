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
