use crate::commands::ExecutionContext;
use anyhow::Result;

pub fn execute(ctx: &ExecutionContext, target_stage: &str) -> Result<()> {
    let stage = ctx.require_stage(target_stage)?;
    for entry in &stage.branches {
        println!("{}", entry.branch);
    }
    Ok(())
}
