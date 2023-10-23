use std::{sync::Arc, net::{Ipv4Addr, SocketAddr, IpAddr}, time::Duration};

use anyhow::{Result, Context};
use dashmap::DashMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::{net::{TcpListener, TcpStream}, io::{AsyncBufRead, AsyncBufReadExt, BufReader, AsyncWrite, AsyncWriteExt, AsyncRead, self}, time::{timeout, sleep}};
use uuid::Uuid;

pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(3);
const SERVER_PORT:u16 = 37835;

#[derive(Debug, Serialize, Deserialize)]
enum ClientMessage {
    Hello(u16),

    Accept(Uuid),
}

#[derive(Debug, Serialize, Deserialize)]
enum ServerMessage {
    Hello(u16),

    Heartbeat,

    Connection(Uuid),

    Error(String),
}

pub struct Server {
    connections: Arc<DashMap<Uuid, TcpStream>>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
        }
    }

    pub async fn listen(self) -> Result<()> {
        let this = Arc::new(self);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), SERVER_PORT);
        let listener = TcpListener::bind(addr).await?;

        loop {
            let (stream, addr) = listener.accept().await?;
            let this = Arc::clone(&this);
            tokio::spawn(
                async move {
                    this.handle_connection(stream, addr).await;
                }
            );
        }
        Ok(())
    }

    async fn handle_connection(&self, stream: tokio::net::TcpStream, addr: SocketAddr) -> Result<()> {
        let mut stream = BufReader::new(stream);
        match recv_json_timeout(&mut stream).await? {
            Some(ClientMessage::Hello(port)) => {
                let listener = match TcpListener::bind(("::", port)).await {
                    Ok(l) => l,
                    Err(_) => {
                        send_json(
                            &mut stream,
                            ServerMessage::Error("port already in use".to_string()),
                        )
                        .await?;
                        return Ok(());
                    } 
                };
                let port = listener.local_addr()?.port();
                send_json(&mut stream, ServerMessage::Hello(port)).await?;

                loop {
                    if send_json(&mut stream, ServerMessage::Heartbeat)
                        .await
                        .is_err()
                    {
                        return Ok(());
                    }
                    const TIMEOUT: Duration = Duration::from_millis(500);
                    if let Ok(result) = timeout(TIMEOUT, listener.accept()).await {
                        let (stream2, addr) = result?;

                        let id = Uuid::new_v4();
                        let conns = Arc::clone(&self.connections);

                        conns.insert(id, stream2);
                        tokio::spawn(async move {
                            sleep(Duration::from_secs(10)).await;
                            conns.remove(&id);
                        });
                        send_json(&mut stream, ServerMessage::Connection(id)).await?;
                    }
                }

                return Ok(());
            },
            Some(ClientMessage::Accept(id)) => {
                match self.connections.remove(&id) {
                    Some((_, stream2)) => proxy(stream, stream2).await?,
                    None => {},
                }
                return Ok(());
            },
            _ => {
                return Ok(());
            }
        }
    }
}

async fn recv_json_timeout<T: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> Result<Option<T>> {
    timeout(NETWORK_TIMEOUT, recv_json(reader, &mut Vec::new()))
        .await
        .context("timed out waiting for initial message")?
}

async fn recv_json<T: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
    buf: &mut Vec<u8>,
) -> Result<Option<T>> {
    buf.clear();
    reader.read_until(0, buf).await?;
    if buf.is_empty() {
        return Ok(None);
    }
    if buf.last() == Some(&0) {
        buf.pop();
    }
    Ok(serde_json::from_slice(buf).context("failed to parse JSON")?)
}

async fn send_json<T: Serialize>(writer: &mut (impl AsyncWrite + Unpin), msg: T) -> Result<()> {
    let msg = serde_json::to_vec(&msg)?;
    writer.write_all(&msg).await?;
    writer.write_all(&[0]).await?;
    Ok(())
}

async fn proxy<S1, S2>(stream1: S1, stream2: S2) -> io::Result<()>
where
    S1: AsyncRead + AsyncWrite + Unpin,
    S2: AsyncRead + AsyncWrite + Unpin,
{
    let (mut s1_read, mut s1_write) = io::split(stream1);
    let (mut s2_read, mut s2_write) = io::split(stream2);
    tokio::select! {
        res = io::copy(&mut s1_read, &mut s2_write) => res,
        res = io::copy(&mut s2_read, &mut s1_write) => res,
    }?;
    Ok(())
}



