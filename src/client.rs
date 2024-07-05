use std::{
    io::{BufRead, Write},
    net::{Shutdown, TcpStream, ToSocketAddrs},
    sync::{atomic, Arc},
    thread,
    time::Duration,
};

use rtsc::locking::Mutex;
use rtsc::{
    channel::{Receiver, Sender},
    ops::Operation,
};
use tracing::trace;

use crate::{
    Direction, Error, API_VERSION, DEFAULT_INCOMING_QUEUE_SIZE, DEFAULT_TIMEOUT, GREETING,
    HEADERS_TRANSMISSION_END,
};

/// Client instance
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    stream: Mutex<TcpStream>,
    connected: Arc<atomic::AtomicBool>,
}

/// Connection options
#[derive(Clone)]
pub struct ConnectionOptions {
    timeout: Duration,
    incoming_queue_size: usize,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            incoming_queue_size: DEFAULT_INCOMING_QUEUE_SIZE,
        }
    }
}

impl ConnectionOptions {
    /// Create a new connection options instance
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the connection timeout (default: 5 seconds)
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    /// Set the incoming queue size (default: 128)
    pub fn incoming_queue_size(mut self, size: usize) -> Self {
        self.incoming_queue_size = size;
        self
    }
}

impl Client {
    /// Connect to a server and create a client instance
    pub fn connect(
        addr: impl ToSocketAddrs,
    ) -> Result<(Self, Receiver<(Direction, String)>), Error> {
        Self::connect_with_options(addr, &ConnectionOptions::default())
    }
    /// Connect to a server and create a client instance with the defined options
    pub fn connect_with_options(
        addr: impl ToSocketAddrs,
        options: &ConnectionOptions,
    ) -> Result<(Self, Receiver<(Direction, String)>), Error> {
        let timeout = options.timeout;
        let op = Operation::new(timeout);
        let stream = TcpStream::connect_timeout(
            &addr
                .to_socket_addrs()?
                .next()
                .ok_or(Error::InvalidAddress)?,
            timeout,
        )?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.set_nodelay(true)?;
        let reader = &mut std::io::BufReader::new(&stream);
        let mut lines = reader.lines();
        trace!("reading greeting");
        let line = lines.next().ok_or(Error::InvalidData)??;
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
        stream.set_read_timeout(Some(op.remaining().map_err(|_| Error::Timeout)?))?;
        // headers are reserved for future use
        for line in lines.by_ref() {
            if line? == HEADERS_TRANSMISSION_END {
                headers_end = true;
                break;
            }
            stream.set_read_timeout(Some(op.remaining().map_err(|_| Error::Timeout)?))?;
        }
        if !headers_end {
            trace!("Invalid headers transmission end");
            return Err(Error::InvalidData);
        }
        trace!(api_version, "connection estabilished");
        stream.set_read_timeout(None)?;
        let (tx, rx) = rtsc::channel::bounded(options.incoming_queue_size);
        let stream_c = stream.try_clone()?;
        let connected = Arc::new(atomic::AtomicBool::new(true));
        let connected_c = connected.clone();
        thread::spawn(move || handle_connection(tx, stream_c, connected_c));
        Ok((
            Self {
                inner: Inner {
                    stream: Mutex::new(stream),
                    connected,
                }
                .into(),
            },
            rx,
        ))
    }
    /// Send a message to the server
    pub fn try_send(&self, data: impl ToString) -> Result<(), Error> {
        let mut stream = self.inner.stream.lock();
        stream
            .write_all(format!("{}\n", data.to_string()).as_bytes())
            .map_err(Into::into)
    }
    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.inner.connected.load(atomic::Ordering::Relaxed)
    }
}

fn handle_connection(
    tx: Sender<(Direction, String)>,
    stream: TcpStream,
    connected: Arc<atomic::AtomicBool>,
) {
    macro_rules! quit {
        () => {{
            stream.shutdown(Shutdown::Both).ok();
            break;
        }};
    }
    macro_rules! report_msg {
        ($dir: expr, $msg: expr) => {
            if tx.send(($dir, $msg)).is_err() {
                quit!();
            }
        };
    }
    let reader = &mut std::io::BufReader::new(&stream);
    let mut last_direction: Option<Direction> = None;
    for line in reader.lines() {
        let Ok(line) = line else {
            quit!();
        };
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

impl Drop for Client {
    fn drop(&mut self) {
        self.inner.stream.lock().shutdown(Shutdown::Both).ok();
        self.inner.connected.store(false, atomic::Ordering::Relaxed);
    }
}
