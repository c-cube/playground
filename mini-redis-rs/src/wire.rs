//! Wire protocol

use std::net::SocketAddr;

use anyhow::{Context, Result};
use async_recursion::async_recursion;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{
        tcp::{ReadHalf, WriteHalf},
        TcpStream,
    },
};

/// TCP Connection.
pub struct Conn<'a> {
    addr: SocketAddr,
    buf: Vec<u8>,
    read: BufReader<ReadHalf<'a>>,
    write: WriteHalf<'a>,
}

/// Redis message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Frame<'a> {
    String(&'a str),
    Int(isize),
    Bulk(&'a [Frame<'a>]),
    Error(&'a str),
}

impl<'a> Conn<'a> {
    /// New connection object from a connected socket.
    pub fn new(sock: &'a mut TcpStream, addr: SocketAddr) -> Self {
        let (read, write) = sock.split();
        log::trace!("hello client on {addr:?}");
        Self {
            addr,
            read: BufReader::new(read),
            buf: vec![0; 16 * 1024],
            write,
        }
    }

    /// Address of the remote socket.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// Read a Redis value using the given arena.
#[async_recursion(?Send)]
pub async fn read_frame<'arena>(
    conn: &mut Conn,
    arena: &'arena bumpalo::Bump,
) -> Result<Option<Frame<'arena>>> {
    conn.buf.clear();
    let n = conn.read.read_until(b'\n', &mut conn.buf).await?;
    if n == 0 {
        log::debug!("connection closed for {a:?}", a = conn.addr);
        return Ok(None); // eof
    }

    // remove whitespace
    let mut buf = &conn.buf[..];
    while !buf.is_empty() && buf[buf.len() - 1].is_ascii_whitespace() {
        buf = &buf[..buf.len() - 1]
    }

    if buf.is_empty() {
        // try again
        return read_frame(conn, arena).await;
    }

    match buf[0] {
        b'-' => {
            let data = std::str::from_utf8(arena.alloc_slice_copy(&buf[1..]))
                .with_context(|| "decoding error as string")?;
            Ok(Some(Frame::Error(data)))
        }
        b'+' => {
            let data = std::str::from_utf8(arena.alloc_slice_copy(&buf[1..]))
                .with_context(|| "decoding string")?;
            Ok(Some(Frame::String(data)))
        }
        b':' => {
            let data = std::str::from_utf8(&buf[1..])
                .with_context(|| "")
                .with_context(|| "decoding integer")?;
            let i: isize = data.parse().with_context(|| "decoding integer")?;
            Ok(Some(Frame::Int(i)))
        }
        b'*' => {
            // array
            log::trace!("read array");
            let len = {
                let data = std::str::from_utf8(&buf[1..])
                    .with_context(|| "decoding length of array")?;
                let len: usize =
                    data.parse().with_context(|| "decoding integer")?;
                len
            };
            // array to fill
            let v = arena.alloc_slice_fill_with(len, |_| Frame::Int(0));
            #[allow(clippy::needless_range_loop)]
            for i in 0..len {
                v[i] = read_frame(conn, arena)
                    .await
                    .with_context(|| "reading element of an array")?
                    .ok_or_else(|| {
                        anyhow::anyhow!("need a value in the array")
                    })?;
            }
            Ok(Some(Frame::Bulk(v)))
        }
        b'$' => {
            // bulk string
            let len = {
                let data = std::str::from_utf8(&buf[1..])
                    .with_context(|| "")
                    .with_context(|| "decoding length of string")?;
                let len: usize =
                    data.parse().with_context(|| "decoding integer")?;
                len
            };
            // array to fill
            let v = arena.alloc_slice_fill_with(len, |_| b'\x00');
            conn.read.read_exact(&mut v[..]).await?;
            let mut b2 = [0u8; 2];
            conn.read.read_exact(&mut b2).await?;
            if b2 != [b'\r', b'\n'] {
                anyhow::bail!("expect crlf after a bulk string");
            }
            let data = std::str::from_utf8(v)
                .with_context(|| " decoding bulk string")?;
            Ok(Some(Frame::String(data)))
        }

        _c => {
            anyhow::bail!("invalid first char: {_c:?}");
        }
    }
}

#[async_recursion(?Send)]
async fn write_frame_rec(conn: &mut Conn, frame: &Frame) -> Result<()> {
    match frame {
        Frame::String(s) => {
            let frame = format!("${}\r\n", s.as_bytes().len());
            conn.write.write_all(frame.as_bytes()).await?;
            conn.write.write_all(s.as_bytes()).await?;
            conn.write.write_all(b"\r\n").await?;
        }
        Frame::Int(i) => {
            let frame = format!(":{}", i);
            conn.write.write_all(frame.as_bytes()).await?
        }
        Frame::Bulk(a) => {
            let frame = format!("*{}\r\n", a.len());
            conn.write.write_all(frame.as_bytes()).await?;
            for x in &a[..] {
                write_frame_rec(conn, x).await?;
            }
        }
        Frame::Error(e) => {
            let msg = format!("-{}\r\n", e);
            conn.write.write_all(msg.as_bytes()).await?;
        }
    }
    Ok(())
}

/// Write a frame.
pub async fn write_frame(conn: &mut Conn<'_>, frame: &Frame<'_>) -> Result<()> {
    log::debug!("sending msg {frame:?}");
    write_frame_rec(conn, frame).await?;
    conn.write.flush().await?;
    Ok(())
}
