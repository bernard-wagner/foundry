use crate::cmd::generate::FunctionSelector;

/// Utility function to convert an identifier to constant case.
pub fn to_constant_case(name: &str) -> String {
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

/// Utility function to convert an identifier to pascal or camel case.
pub(crate) fn format_identifier(input: &str, is_pascal_case: bool) -> String {
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

#[derive(Debug, Clone)]
struct BinaryData {
    selectors: Vec<FunctionSelector>,
    children: Vec<BinaryData>,
}
pub(crate) fn build_binary_data(selectors: Vec<FunctionSelector>) -> BinaryData {
    const MAX_SELECTORS_PER_SWITCH_STATEMENT: usize = 9;

    fn binary_split(node: &mut BinaryData) {
        if node.selectors.len() > MAX_SELECTORS_PER_SWITCH_STATEMENT {
            let mid_idx = (node.selectors.len() + 1) / 2;

            let mut child_a = BinaryData {
                selectors: node.selectors.drain(..mid_idx).collect(),
                children: Vec::new(),
            };

            let mut child_b = BinaryData {
                selectors: node.selectors.drain(..).collect(),
                children: Vec::new(),
            };

            binary_split(&mut child_a);
            binary_split(&mut child_b);

            node.children.push(child_a);
            node.children.push(child_b);
        }
    }

    let mut root = BinaryData {
        selectors,
        children: Vec::new(),
    };

    binary_split(&mut root);

    root
}
fn repeat_string(s: &str, count: usize) -> String {
    (0..count).map(|_| s).collect()
}

pub(crate) fn render_selectors(mut binary_data: BinaryData) -> String {
    let mut selectors_str: Vec<String> = Vec::new();

    fn render_node(node: &mut BinaryData, indent: usize, selectors_str: &mut Vec<String>) {
        if !node.children.is_empty() {
            let mut child_a = node.children.remove(0);
            let mut child_b = node.children.remove(0);

            fn find_mid_selector(node: &mut BinaryData) -> &FunctionSelector {
                if !node.selectors.is_empty() {
                    &node.selectors[0]
                } else {
                    find_mid_selector(&mut node.children[0])
                }
            }

            let mid_selector = find_mid_selector(&mut child_b);

            selectors_str.push(format!(
                "{}if lt(sig,{}) {{",
                repeat_string("    ", 4 + indent),
                mid_selector.selector
            ));
            render_node(&mut child_a, indent + 1, selectors_str);
            selectors_str.push(format!("{}}}", repeat_string("    ", 4 + indent)));

            render_node(&mut child_b, indent, selectors_str);
        } else {
            selectors_str.push(format!("{}switch sig", repeat_string("    ", 4 + indent)));
            for s in &node.selectors {
                selectors_str.push(format!(
                    "{}case {} {{ result := {} }} // {}.{}()",
                    repeat_string("    ", 4 + indent),
                    s.selector,
                    to_constant_case(&s.contract_name),
                    s.contract_name,
                    s.name
                ));
            }
            selectors_str.push(format!("{}leave", repeat_string("    ", 4 + indent)));
        }
    }

    render_node(&mut binary_data, 0, &mut selectors_str);

    selectors_str.join("\n")
}

pub fn render_modules(modules: Vec<FunctionSelector>) -> String {
    let mut modules_str: Vec<String> = Vec::new();

    for FunctionSelector{address, contract_name, .. } in modules {
        modules_str.push(format!("address constant {} = {};", contract_name, address.to_checksum(None)));
    }

    modules_str.join("\n")
}