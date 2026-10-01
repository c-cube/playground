mod daemon;
mod gui;
mod proto;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "tiny unix-socket chat with a Tk UI")]
struct Cli {
    /// Path to the daemon's unix socket
    #[arg(short, long, default_value = "./chat.sock", global = true)]
    sock: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the daemon
    Daemon,
    /// Open the Tk client
    Gui {
        /// Name to post as (random by default)
        #[arg(short, long)]
        author: Option<String>,
        /// Tcl UI script, re-sourced when it changes on disk
        #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui.tcl"))]
        ui: PathBuf,
    },
    /// Post a single message
    Send {
        /// Name to post as (default: $USER)
        #[arg(short, long)]
        author: Option<String>,
        chan: String,
        msg: String,
    },
    /// List channels (exits with an error if no daemon is reachable)
    Ls,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Daemon => daemon::run(&cli.sock),
        Cmd::Gui { author, ui } => gui::run(&cli.sock, author, ui),
        Cmd::Send { author, chan, msg } => {
            let author = author.or_else(|| std::env::var("USER").ok()).unwrap_or_else(|| "anon".into());
            let mut conn = proto::Conn::connect(&cli.sock)?;
            match conn.call(&proto::Req::Post { chan, author, msg })? {
                proto::Resp::Posted { .. } => Ok(()),
                r => bail!("unexpected response {r:?}"),
            }
        }
        Cmd::Ls => match proto::Conn::connect(&cli.sock)?.call(&proto::Req::ListChans)? {
            proto::Resp::Chans { chans } => {
                chans.iter().for_each(|c| println!("{c}"));
                Ok(())
            }
            r => bail!("unexpected response {r:?}"),
        },
    }
}
