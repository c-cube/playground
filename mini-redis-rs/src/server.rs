use std::{net::SocketAddr, sync::Arc};

use crate::wire::{self, Conn, Frame};
use anyhow::Result;
use dashmap::DashMap;
use tokio::net::TcpStream;

/// Main state for the database.
#[derive(Debug, Default)]
pub struct State {
    kv: DashMap<String, String>,
}

/// Handler for a given client.
pub struct ClientHandler<'a> {
    addr: SocketAddr,
    conn: Conn<'a>,
}

impl<'a> ClientHandler<'a> {
    pub fn new_from_conn(conn: Conn<'a>) -> Self {
        let addr = conn.addr();
        Self { conn, addr }
    }

    pub fn new(sock: &'a mut TcpStream, addr: SocketAddr) -> Self {
        let conn = Conn::new(sock, addr);
        Self::new_from_conn(conn)
    }

    /// Serve queries from this client.
    ///
    /// The state is stored in `st`.
    pub async fn serve(&mut self, st: Arc<State>) -> Result<()> {
        let addr = self.addr;
        let mut arena = bumpalo::Bump::new();

        loop {
            let msg = match wire::read_frame(&mut self.conn, &arena).await {
                Ok(Some(msg)) => msg,
                Ok(None) => break,
                Err(e) => {
                    log::error!("could not read frame: {e:?}");
                    continue;
                }
            };
            log::debug!("got msg {msg:#?} from {addr:?}");

            match msg {
                Frame::Bulk(&[Frame::String("get"), Frame::String(k)]) => {
                    log::debug!("get {k:?}");
                    match st.kv.get(k) {
                        Some(v) => {
                            log::trace!("get: reply with {v:?}");
                            let v = &*v;
                            wire::write_frame(
                                &mut self.conn,
                                &Frame::String(v),
                            )
                            .await?;
                        }
                        None => {
                            wire::write_frame(
                                &mut self.conn,
                                &Frame::String("-not found"),
                            )
                            .await?;
                        }
                    };
                }
                Frame::Bulk(
                    &[Frame::String("set"), Frame::String(k), Frame::String(v)],
                ) => {
                    log::debug!("insert {k:?} => {v:?}");
                    st.kv.insert(k.to_string(), v.to_string());
                    wire::write_frame(&mut self.conn, &Frame::String("OK"))
                        .await?;
                }

                Frame::Bulk(arr)
                    if !arr.is_empty()
                        && arr[0] == Frame::String("COMMAND") =>
                {
                    // nothing to do
                    let frame = &Frame::Error("unknown command");
                    wire::write_frame(&mut self.conn, frame).await?;
                }
                _ => {
                    let msg = format!("unknown command {msg:?}");
                    wire::write_frame(&mut self.conn, &Frame::Error(&msg))
                        .await?;
                }
            }

            arena.reset();
        }

        log::info!("done serving client {addr:?}");
        Ok(())
    }
}
