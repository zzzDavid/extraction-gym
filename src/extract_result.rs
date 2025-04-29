mod extract;

use extract::*;
use egraph_serialize::*;
use ordered_float::NotNan;
use anyhow::Context;
use env_logger;

pub type Cost = NotNan<f64>;
pub const INFINITY: Cost = unsafe { NotNan::new_unchecked(std::f64::INFINITY) };

fn main() {
    env_logger::init();

    let mut args = pico_args::Arguments::from_env();

    let extractor_name: String = args
        .opt_value_from_str("--extractor")
        .unwrap()
        .unwrap_or_else(|| "faster-greedy-dag".into());

    let filename: String = args.free_from_str().unwrap();

    let rest = args.finish();
    if !rest.is_empty() {
        panic!("Unknown arguments: {:?}", rest);
    }

    let egraph = EGraph::from_json_file(&filename)
        .with_context(|| format!("Failed to parse {filename}"))
        .unwrap();

    let extractor = match extractor_name.as_str() {
        "faster-greedy-dag" => extract::faster_greedy_dag::FasterGreedyDagExtractor.boxed(),
        "faster-bottom-up" => extract::faster_bottom_up::FasterBottomUpExtractor.boxed(),
        "bottom-up" => extract::bottom_up::BottomUpExtractor.boxed(),
        #[cfg(feature = "ilp-cbc")]
        "ilp-cbc-timeout" => extract::ilp_cbc::CbcExtractorWithTimeout::<10>.boxed(),
        _ => panic!("Unknown extractor: {}", extractor_name),
    };

    let result = extractor.extract(&egraph, &egraph.root_eclasses);
    result.check(&egraph);

    // Print the extraction result
    // println!("Extraction Result:");
    // println!("-----------------");
    
    // Build a map from class_id to node_id for easy lookup
    let class_to_node: std::collections::HashMap<ClassId, NodeId> = result.choices.clone().into_iter().collect();
    
    // Map to store variable names for sub-expressions
    let mut expr_vars = std::collections::HashMap::new();
    
    // Function to recursively print assignments for sub-expressions
    fn print_assignments(
        egraph: &EGraph,
        class_to_node: &std::collections::HashMap<ClassId, NodeId>,
        class_id: &ClassId,
        expr_vars: &mut std::collections::HashMap<ClassId, String>,
    ) -> String {
        // Check if we've already processed this class
        if let Some(var_name) = expr_vars.get(class_id) {
            return var_name.clone();
        }
        
        let node_id = match class_to_node.get(class_id) {
            Some(id) => id,
            None => return format!("unknown_{}", class_id),
        };
        
        let node = &egraph[node_id];
        
        // Extract the node name to use as the variable name
        let var_name = if node.children.is_empty() {
            if node.op.starts_with("Var(") && node.op.ends_with(")") {
                // Extract the variable name from Var("name")
                let var_name_inner = &node.op[4..node.op.len()-1];
                // Remove quotes if present
                let var_name_clean = var_name_inner.trim_matches('"');
                var_name_clean.to_string()
            } else {
                // Use node_id as a string for other leaf nodes
                format!("{}", node_id)
            }
        } else {
            // For non-leaf nodes, use node_id as name
            format!("{}", node_id)
        };
        
        // Process children and print assignments
        if node.children.is_empty() {
            // Leaf node - no need to print assignment for variables
            if !node.op.starts_with("Var(") {
                println!("{} = {}", var_name, node.op);
            }
        } else {
            // Process children first to ensure dependencies are handled
            let mut child_vars = Vec::new();
            for child in &node.children {
                let child_class = egraph.nid_to_cid(child);
                let child_var = print_assignments(egraph, class_to_node, child_class, expr_vars);
                child_vars.push(child_var);
            }
            
            // Format the assignment based on operation
            if node.op.starts_with("Add") {
                if child_vars.len() == 2 {
                    println!("{} = {} + {}", var_name, child_vars[0], child_vars[1]);
                } else {
                    let joined = child_vars.join(" + ");
                    println!("{} = {}", var_name, joined);
                }
            } else if node.op.starts_with("Not") {
                if child_vars.len() == 1 {
                    println!("{} = ~{}", var_name, child_vars[0]);
                } else {
                    println!("{} = ~({})", var_name, child_vars.join(", "));
                }
            } else if node.op.starts_with("Or") {
                if child_vars.len() == 2 {
                    println!("{} = {} | {}", var_name, child_vars[0], child_vars[1]);
                } else {
                    let joined = child_vars.join(" | ");
                    println!("{} = {}", var_name, joined);
                }
            } else if node.op.starts_with("And") {
                if child_vars.len() == 2 {
                    println!("{} = {} & {}", var_name, child_vars[0], child_vars[1]);
                } else {
                    let joined = child_vars.join(" & ");
                    println!("{} = {}", var_name, joined);
                }
            } else if node.op.starts_with("Mul") {
                // Check for Mul operation with a number constant
                if node.op.contains("Num(") {
                    // Extract the number from the operation string
                    if let Some(start) = node.op.find("Num(") {
                        if let Some(end) = node.op[start..].find(")") {
                            let num_str = &node.op[start+4..start+end];
                            println!("{} = {} * {}", var_name, child_vars[0], num_str);
                        } else {
                            // Fallback if parsing fails
                            let joined = child_vars.join(" * ");
                            println!("{} = {}", var_name, joined);
                        }
                    } else {
                        // Fallback if parsing fails
                        let joined = child_vars.join(" * ");
                        println!("{} = {}", var_name, joined);
                    }
                } else if child_vars.len() == 2 {
                    println!("{} = {} * {}", var_name, child_vars[0], child_vars[1]);
                } else {
                    let joined = child_vars.join(" * ");
                    println!("{} = {}", var_name, joined);
                }
            } else if node.op.starts_with("Shl") {
                if child_vars.len() == 2 {
                    println!("{} = {} << {}", var_name, child_vars[0], child_vars[1]);
                } else {
                    // Extract the shift amount
                    if let Some(amount_start) = node.op.find(',') {
                        if let Some(end) = node.op[amount_start..].find(")") {
                            let amount = node.op[amount_start+1..amount_start+end].trim();
                            println!("{} = {} << {}", var_name, child_vars[0], amount);
                        } else {
                            println!("{} = {} << 1", var_name, child_vars[0]);
                        }
                    } else {
                        println!("{} = {} << 1", var_name, child_vars[0]);
                    }
                }
            } else if node.op.starts_with("Shr") {
                if child_vars.len() == 2 {
                    println!("{} = {} >> {}", var_name, child_vars[0], child_vars[1]);
                } else {
                    // Extract the shift amount
                    if let Some(amount_start) = node.op.find(',') {
                        if let Some(end) = node.op[amount_start..].find(")") {
                            let amount = node.op[amount_start+1..amount_start+end].trim();
                            println!("{} = {} >> {}", var_name, child_vars[0], amount);
                        } else {
                            println!("{} = {} >> 1", var_name, child_vars[0]);
                        }
                    } else {
                        println!("{} = {} >> 1", var_name, child_vars[0]);
                    }
                }
            } else if node.op.starts_with("RootNode") {
                if let Some(output_name_start) = node.op.find('"') {
                    if let Some(output_name_end) = node.op[output_name_start+1..].find('"') {
                        let output_name = &node.op[output_name_start+1..output_name_start+1+output_name_end];
                        println!("{} = {}", output_name, child_vars[0]);
                    } else {
                        println!("{} = {}", var_name, child_vars[0]);
                    }
                } else {
                    println!("{} = {}", var_name, child_vars[0]);
                }
            } else {
                if child_vars.is_empty() {
                    println!("{} = {}", var_name, node.op);
                } else {
                    println!("{} = {}({})", var_name, node.op, child_vars.join(", "));
                }
            }
        }
        
        // Store the variable name for this expression
        expr_vars.insert(class_id.clone(), var_name.clone());
        var_name
    }
    
    // Print assignments for each root eclass
    for root_class in &egraph.root_eclasses {
        // println!("Root expression assignments:");
        let result_var = print_assignments(&egraph, &class_to_node, root_class, &mut expr_vars);
        // println!("output = {}", result_var);
    }

    // Print costs
    let tree = result.tree_cost(&egraph, &egraph.root_eclasses);
    let dag = result.dag_cost(&egraph, &egraph.root_eclasses);
    // println!("\nTree cost: {}", tree);
    // println!("DAG cost: {}", dag);
}