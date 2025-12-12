#[path = "../src/model_download.rs"]
mod model_download;

use anyhow::Result;
use model_download::{default_model_path, ensure_model_available};
use std::path::PathBuf;

use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::ValueType,
};

fn main() -> Result<()> {
    env_logger::init();

    let model_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_model_path);

    println!("Loading model: {}", model_path.display());
    ensure_model_available(&model_path)?;
    print_model_info(&model_path)?;

    Ok(())
}

fn print_model_info(model_path: &PathBuf) -> Result<()> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(2)?
        .commit_from_file(model_path)?;

    println!("Inputs:");
    for (idx, input) in session.inputs.iter().enumerate() {
        println!(
            "  {}: name=\"{}\" type={:?}",
            idx, input.name, input.input_type
        );
        if let ValueType::Tensor { shape, .. } = &input.input_type {
            println!("     shape={:?}", shape);
        }
    }

    println!("Outputs:");
    for (idx, output) in session.outputs.iter().enumerate() {
        println!(
            "  {}: name=\"{}\" type={:?}",
            idx, output.name, output.output_type
        );
        if let ValueType::Tensor { shape, .. } = &output.output_type {
            println!("     shape={:?}", shape);
        }
    }

    Ok(())
}
