use std::collections::BTreeMap;

use alloy_json_abi::{Function, JsonAbi};
use alloy_primitives::{Address, B256};
use eyre::Result;
use foundry_compilers::{artifacts::CompactContractBytecode, info::ContractInfo, Project};
use hex::ToHexExt;
use itertools::Itertools;

use crate::cmd::generate::format_identifier;

#[derive(Debug, Clone)]
pub struct RouterTemplateInputs {
    address: Address,
    contract_name: String,
    function_name: String,
    selector: String,
}

#[derive(Debug, Clone)]
struct BinaryData {
    selectors: Vec<RouterTemplateInputs>,
    children: Vec<BinaryData>,
}

pub(crate) fn build_router(
    project: &Project,
    router_name: String,
    module_names: Vec<String>,
    deployer: Address,
    salt: B256,
) -> Result<String> {
    let router_name = format_identifier(&router_name, true);

    let cache = project.read_cache_file()?;
    let cached_artifacts = cache.read_artifacts::<CompactContractBytecode>()?;

    let mut combined_abi = JsonAbi::new();
    let mut functions = BTreeMap::<String, Function>::new();
    let mut selectors = Vec::new();

    for module_name in module_names.iter() {
        let ContractInfo { name: module_name, path: module_path } = ContractInfo::new(module_name);

        let cached_artifact = module_path
            .and_then(|path| cached_artifacts.find(path, module_name.clone()))
            .or(cached_artifacts.find_first(module_name.clone()))
            .ok_or_else(|| eyre::eyre!("No cached artifact found for contract `{module_name}`"))?;

        let bytecode = cached_artifact
            .bytecode
            .as_ref()
            .and_then(|b| b.bytes())
            .ok_or_else(|| eyre::eyre!("No bytecode found for contract `{module_name}`"))?;

        // calculate create2 address
        let address = Address::create2_from_code(&deployer, salt, bytecode);

        let abi = cached_artifact
            .abi
            .as_ref()
            .ok_or_else(|| eyre::eyre!("No ABI found for contract `{module_name}`"))?;

        for function_set in abi.functions.iter() {
            for function in function_set.1.iter() {
                let selector: String = function.selector().encode_hex_with_prefix();

                if functions.contains_key(&selector) {
                    return Err(eyre::eyre!("Duplicate selector found"));
                }

                functions.insert(selector.clone(), function.clone());

                if let Some(f) = combined_abi.functions.get_mut(&function.name) {
                    f.push(function.clone());
                } else {
                    combined_abi.functions.insert(function.name.clone(), vec![function.clone()]);
                };

                selectors.push(RouterTemplateInputs {
                    address,
                    contract_name: module_name.clone(),
                    function_name: function.name.clone(),
                    selector,
                });
            }
        }

        if abi.fallback.is_some() {
            if combined_abi.fallback.is_some() {
                return Err(eyre::eyre!("Multiple fallback functions found"));
            }
            combined_abi.fallback = abi.fallback;
        }
        if abi.receive.is_some() {
            if combined_abi.receive.is_some() {
                return Err(eyre::eyre!("Multiple receive functions found"));
            }
            combined_abi.receive = abi.receive;
        }
    }

    for (_, function) in functions.iter() {
        combined_abi.functions.insert(function.name.clone(), vec![function.clone()]);
    }

    let interface = combined_abi.to_sol(format!("I{}", router_name).as_str(), None);

    let router_tree = build_binary_data(selectors.clone());
    let module_lookup = render_modules(selectors.clone());
    //let functions = render_interface(selectors.clone());

    let selectors = render_selectors(router_tree);

    // Create the router file content.
    let router_content = include_str!("../../../assets/generated/RouterTemplate.t.sol");
    let router_content = router_content
        .replace("{selectors}", &selectors)
        .replace("{interface}", &interface)
        .replace("{router_name}", &router_name)
        .replace("{modules}", &module_lookup);

    // Create the router directory if it doesn't exist.

    Ok(router_content)
}

fn build_binary_data(selectors: Vec<RouterTemplateInputs>) -> BinaryData {
    const MAX_SELECTORS_PER_SWITCH_STATEMENT: usize = 9;

    fn binary_split(node: &mut BinaryData) {
        if node.selectors.len() > MAX_SELECTORS_PER_SWITCH_STATEMENT {
            let mid_idx = (node.selectors.len() + 1) / 2;

            let mut child_a = BinaryData {
                selectors: node.selectors.drain(..mid_idx).collect(),
                children: Vec::new(),
            };

            let mut child_b =
                BinaryData { selectors: node.selectors.drain(..).collect(), children: Vec::new() };

            binary_split(&mut child_a);
            binary_split(&mut child_b);

            node.children.push(child_a);
            node.children.push(child_b);
        }
    }

    let mut root = BinaryData { selectors, children: Vec::new() };

    binary_split(&mut root);

    root
}

fn repeat_string(s: &str, count: usize) -> String {
    (0..count).map(|_| s).collect()
}

fn render_selectors(mut binary_data: BinaryData) -> String {
    let mut selectors_str: Vec<String> = Vec::new();

    fn render_node(node: &mut BinaryData, indent: usize, selectors_str: &mut Vec<String>) {
        if !node.children.is_empty() {
            let mut child_a = node.children.remove(0);
            let mut child_b = node.children.remove(0);

            fn find_mid_selector(node: &mut BinaryData) -> &RouterTemplateInputs {
                if !node.selectors.is_empty() {
                    &node.selectors[0]
                } else {
                    find_mid_selector(&mut node.children[0])
                }
            }

            let mid_selector = find_mid_selector(&mut child_b);

            selectors_str.push(format!(
                "{}if lt(sig, {}) {{",
                repeat_string("    ", indent),
                mid_selector.selector
            ));
            render_node(&mut child_a, indent + 1, selectors_str);
            selectors_str.push(format!("{}}}", repeat_string("    ", indent)));

            render_node(&mut child_b, indent + 2, selectors_str);
        } else {
            selectors_str.push(format!("{}switch sig", repeat_string("    ", indent)));
            for s in &node.selectors {
                selectors_str.push(format!(
                    "{}case {} {{ result := {} }} // {}.{}()",
                    repeat_string("    ", indent + 1),
                    s.selector,
                    to_constant_case(&s.contract_name),
                    s.contract_name,
                    s.function_name
                ));
            }
            selectors_str.push(format!("{}leave", repeat_string("    ", indent)));
        }
    }

    render_node(&mut binary_data, 4, &mut selectors_str);

    selectors_str.join("\n")
}

fn render_modules(modules: Vec<RouterTemplateInputs>) -> String {
    let mut modules_str: Vec<String> = Vec::new();

    let modules = modules
        .clone()
        .into_iter()
        .unique_by(|m| m.contract_name.clone())
        .collect::<Vec<RouterTemplateInputs>>();

    for RouterTemplateInputs { address, contract_name, .. } in modules {
        modules_str.push(format!(
            "address constant {} = {};",
            to_constant_case(&contract_name),
            address.to_checksum(None)
        ));
    }

    modules_str.join("\n")
}

/// Utility function to convert an identifier to constant case.
fn to_constant_case(name: &str) -> String {
    let mut result = String::new();
    let mut prev_is_uppercase = false;

    for c in name.chars() {
        if c.is_uppercase() {
            if !prev_is_uppercase {
                result.push('_');
            }
            prev_is_uppercase = true;
        } else {
            prev_is_uppercase = false;
        }

        result.push(c);
    }

    result.to_uppercase()
}
