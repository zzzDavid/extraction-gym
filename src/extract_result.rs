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
    println!("Extraction Result:");
    println!("-----------------");
    
    // First, build a map from class_id to node_id for easy lookup
    let mut class_to_node: std::collections::HashMap<ClassId, NodeId> = result.choices.clone().into_iter().collect();
    
    // Function to recursively print the expression
    fn print_expr(egraph: &EGraph, class_to_node: &std::collections::HashMap<ClassId, NodeId>, class_id: &ClassId, indent: usize) -> String {
        let node_id = match class_to_node.get(class_id) {
            Some(id) => id,
            None => return format!("UnknownClass({})", class_id),
        };
        
        let node = &egraph[node_id];
        
        if node.children.is_empty() {
            // Leaf node
            format!("{}", node.op)
        } else {
            // Internal node
            let mut result = format!("({}", node.op);
            for child in &node.children {
                // Get the class ID from the egraph using the child node ID
                let child_class = egraph.nid_to_cid(child);
                result.push_str(&format!(" {}", print_expr(egraph, class_to_node, child_class, indent + 2)));
            }
            result.push(')');
            result
        }
    }
    
    // Print the S-expression for each root eclass
    for root_class in &egraph.root_eclasses {
        println!("Root expression:");
        println!("{}", print_expr(&egraph, &class_to_node, root_class, 0));
    }

    // Print costs
    let tree = result.tree_cost(&egraph, &egraph.root_eclasses);
    let dag = result.dag_cost(&egraph, &egraph.root_eclasses);
    println!("\nTree cost: {}", tree);
    println!("DAG cost: {}", dag);
} 