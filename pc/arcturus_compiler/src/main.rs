//! Arcturus AI Model Compiler CLI

use anyhow::Result;
use arcturus_compiler::{CompilerConfig, ModelCompiler};
use clap::Parser;

#[derive(Parser)]
#[command(name = "arcturus-compiler")]
#[command(about = "Compile AI models for Arcturus chip")]
struct Args {
    /// Input model file (PyTorch .bin or .pt)
    #[arg(short, long)]
    input: String,

    /// Output W matrix file
    #[arg(short, long)]
    output: String,

    /// Quantization bits (4, 8, 16)
    #[arg(short, long, default_value = "16")]
    quantization: u8,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config = CompilerConfig {
        quantization_bits: args.quantization,
        max_matrix_size: 10000,
    };

    let compiler = ModelCompiler::new(config);
    let w_matrix = compiler.compile(&args.input)?;

    std::fs::write(&args.output, w_matrix)?;

    println!("Compiled {} -> {}", args.input, args.output);

    Ok(())
}
