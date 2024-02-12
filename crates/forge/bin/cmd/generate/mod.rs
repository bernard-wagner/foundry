mod utils;

use clap::{Parser, Subcommand};
use eyre::Result;
use foundry_common::fs;
use std::path::Path;
use alloy_primitives::{Address, U256};
use ethers_core::abi::AbiEncode;
use foundry_compilers::artifacts::output_selection::ContractOutputSelection;
use foundry_compilers::info::{ContractInfo, ContractInfoRef};
use itertools::Itertools;
use yansi::Paint;
use foundry_cli::opts::{CompilerArgs, CoreBuildArgs, ProjectPathsArgs};
use foundry_common::compile::ProjectCompiler;

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
    Router(GenerateRouterArgs),
}

#[derive(Debug, Parser)]
pub struct GenerateTestArgs {
    /// Contract name for test generation.
    #[arg(long, short, value_name = "CONTRACT_NAME")]
    pub contract_name: String,
}

#[derive(Debug, Parser)]
pub struct GenerateRouterArgs {
    #[clap(flatten)]
    pub project_paths: ProjectPathsArgs,

    /// Router name for router generation.
    #[clap(long, short, value_name = "ROUTER_NAME")]
    pub name: String,

    #[clap(long, default_value = "0x4e59b44847b379578588920ca78fbf26c0b4956c")]
    deployer: Address,

    #[clap(long, default_value = "0x00")]
    salt: U256,

    /// Contract names for router generation.
    pub contract_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionSelector {
    address: Address,
    contract_name: String,
    name: String,
    selector: String,
}

impl GenerateTestArgs {
    pub fn run(self) -> Result<()> {
        let contract_name = utils::format_identifier(&self.contract_name, true);
        let instance_name = utils::format_identifier(&self.contract_name, false);

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
        let GenerateRouterArgs {
            deployer,
            name,
            contract_names,
            salt,
            project_paths,
        } = self;

        let router_name = utils::format_identifier(&name, true);

        let build_args = CoreBuildArgs {
            project_paths: project_paths.clone(),
            compiler: CompilerArgs {
                extra_output: vec![ContractOutputSelection::Abi],
                ..Default::default()
            },
            ..Default::default()
        };

        let project = build_args.project()?;

        let output = ProjectCompiler::new().quiet(true).compile(&project)?;

        let artifacts = contract_names.iter().flat_map(|identifier| {
            let ContractInfoRef{ path, name } = ContractInfo::new(identifier).into();

            let found_artifact = if let Some(path) = path {
                output.find(path, name.clone())
            } else {
                output.find_first(name.clone())
            };

            let artifact = found_artifact
                .ok_or_else(|| {
                    eyre::eyre!(
                        "Could not find artifact `{name}` in the compiled artifacts"
                    )
                })?
                .clone();

            // calculate create2 address
            let address = Address::create2_from_code(&deployer, salt.to_be_bytes(), artifact.bytecode.clone().ok_or_else(|| {
                eyre::eyre!("No bytecode found for contract `{name}`")
            })?.bytes().ok_or_else(|| {
                eyre::eyre!("No bytecode found for contract `{name}`")
            })?);

            let result = artifact
                .abi
                .ok_or_else(|| {
                    eyre::eyre!("No ABI found for contract `{name}`")
                })?
                .functions
                .into_values()
                .flatten()
                .map(|function| FunctionSelector {
                    address,
                    contract_name: name.into(),
                    name: function.clone().name,
                    selector: function.selector().encode_hex(),
                })
                .collect::<Vec<FunctionSelector>>();

            Ok(result)
        }).flatten().sorted_by(|a, b| a.selector.cmp(&b.selector))
            .collect::<Vec<_>>();

        let router_tree = utils::build_binary_data(artifacts);
        let module_lookup = utils::render_modules(artifacts);

        let selectors = utils::render_selectors(router_tree);

        // Create the router file content.
        let router_content = include_str!("../../../assets/generated/RouterTemplate.t.sol");
        let router_content = router_content
            .replace("{selectors", &selectors)
            .replace("{router_name}", &router_name)
            .replace("{module_names}", &contract_names.join(", "));

        // Create the router directory if it doesn't exist.
        fs::create_dir_all("router")?;

        // Define the router file path
        let router_file_path = Path::new("router").join(format!("{}.t.sol", router_name));

        // Write the router content to the router file.
        fs::write(&router_file_path, router_content)?;

        println!("{} router file: {}", Paint::green("Generated"), router_file_path.to_str().unwrap());
        Ok(())
    }
}
