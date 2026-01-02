use std::{
    collections::HashSet,
    io::BufRead,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
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
}

#[derive(Debug, Default)]
struct Stats {
    files: AtomicU64,
    lines: AtomicU64,
    matches: AtomicU64,
    errors: Mutex<HashSet<String>>,
}

impl Stats {
    fn add_err(&self, err: impl Into<anyhow::Error>) {
        let err = format!("{}", err.into().root_cause());
        let mut errors = self.errors.lock().unwrap();
        errors.insert(err);
    }
}

fn process_file(regex: &Regex, path: PathBuf, stats: Arc<Stats>) -> Result<()> {
    log::debug!("processing file {path:?}");
    stats.files.fetch_add(1, Ordering::SeqCst);

    let mut file = std::io::BufReader::new(std::fs::File::open(&path)?);

    let mut line_buf = String::with_capacity(64 * 1024);
    let mut line_count = 0;
    let mut match_count = 0;

    loop {
        line_buf.clear();
        let n = file.read_line(&mut line_buf)?;
        if n == 0 {
            break;
        }

        line_count += 1;

        let line = &line_buf[0..n - 1];
        if regex.is_match(&line) {
            match_count += 1;

            let _basename = path.file_name().unwrap_or(path.as_os_str()).display();
            // println!("{_basename}: {line:?}");
        }
    }

    stats.lines.fetch_add(line_count, Ordering::SeqCst);
    stats.matches.fetch_add(match_count, Ordering::Relaxed);
    Ok(())
}

/// Iterate on relevant files
fn dir_entries(dir: String, stats: Arc<Stats>) -> impl Iterator<Item = PathBuf> {
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
    env_logger::try_init()?;
    let cli = Cli::try_parse().with_context(|| "parsing CLI")?;
    log::debug!("cli: {cli:#?}");

    let t_start = Instant::now();

    let stats = Arc::new(Stats::default());

    {
        let stats1 = stats.clone();
        let stats2 = stats.clone();
        cli.dirs
            .iter()
            .cloned()
            .flat_map(move |dir| dir_entries(dir, stats1.clone()))
            // .par_bridge()
            .for_each(
                move |path| match process_file(&cli.regex, path, stats2.clone()) {
                    Ok(()) => (),
                    Err(err) => stats2.add_err(err),
                },
            )
    }

    let t_stop = Instant::now();

    log::info!("stats: {:?}", *stats);
    log::info!("done in {:?}", t_stop - t_start);
    Ok(())
}
