use std::{
    collections::BTreeMap,
    io::{BufRead as _, BufReader, Write},
    net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs},
    sync::{atomic, Arc},
    thread,
    time::Duration,
};

use rtsc::locking::Mutex;
use rtsc::{
    channel::{self, Receiver, Sender},
    semaphore::Semaphore,
};
use tracing::{trace, warn};

use crate::{
    Direction, Error, API_VERSION, DEFAULT_INCOMING_QUEUE_SIZE, DEFAULT_OUTGOING_QUEUE_SIZE,
    GREETING, HEADERS_TRANSMISSION_END,
};

const DEFAULT_MAX_CLIENTS: usize = 16;

/// Server instance
#[derive(Clone)]
pub struct Server {
    inner: Arc<Inner>,
}

impl Server {
    /// Create a new server instance with the specified timeout
    pub fn new(timeout: Duration) -> Self {
        let (incoming_data_tx, incoming_data_rx) = channel::bounded(DEFAULT_INCOMING_QUEUE_SIZE);
        Self {
            inner: Inner {
                timeout,
                clinet_id: atomic::AtomicUsize::new(0),
                clients: <_>::default(),
                client_count: atomic::AtomicUsize::new(0),
                outgoing_queue_size: atomic::AtomicUsize::new(DEFAULT_OUTGOING_QUEUE_SIZE),
                max_clients: atomic::AtomicUsize::new(DEFAULT_MAX_CLIENTS),
                incoming_data_tx: Mutex::new(incoming_data_tx),
                incoming_data_rx: Mutex::new(Some(incoming_data_rx)),
            }
            .into(),
        }
    }
    /// Set the maximum number of clients (default: 16)
    pub fn set_max_clients(&self, max_clients: usize) -> Result<(), Error> {
        self.inner
            .max_clients
            .store(max_clients, atomic::Ordering::Relaxed);
        Ok(())
    }
    /// Set the outgoing queue size (default: 128). Note: if a client's queue size is full,
    /// messages are dropped to prevent any server blocking.
    pub fn set_outgoing_queue_size(&self, size: usize) -> Result<(), Error> {
        self.inner
            .outgoing_queue_size
            .store(size, atomic::Ordering::Relaxed);
        Ok(())
    }
    /// Set the incoming queue size (default: 128)
    pub fn set_incoming_queue_size(&self, size: usize) -> Result<(), Error> {
        let mut rx = self.inner.incoming_data_rx.lock();
        if rx.is_none() {
            return Err(Error::DataChannelTaken);
        }
        let (incoming_data_tx, incoming_data_rx) = channel::bounded(size);
        *self.inner.incoming_data_tx.lock() = incoming_data_tx;
        *rx = Some(incoming_data_rx);
        Ok(())
    }
    /// Take the data channel
    pub fn take_data_channel(&self) -> Result<Receiver<Arc<String>>, Error> {
        self.inner
            .incoming_data_rx
            .lock()
            .take()
            .ok_or(Error::DataChannelTaken)
    }
    /// Send a message to the clients
    #[inline]
    pub fn send(&self, data: impl ToString) {
        if self.inner.client_count.load(atomic::Ordering::Relaxed) > 0 {
            self.inner
                .send(Direction::ServerToClient, data.to_string().into());
        }
    }
    /// Serve the server
    pub fn serve(&self, addr: impl ToSocketAddrs + std::fmt::Debug) -> Result<(), Error> {
        let listener = TcpListener::bind(addr)?;
        self.serve_with_listener(listener)
    }
    /// Serve the server with the specified listener
    pub fn serve_with_listener(&self, listener: TcpListener) -> Result<(), Error> {
        trace!(addr = ?listener.local_addr(), "starting server");
        let semaphore = Semaphore::new(self.inner.max_clients.load(atomic::Ordering::Relaxed));
        while let Ok((mut socket, addr)) = listener.accept() {
            trace!(?addr, "new connection");
            let permission = semaphore.acquire();
            trace!(?addr, "handling connection");
            let (outgoing_data_tx, outgoing_data_rx) = channel::bounded(
                self.inner
                    .outgoing_queue_size
                    .load(atomic::Ordering::Relaxed),
            );
            let client_id = self.inner.clinet_id.fetch_add(1, atomic::Ordering::Relaxed);
            self.inner
                .clients
                .lock()
                .insert(client_id, outgoing_data_tx);
            let inner = self.inner.clone();
            let incoming_data_tx = self.inner.incoming_data_tx.lock().clone();
            self.inner
                .client_count
                .fetch_add(1, atomic::Ordering::Relaxed);
            thread::spawn(move || {
                let _permission = permission;
                let _r = handle_connection(&mut socket, &inner, incoming_data_tx, outgoing_data_rx);
                inner.client_count.fetch_sub(1, atomic::Ordering::Relaxed);
                inner.clients.lock().remove(&client_id);
            });
        }
        Ok(())
    }
}

type ClientMap = BTreeMap<usize, Sender<(Direction, Arc<String>)>>;

struct Inner {
    timeout: Duration,
    clinet_id: atomic::AtomicUsize,
    clients: Mutex<ClientMap>,
    client_count: atomic::AtomicUsize,
    outgoing_queue_size: atomic::AtomicUsize,
    max_clients: atomic::AtomicUsize,
    incoming_data_tx: Mutex<Sender<Arc<String>>>,
    incoming_data_rx: Mutex<Option<Receiver<Arc<String>>>>,
}

impl Inner {
    fn send(&self, direction: Direction, data: Arc<String>) {
        for client in self.clients.lock().values() {
            if let Err(e) = client.try_send((direction, data.clone())) {
                if e == rtsc::Error::ChannelFull {
                    warn!("failed to send data to a client, queue overflow");
                }
                // ignore all other errors
            }
        }
    }
}

fn handle_connection(
    socket: &mut TcpStream,
    inner: &Inner,
    incoming_data_tx: Sender<Arc<String>>,
    outgoing_data_rx: Receiver<(Direction, Arc<String>)>,
) -> Result<(), Box<dyn std::error::Error>> {
    socket.set_write_timeout(Some(inner.timeout))?;
    socket.set_nodelay(true)?;
    socket.write_all(
        format!(
            "{}/{}\n{}\n",
            GREETING, API_VERSION, HEADERS_TRANSMISSION_END
        )
        .as_bytes(),
    )?;
    let reader = BufReader::new(socket.try_clone()?);
    let mut writer = socket.try_clone()?;
    thread::spawn(move || {
        for (direction, data) in outgoing_data_rx {
            if writer.write_all(direction.as_bytes()).is_err()
                || writer.write_all(data.as_bytes()).is_err()
                || writer.write_all(b"\n").is_err()
            {
                trace!("writer error or finished - shutting down");
                writer.shutdown(Shutdown::Both).ok();
                break;
            }
        }
    });
    for line in reader.lines() {
        let line: Arc<String> = line?.into();
        incoming_data_tx.send(line.clone())?;
        inner.send(Direction::ClientToServer, line);
    }
    trace!("shutting down connection");
    socket.shutdown(Shutdown::Both)?;
    Ok(())
}
