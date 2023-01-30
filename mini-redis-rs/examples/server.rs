use anyhow::{Context, Result};
use mini_redis_rs::server;
use std::sync::Arc;
use tokio::{net::TcpListener, task::LocalSet};

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<()> {
    env_logger::init();
    let st: Arc<server::State> = Arc::new(Default::default());

    let addr = "127.0.0.1:6379";
    let listen = TcpListener::bind(addr)
        .await
        .with_context(|| "binding socket")?;

    log::info!("serving on {addr}");
    let local = LocalSet::new(); // spawn on same thread

    local
        .run_until(async move {
            loop {
                let (mut sock, addr) =
                    listen.accept().await.expect("ohno listen"); // FIXME
                log::info!("new client on {a:?}", a = addr);
                let st = st.clone();
                tokio::task::spawn_local(async move {
                    log::trace!("hello client on {addr:?}");
                    let mut client =
                        server::ClientHandler::new(&mut sock, addr);
                    client.serve(st).await?;
                    anyhow::Ok(())
                });
            }
        })
        .await;
    Ok(())
}
