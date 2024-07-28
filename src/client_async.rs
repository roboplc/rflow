use std::{
    net::ToSocketAddrs,
    sync::{atomic, Arc},
    time::Duration,
};

use parking_lot_rt::Mutex as SyncMutex;
use rtsc::{
    channel_async::{Receiver, Sender},
    ops::Operation,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::Mutex,
    task::JoinHandle,
};
use tracing::trace;

use crate::{
    client::ConnectionOptions, Direction, Error, API_VERSION, GREETING, HEADERS_TRANSMISSION_END,
};

/// Client instance
#[derive(Clone)]
pub struct ClientAsync {
    inner: Arc<Inner>,
}

struct Inner {
    writer: Mutex<OwnedWriteHalf>,
    connected: Arc<atomic::AtomicBool>,
    timeout: Duration,
    reader_fut: SyncMutex<JoinHandle<()>>,
}

impl ClientAsync {
    /// Connect to a server and create a client instance
    pub async fn connect(
        addr: impl ToSocketAddrs,
    ) -> Result<(Self, Receiver<(Direction, String)>), Error> {
        Self::connect_with_options(addr, &ConnectionOptions::default()).await
    }
    /// Connect to a server and create a client instance with the defined options
    pub async fn connect_with_options(
        addr: impl ToSocketAddrs,
        options: &ConnectionOptions,
    ) -> Result<(Self, Receiver<(Direction, String)>), Error> {
        let timeout = options.timeout;
        let op = Operation::new(timeout);
        let mut stream = tokio::time::timeout(
            timeout,
            TcpStream::connect(
                &addr
                    .to_socket_addrs()?
                    .next()
                    .ok_or(Error::InvalidAddress)?,
            ),
        )
        .await??;
        stream.set_nodelay(true)?;
        let reader = tokio::io::BufReader::new(&mut stream);
        let mut lines = reader.lines();
        trace!("reading greeting");
        let line = tokio::time::timeout(
            op.remaining().map_err(|_| Error::Timeout)?,
            lines.next_line(),
        )
        .await??
        .ok_or(Error::InvalidData)?;
        let mut sp = line.split('/');
        if sp.next() != Some(GREETING) {
            return Err(Error::InvalidData);
        }
        let api_version: u8 = sp
            .next()
            .ok_or_else(|| {
                trace!("Unable to parse greetings header value");
                Error::InvalidData
            })?
            .trim()
            .parse()
            .map_err(|error| {
                trace!(%error, "Unable to parse greetings header value");
                Error::InvalidData
            })?;
        if api_version != API_VERSION {
            return Err(Error::ApiVersion(api_version));
        }
        trace!("reading headers");
        let mut headers_end = false;
        // headers are reserved for future use
        while let Ok(Some(line)) = tokio::time::timeout(
            op.remaining().map_err(|_| Error::Timeout)?,
            lines.next_line(),
        )
        .await?
        {
            if line == HEADERS_TRANSMISSION_END {
                headers_end = true;
                break;
            }
        }
        if !headers_end {
            trace!("Invalid headers transmission end");
            return Err(Error::InvalidData);
        }
        trace!(api_version, "connection estabilished");
        let (tx, rx) = rtsc::channel_async::bounded(options.incoming_queue_size);
        let (reader, writer) = stream.into_split();
        let connected = Arc::new(atomic::AtomicBool::new(true));
        let connected_c = connected.clone();
        let reader_fut = tokio::spawn(handle_connection(tx, reader, connected_c));
        Ok((
            Self {
                inner: Inner {
                    writer: Mutex::new(writer),
                    connected,
                    timeout,
                    reader_fut: SyncMutex::new(reader_fut),
                }
                .into(),
            },
            rx,
        ))
    }
    /// Send a message to the server
    pub async fn try_send(&self, data: impl ToString) -> Result<(), Error> {
        let mut writer = self.inner.writer.lock().await;
        tokio::time::timeout(
            self.inner.timeout,
            writer.write_all(format!("{}\n", data.to_string()).as_bytes()),
        )
        .await?
        .map_err(Into::into)
    }
    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.inner.connected.load(atomic::Ordering::Relaxed)
    }
}

async fn handle_connection(
    tx: Sender<(Direction, String)>,
    mut reader: OwnedReadHalf,
    connected: Arc<atomic::AtomicBool>,
) {
    macro_rules! quit {
        () => {{
            break;
        }};
    }
    macro_rules! report_msg {
        ($dir: expr, $msg: expr) => {
            if tx.send(($dir, $msg)).await.is_err() {
                quit!();
            }
        };
    }
    let reader = tokio::io::BufReader::new(&mut reader);
    let mut last_direction: Option<Direction> = None;
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(msg) = line.strip_prefix(Direction::ClientToServer.as_str()) {
            last_direction = Some(Direction::ClientToServer);
            report_msg!(Direction::ClientToServer, msg.to_string());
        } else if let Some(msg) = line.strip_prefix(Direction::ServerToClient.as_str()) {
            last_direction = Some(Direction::ServerToClient);
            report_msg!(Direction::ServerToClient, msg.to_string());
        } else {
            let Some(last_direction) = last_direction else {
                quit!();
            };
            report_msg!(last_direction, line);
        }
    }
    connected.store(false, atomic::Ordering::Relaxed);
}

impl Drop for ClientAsync {
    fn drop(&mut self) {
        self.inner.reader_fut.lock().abort();
        self.inner.connected.store(false, atomic::Ordering::Relaxed);
    }
}
