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
    for (class_id, node_id) in &result.choices {
        let node = &egraph[node_id];
        println!("Class {} -> Node {} (op: {}, cost: {})", 
            class_id, 
            node_id,
            node.op,
            node.cost
        );
        if !node.children.is_empty() {
            println!("  Children: {:?}", node.children);
        }
    }

    // Print costs
    let tree = result.tree_cost(&egraph, &egraph.root_eclasses);
    let dag = result.dag_cost(&egraph, &egraph.root_eclasses);
    println!("\nTree cost: {}", tree);
    println!("DAG cost: {}", dag);
} 