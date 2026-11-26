use crate::{commands::ExecutionContext, printer::pretty_print_block};
use anyhow::Result;
use cli_core::ui::color_str;

pub fn execute(ctx: &ExecutionContext) -> Result<()> {
    let mut dev_branch_count = 0usize;

    for stage in &ctx.stages {
        let title = if let Some(reference) = &stage.reference {
            format!(
                "{} {}",
                color_str(&stage.label, &stage.color),
                color_str(&format!("({})", reference), "grey")
            )
        } else {
            if stage.name == "dev" {
                dev_branch_count = stage.branches.len();
            }
            color_str(&stage.label, &stage.color)
        };

        pretty_print_block(
            &title,
            &stage.branches,
            &stage.color,
            &ctx.branches_info,
            &ctx.worktrees_by_branch,
        );
    }

    let mut count_str = color_str(&dev_branch_count.to_string(), "bold");
    if dev_branch_count > 20 {
        count_str = color_str(&count_str, "red");
    } else if dev_branch_count > 10 {
        count_str = color_str(&count_str, "orange");
    }

    println!(
        "You have {} {} in active development",
        count_str,
        if dev_branch_count > 1 {
            "branches"
        } else {
            "branch"
        }
    );

    Ok(())
}
