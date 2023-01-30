//! Load testing tool

use std::{net::SocketAddr, time::Instant};

use anyhow::{Context, Result};
use mini_redis_rs::Client;
use tokio::{net::TcpStream, task::LocalSet};

const N_CONN: usize = 1_024;
const N_ITER: usize = 1_000;
const KEYS: &[&str] = &["a", "b", "c", "d", "e"];

#[tokio::main(flavor="current_thread")]
pub async fn main() -> Result<()> {
    env_logger::init();
    let addr: SocketAddr =
        "127.0.0.1:6379".parse().with_context(|| "trying to get address")?;
    log::info!(
        "testing {addr:?} with {N_CONN} connections, {N_ITER} iterations"
    );

    let local_set = LocalSet::new();

    let start = Instant::now();

    for _task in 0..N_CONN {
        local_set.spawn_local(async move {
            let mut arena = bumpalo::Bump::new(); // arena for this task
            log::debug!("connect to {addr} (task {_task})");
            let mut sock = TcpStream::connect(addr).await?;

            let mut client = Client::new(&mut sock, addr);

            for _i in 0..N_ITER {
                //log::debug!("start iteration {_i} for task {_task}");
                arena.reset();
                let key = KEYS[_i % KEYS.len()];
                let n: usize = match client.q_get(key, &arena).await {
                    Ok(str) => str
                        .parse::<usize>()
                        .with_context(|| "parsing integer obtained from get")
                        .unwrap_or(0),
                    Err(e) => {
                        log::error!("error in get: {e:?}");
                        0
                    }
                };

                let v = format!("{}", n + 1);
                match client.q_set(key, &v, &arena).await {
                    Ok(_) => (),
                    Err(e) => {
                        log::error!("error in set: {e:?}");
                        continue;
                    }
                };
            }
            anyhow::Ok(())
        });
    }

    local_set.await;

    let elapsed = start.elapsed();
    let n = N_CONN * N_ITER * KEYS.len();
    println!(
        "done {n} get+set in {t}s ({rate:.2}/s)",
        t = (elapsed.as_millis() as f64) / 1000.,
        rate = (n as f64) / (elapsed.as_secs_f64())
    );

    Ok(())
}
