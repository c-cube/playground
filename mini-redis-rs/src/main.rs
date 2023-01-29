use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use async_recursion::async_recursion;
use dashmap::DashMap;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{
        tcp::{ReadHalf, WriteHalf},
        TcpListener,
    },
    task::LocalSet,
};

/// Main state
#[derive(Debug, Default)]
struct State {
    kv: DashMap<String, String>,
}

/// Redis message
#[derive(Clone, Debug, Eq, PartialEq)]
enum Frame<'a> {
    String(&'a str),
    Int(isize),
    Bulk(&'a [Frame<'a>]),
    Error(&'a str),
}

struct Client<'a> {
    addr: SocketAddr,
    buf: Vec<u8>,
    read: BufReader<ReadHalf<'a>>,
    write: WriteHalf<'a>,
}

/// Read a Redis value
#[async_recursion(?Send)]
async fn read_frame_rec<'arena>(
    client: &mut Client,
    arena: &'arena bumpalo::Bump,
) -> Result<Option<Frame<'arena>>> {
    client.buf.clear();
    let n = client.read.read_until(b'\n', &mut client.buf).await?;
    if n == 0 {
        log::debug!("connection closed for {a:?}", a = client.addr);
        return Ok(None); // eof
    }

    log::trace!("got raw buf {buf:?}", buf = std::str::from_utf8(&client.buf));

    // remove whitespace
    let mut buf = &client.buf[..];
    while !buf.is_empty() && buf[buf.len() - 1].is_ascii_whitespace() {
        buf = &buf[..buf.len() - 1]
    }

    if buf.is_empty() {
        // try again
        return read_frame(client, arena).await;
    }
    log::trace!("got trimmed buf {buf:?}", buf = std::str::from_utf8(buf));

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
                    .with_context(|| "")
                    .with_context(|| "decoding length of array")?;
                let len: usize =
                    data.parse().with_context(|| "decoding integer")?;
                len
            };
            // array to fill
            let v = arena.alloc_slice_fill_with(len, |_| Frame::Int(0));
            #[allow(clippy::needless_range_loop)]
            for i in 0..len {
                v[i] =
                    read_frame_rec(client, arena).await?.ok_or_else(|| {
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
            client.read.read_exact(&mut v[..]).await?;
            let data = std::str::from_utf8(v)
                .with_context(|| " decoding bulk string")?;
            Ok(Some(Frame::String(data)))
        }

        _c => {
            anyhow::bail!("invalid first char: {_c:?}");
        }
    }
}

async fn read_frame<'a, 'arena>(
    client: &'a mut Client<'_>,
    arena: &'arena bumpalo::Bump,
) -> Result<Option<Frame<'arena>>> {
    // read full frame
    let fr = read_frame_rec(client, &arena).await;
    let _ = client.read.read_until(b'\n', &mut client.buf); // trailing line
    fr
}

/// Write a frame
#[async_recursion(?Send)]
async fn write_frame_rec(client: &mut Client, frame: &Frame) -> Result<()> {
    match frame {
        Frame::String(s) => {
            let frame = format!("${}\r\n", s.as_bytes().len());
            client.write.write_all(frame.as_bytes()).await?;
            client.write.write_all(s.as_bytes()).await?;
        }
        Frame::Int(i) => {
            let frame = format!(":{}", i);
            client.write.write_all(frame.as_bytes()).await?
        }
        Frame::Bulk(a) => {
            let frame = format!("*{}", a.len());
            client.write.write_all(frame.as_bytes()).await?;
            for x in &a[..] {
                write_frame_rec(client, x).await?;
            }
        }
        Frame::Error(e) => {
            let msg = format!("-{}", e);
            client.write.write_all(msg.as_bytes()).await?;
        }
    }
    Ok(())
}

async fn write_frame(client: &mut Client<'_>, frame: &Frame<'_>) -> Result<()> {
    write_frame_rec(client, frame).await?;
    client.write.write_all(b"\r\n").await?;
    Ok(())
}

impl<'a> Client<'a> {
    async fn serve(&mut self, st: Arc<State>) -> Result<()> {
        let addr = self.addr;
        let mut arena = bumpalo::Bump::new();

        loop {
            let msg = match read_frame(self, &arena).await {
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
                    log::trace!("get {k:?}");
                    match st.kv.get(k) {
                        Some(v) => {
                            log::trace!("get: reply with {v:?}");
                            let v = &*v;
                            write_frame(self, &Frame::String(v)).await?;
                            self.write.flush().await?;
                        }
                        None => {
                            write_frame(self, &Frame::String("-not found"))
                                .await?;
                            self.write.flush().await?;
                        }
                    };
                }
                Frame::Bulk(
                    &[Frame::String("set"), Frame::String(k), Frame::String(v)],
                ) => {
                    log::trace!("insert {k:?} => {v:?}");
                    st.kv.insert(k.to_string(), v.to_string());
                    write_frame(self, &Frame::String("ok")).await?;
                    self.write.flush().await?;
                }

                Frame::Bulk(arr)
                    if !arr.is_empty()
                        && arr[0] == Frame::String("COMMAND") =>
                {
                    // nothing to do
                    log::trace!("ignore COMMAND");
                }
                _ => anyhow::bail!("unknown command {msg:?}"),
            }

            arena.reset();
        }

        log::info!("done serving client {addr:?}");
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<()> {
    env_logger::init();
    let st: Arc<State> = Arc::new(Default::default());

    let listen = TcpListener::bind("127.0.0.1:6379")
        .await
        .with_context(|| "binding socket")?;

    let local = LocalSet::new(); // spawn on same thread

    local
        .run_until(async move {
            loop {
                let (mut sock, addr) =
                    listen.accept().await.expect("ohno listen"); // FIXME
                log::info!("new client on {a:?}", a = addr);
                let st = st.clone();
                tokio::task::spawn_local(async move {
                    let (read, write) = sock.split();
                    log::trace!("hello client on {addr:?}");
                    let mut client = Client {
                        addr,
                        read: BufReader::new(read),
                        buf: vec![0; 16 * 1024],
                        write,
                    };
                    client.serve(st).await?;
                    anyhow::Ok(())
                });
            }
        })
        .await;
    Ok(())
}
