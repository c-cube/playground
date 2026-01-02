use std::{
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
use regex::Regex;

#[derive(Debug, Parser)]
struct Cli {
    regex: Regex,
    dirs: Vec<String>,
    // level of parallelism
    // j: Option<u8>
}

#[derive(Debug, Default)]
struct Stats {
    files: AtomicU64,
    lines: AtomicU64,
    matches: AtomicU64,
    errors: Mutex<Vec<anyhow::Error>>,
}

fn process_file(regex: &Regex, path: PathBuf, stats: Arc<Stats>) -> Result<()> {
    log::debug!("processing file {path:?}");
    stats.files.fetch_add(1, Ordering::SeqCst);

    let mut file = std::io::BufReader::new(std::fs::File::open(&path)?);

    let mut line_buf = String::with_capacity(64 * 1024);
    loop {
        line_buf.clear();
        let n = file.read_line(&mut line_buf)?;
        if n == 0 {
            break;
        }

        stats.lines.fetch_add(1, Ordering::SeqCst);

        let line = &line_buf[0..n - 1];
        if regex.is_match(&line) {
            stats.matches.fetch_add(1, Ordering::Relaxed);

            let basename = path.file_name().unwrap_or(path.as_os_str()).display();
            println!("{basename}: {line:?}");
        }
    }

    Ok(())
}

fn process_dir(regex: &Regex, dir: String, stats: Arc<Stats>) -> Result<()> {
    for file in walkdir::WalkDir::new(dir) {
        let file = match file {
            Ok(f) if f.file_type().is_file() => f,
            Ok(_) => continue,
            Err(err) => {
                let mut errors = stats.errors.lock().unwrap();
                errors.push(err.into());
                continue;
            }
        };

        process_file(regex, file.into_path(), stats.clone())?;
    }

    Ok(())
}

fn main() -> Result<()> {
    env_logger::try_init()?;
    let cli = Cli::try_parse().with_context(|| "parsing CLI")?;
    log::debug!("cli: {cli:#?}");

    let t_start = Instant::now();

    let stats = Arc::new(Stats::default());

    for dir in &cli.dirs {
        process_dir(&cli.regex, dir.clone(), stats.clone())?;
    }

    let t_stop = Instant::now();

    log::info!("stats: {:?}", *stats);
    log::info!("done in {:?}", t_stop - t_start);
    Ok(())
}
