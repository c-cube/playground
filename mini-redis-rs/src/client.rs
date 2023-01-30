//! Basic redis client.

use crate::{
    wire::Conn,
    wire::{self, Frame},
};
use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpStream;

/// A basic client.
pub struct Client<'a> {
    conn: Conn<'a>,
}

impl<'a> Client<'a> {
    pub fn new(sock: &'a mut TcpStream, addr: SocketAddr) -> Self {
        let conn = Conn::new(sock, addr);
        Self { conn }
    }

    pub async fn q_get<'are>(
        &mut self,
        key: &str,
        arena: &'are bumpalo::Bump,
    ) -> Result<&'are str> {
        let query = Frame::Bulk(arena.alloc_slice_copy(&[
            Frame::String(arena.alloc_str("get")),
            Frame::String(arena.alloc_str(key)),
        ]));
        wire::write_frame(&mut self.conn, &query).await?;

        let res = wire::read_frame(&mut self.conn, arena).await?;
        match res {
            Some(Frame::String(s)) => Ok(s),
            Some(Frame::Error(e)) => {
                anyhow::bail!("server replied with error {e}")
            }
            Some(f) => {
                anyhow::bail!("server replied with unexpected frame {f:?}")
            }
            None => anyhow::bail!("could not read a frame"),
        }
    }

    pub async fn q_set<'are>(
        &mut self,
        key: &str,
        value: &str,
        arena: &'are bumpalo::Bump,
    ) -> Result<bool> {
        let query = Frame::Bulk(arena.alloc_slice_copy(&[
            Frame::String(arena.alloc_str("set")),
            Frame::String(arena.alloc_str(key)),
            Frame::String(arena.alloc_str(value)),
        ]));
        wire::write_frame(&mut self.conn, &query).await?;

        let res = wire::read_frame(&mut self.conn, arena).await?;
        match res {
            Some(Frame::String("ok")) => Ok(true),
            Some(Frame::Error(e)) => {
                anyhow::bail!("server replied with error {e}")
            }
            Some(f) => {
                anyhow::bail!("server replied with unexpected frame {f:?}")
            }
            None => anyhow::bail!("could not read a frame"),
        }
    }
}
