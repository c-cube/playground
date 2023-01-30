//! Load testing tool

use std::{net::SocketAddr, time::Instant};

use anyhow::{Context, Result};
use mini_redis_rs::Client;
use tokio::{net::TcpStream, task::LocalSet};

const N_CONN: usize = 8;
const N_ITER: usize = 100;

#[tokio::main]
pub async fn main() -> Result<()> {
    env_logger::init();
    let addr: SocketAddr =
        "127.0.0.1:6379".parse().with_context(|| "trying to get address")?;
    log::info!(
        "testing {addr:?} with {N_CONN} connections, {N_ITER} iterations"
    );

    let keys = ["a", "b", "c", "d", "e"];

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
                let key = keys[_i % keys.len()];
                let n: usize = match client.q_get(key, &arena).await {
                    Ok(str) => {
                        str.parse::<usize>().with_context(|| {
                            "parsing integer obtained from get"
                        }).unwrap_or(0)
                    }
                    Err(e) => {
                        log::error!("error in get: {e:?}");
                        continue;
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
    println!(
        "done {n} get+set in {t}s",
        n = N_CONN * N_ITER,
        t = (elapsed.as_millis() as f64) / 1000.
    );

    Ok(())
}
