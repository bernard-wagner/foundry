mod router;

use alloy_primitives::{Address, B256};
use clap::{Parser, Subcommand};
use eyre::Result;
use foundry_cli::opts::{CompilerArgs, CoreBuildArgs, ProjectPathsArgs};
use foundry_common::{
    compile::{ProjectCompiler, SkipBuildFilter, SkipBuildFilters},
    fs,
};
use foundry_compilers::artifacts::output_selection::ContractOutputSelection;
use std::path::Path;
use yansi::Paint;

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

    /// Scaffolds router file for given contracts
    Router(Box<GenerateRouterArgs>),
}

#[derive(Debug, Parser)]
pub struct GenerateTestArgs {
    /// Contract name for test generation.
    #[arg(long, short, value_name = "CONTRACT_NAME")]
    pub contract_name: String,
}

#[derive(Debug, Parser)]
pub struct GenerateRouterArgs {
    /// Router name for router generation.
    #[clap(long, value_name = "ROUTER_NAME")]
    pub name: String,

    #[clap(long, default_value = "0x4e59b44847b379578588920ca78fbf26c0b4956c")]
    deployer: Address,

    #[clap(
        long,
        default_value = "0x0000000000000000000000000000000000000000000000000000000000000000"
    )]
    salt: B256,

    /// Contract names for router generation.
    pub module_names: Vec<String>,

    #[clap(flatten)]
    pub project_paths: ProjectPathsArgs,
}

impl GenerateTestArgs {
    pub fn run(self) -> Result<()> {
        let contract_name = format_identifier(&self.contract_name, true);
        let instance_name = format_identifier(&self.contract_name, false);

        // Create the test file content.
        let test_content = include_str!("../../../assets/generated/TestTemplate.t.sol");
        let test_content = test_content
            .replace("{contract_name}", &contract_name)
            .replace("{instance_name}", &instance_name);

        // Create the test directory if it doesn't exist.
        fs::create_dir_all("test")?;

        // Define the test file path
        let test_file_path = Path::new("test").join(format!("{}.t.sol", contract_name));

        // Write the test content to the test file.
        fs::write(&test_file_path, test_content)?;

        println!("{} test file: {}", "Generated".green(), test_file_path.to_str().unwrap());
        Ok(())
    }
}

impl GenerateRouterArgs {
    pub fn run(self) -> Result<()> {
        let GenerateRouterArgs { deployer, name: router_name, module_names, salt, project_paths } =
            self;

        let build_args = CoreBuildArgs {
            project_paths: project_paths.clone(),
            compiler: CompilerArgs {
                extra_output: vec![ContractOutputSelection::Abi],
                ..Default::default()
            },
            ..Default::default()
        };

        let project = build_args.project()?;

        let output_dir = project.sources_path().as_path().to_path_buf().join("generated/routers");

        let filter = SkipBuildFilters::new(
            [
                SkipBuildFilter::Tests,
                SkipBuildFilter::Custom(format!("{}/**.sol", output_dir.to_str().unwrap())),
            ],
            project.root().clone(),
        )?;

        ProjectCompiler::new().filter(Box::new(filter)).quiet(true).compile(&project)?;

        let output =
            router::build_router(&project, router_name.clone(), module_names, deployer, salt)?;

        let output_dir = Path::new(&output_dir);
        fs::create_dir_all(output_dir)?;

        let router_file_path = output_dir.join(format!("{}.g.sol", router_name));
        fs::write(&router_file_path, output)?;
        println!(
            "{} router file: {}",
            Paint::green("Generated"),
            router_file_path.to_str().unwrap()
        );

        Ok(())
    }
}

/// Utility function to convert an identifier to pascal or camel case.
fn format_identifier(input: &str, is_pascal_case: bool) -> String {
    let mut result = String::new();
    let mut capitalize_next = is_pascal_case;

    for word in input.split_whitespace() {
        if !word.is_empty() {
            let (first, rest) = word.split_at(1);
            let formatted_word = if capitalize_next {
                format!("{}{}", first.to_uppercase(), rest)
            } else {
                format!("{}{}", first.to_lowercase(), rest)
            };
            capitalize_next = true;
            result.push_str(&formatted_word);
        }
    }
    result
}
