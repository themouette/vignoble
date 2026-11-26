use anyhow::Result;
use cli_core::update::{self, UpdateArgs};

pub const VERSION: &str = env!("BRANCHES_STATUS_VERSION");
pub const BIN_NAME: &str = env!("CARGO_PKG_NAME");

pub fn execute(args: &UpdateArgs) -> Result<()> {
    update::execute(BIN_NAME, VERSION, args)?;
    Ok(())
}
