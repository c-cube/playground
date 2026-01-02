use std::{
    collections::HashMap,
    io::BufRead,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};

use anyhow::{Context, Result};
use clap::Parser;
use rayon::prelude::*;
use regex::Regex;

#[derive(Debug, Parser)]
struct Cli {
    regex: Regex,
    dirs: Vec<String>,
    /// Parallel version
    #[arg(long)]
    par: bool,
    /// Print the matches
    #[arg(short, long)]
    print: bool,
}

#[derive(Debug, Default)]
struct Stats {
    files: AtomicU64,
    lines: AtomicU64,
    matches: AtomicU64,
    bytes: AtomicUsize,
    errors: Mutex<HashMap<String, usize>>,
}

impl Stats {
    fn add_err(&self, err: impl Into<anyhow::Error>) {
        let err = format!("{}", err.into().root_cause());
        let mut errors = self.errors.lock().unwrap();
        let entry = errors.entry(err);
        *entry.or_insert(0) += 1;
    }
}

fn process_file(cli: &Cli, path: PathBuf, stats: &Stats) -> Result<()> {
    log::debug!("processing file {path:?}");
    stats.files.fetch_add(1, Ordering::SeqCst);

    // avoid sharing internal state https://docs.rs/regex/latest/regex/#performance
    let regex = cli.regex.clone();

    const BUF_SIZE: usize = 64 * 1024;
    let mut file = std::io::BufReader::with_capacity(BUF_SIZE, std::fs::File::open(&path)?);

    let mut line_buf = String::with_capacity(BUF_SIZE);
    let mut line_count = 0;
    let mut byte_count = 0;
    let mut match_count = 0;

    loop {
        line_buf.clear();
        let n = file.read_line(&mut line_buf)?;
        if n == 0 {
            break;
        }

        byte_count += n;
        line_count += 1;

        let mut line = &line_buf[0..n];
        if line.as_bytes()[n - 1] == b'\n' {
            line = &line[0..n - 1];
        }

        if regex.is_match(&line) {
            match_count += 1;

            if cli.print {
                let basename = path.file_name().unwrap_or(path.as_os_str()).display();
                println!("{basename}: {line}");
            }
        }
    }

    stats.lines.fetch_add(line_count, Ordering::SeqCst);
    stats.matches.fetch_add(match_count, Ordering::Relaxed);
    stats.bytes.fetch_add(byte_count, Ordering::Relaxed);

    Ok(())
}

fn process_file_noerr(cli: &Cli, path: PathBuf, stats: &Stats) {
    match process_file(cli, path, stats) {
        Ok(()) => (),
        Err(err) => stats.add_err(err),
    }
}

/// Iterate on relevant files
fn dir_entries(dir: String, stats: &Stats) -> impl Iterator<Item = PathBuf> {
    walkdir::WalkDir::new(dir).into_iter().filter_map(
        move |file: walkdir::Result<walkdir::DirEntry>| match file {
            Ok(f) if f.file_type().is_file() => Some(f.into_path()),
            Ok(_) => None,
            Err(err) => {
                stats.add_err(err);
                None
            }
        },
    )
}

fn main() -> Result<()> {
    let cli = Cli::try_parse().with_context(|| "parsing CLI")?;

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init()?;
    log::debug!("cli: {cli:#?}");

    let t_start = Instant::now();

    let stats = Stats::default();

    {
        let dirs = cli
            .dirs
            .iter()
            .cloned()
            .flat_map(|dir| dir_entries(dir, &stats));

        if cli.par {
            dirs.par_bridge()
                .for_each(|p| process_file_noerr(&cli, p.clone(), &stats))
        } else {
            dirs.for_each(|p| process_file_noerr(&cli, p.clone(), &stats))
        }
    }

    let elapsed = Instant::now() - t_start;

    log::info!("stats: {:?}", stats);
    log::info!(
        "done in {:?} ({:.2} MB/s)",
        elapsed,
        (stats.bytes.load(Ordering::SeqCst) as f64 * 1e-6) / elapsed.as_secs_f64()
    );
    Ok(())
}
