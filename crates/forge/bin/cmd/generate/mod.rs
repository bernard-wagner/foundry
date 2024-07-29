use clap::{Parser, Subcommand};

use crate::cmd::generate::test::GenerateTestArgs;
use crate::cmd::generate::router::GenerateRouterArgs;

pub mod test;
pub mod router;

/// CLI arguments for `forge generate`.
#[derive(Debug, Parser)]
pub struct GenerateArgs {
    #[command(subcommand)]
    pub sub: GenerateSubcommands,
}

#[derive(Debug, Subcommand)]
pub enum GenerateSubcommands {
    /// Scaffolds test file for given contract.
    Test(GenerateTestArgs),

    /// Generate ERCXXX router.
    Router(Box<GenerateRouterArgs>),
}