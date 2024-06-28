#![ doc = include_str!( concat!( env!( "CARGO_MANIFEST_DIR" ), "/", "README.md" ) ) ]
#![deny(missing_docs)]

use core::fmt;
use std::{net::ToSocketAddrs, sync::Arc, time::Duration};

use once_cell::sync::Lazy;
use rtsc::channel::Receiver;

mod server;
pub use server::Server;

mod client;
pub use client::{Client, ConnectionOptions};

const GREETING: &str = "RFLOW";
const HEADERS_TRANSMISSION_END: &str = "---";

const API_VERSION: u8 = 1;

const DEFAULT_INCOMING_QUEUE_SIZE: usize = 128;
const DEFAULT_OUTGOING_QUEUE_SIZE: usize = 128;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

static DEFAULT_SERVER: Lazy<Server> = Lazy::new(|| Server::new(DEFAULT_TIMEOUT));

/// Serve the default server
pub fn serve(addr: impl ToSocketAddrs + std::fmt::Debug) -> Result<(), Error> {
    DEFAULT_SERVER.serve(addr)
}

/// Spawn the default server as a separate thread and return the data channel
pub fn spawn(addr: impl ToSocketAddrs + std::fmt::Debug) -> Result<Receiver<Arc<String>>, Error> {
    let listener = std::net::TcpListener::bind(addr)?;
    std::thread::spawn(move || {
        DEFAULT_SERVER
            .serve_with_listener(listener)
            .expect("RFlow server error");
    });
    DEFAULT_SERVER.take_data_channel()
}

/// Send a message to the default server's clients
pub fn send(data: impl ToString) {
    DEFAULT_SERVER.send(data);
}

/// Take the default server data channel
pub fn take_data_channel() -> Result<Receiver<Arc<String>>, Error> {
    DEFAULT_SERVER.take_data_channel()
}

/// Direction of the message (client)
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Direction {
    /// Client-to-server message
    ClientToServer,
    /// Server-to-client message
    ServerToClient,
    /// Unknown, use the last known direction
    Last,
}

impl Direction {
    /// Get direction as bytes
    #[inline]
    pub fn as_bytes(self) -> &'static [u8] {
        self.as_str().as_bytes()
    }
    /// Get direction as string
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClientToServer => ">>>",
            Self::ServerToClient => "<<<",
            Self::Last => unreachable!(),
        }
    }
    /// Get direction as char
    pub fn as_char(self) -> char {
        match self {
            Self::ClientToServer => '>',
            Self::ServerToClient => '<',
            Self::Last => unreachable!(),
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error type
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Data channel is already taken
    #[error("Data channel is already taken")]
    DataChannelTaken,
    /// All I/O errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported API version: {0}")]
    /// Unsupported API version
    ApiVersion(u8),
    /// Invalid data
    #[error("Invalid data")]
    InvalidData,
    /// Invalid TCP/IP address/host name/port
    #[error("Invalid address")]
    InvalidAddress,
    /// Timed out
    #[error("Timed out")]
    Timeout,
}
