use anyhow::Result;
use clap::Parser;
use simple_twine_rust::*;
use std::{fs, path::PathBuf};

/// Simple program to read a twine file
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file to read
    #[arg(help = "Path to the twine file to decode")]
    input: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Reading file: {:?}", args.input);
    let data = fs::read(args.input)?;
    let value = decode_from_buffer(&data);
    println!("{:?}", value);
    Ok(())
}
