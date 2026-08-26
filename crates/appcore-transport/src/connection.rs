// =============================================================================
//        #######
//     ###       ###     F: connection.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! Plain and TLS socket ownership behind one reusable connection type.

use crate::{HttpScheme, HttpTarget, HttpTimeouts, TransportError, TransportResult};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub(crate) enum TransportConnection {
    Plain(TcpStream),
    Tls(native_tls::TlsStream<TcpStream>),
}

impl TransportConnection {
    pub(crate) fn connect(
        target: &HttpTarget,
        timeouts: HttpTimeouts,
        tls: Option<&native_tls::TlsConnector>,
    ) -> TransportResult<Self> {
        let stream = connect_tcp(target, timeouts.connect_ms)?;
        configure(&stream, timeouts)?;
        match target.scheme() {
            HttpScheme::Http => Ok(Self::Plain(stream)),
            HttpScheme::Https => tls
                .ok_or_else(|| TransportError::Tls("TLS connector unavailable".to_string()))?
                .connect(target.host(), stream)
                .map(Self::Tls)
                .map_err(|error| TransportError::Tls(error.to_string())),
        }
    }

    pub(crate) fn set_timeouts(&self, timeouts: HttpTimeouts) -> TransportResult<()> {
        match self {
            Self::Plain(stream) => configure(stream, timeouts),
            Self::Tls(stream) => configure(stream.get_ref(), timeouts),
        }
    }
}

impl Read for TransportConnection {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer),
            Self::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for TransportConnection {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buffer),
            Self::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

fn connect_tcp(target: &HttpTarget, timeout_ms: u64) -> TransportResult<TcpStream> {
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let started = Instant::now();
    let addresses = (target.host(), target.port())
        .to_socket_addrs()
        .map_err(|error| TransportError::Dns(error.to_string()))?;
    if started.elapsed() >= timeout {
        return Err(TransportError::Timeout);
    }
    let mut last_error = None;
    for address in addresses {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(TransportError::Timeout);
        }
        match TcpStream::connect_timeout(&address, remaining) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.map_or_else(
        || TransportError::Dns("host resolved to no addresses".to_string()),
        map_io_error,
    ))
}

fn configure(stream: &TcpStream, timeouts: HttpTimeouts) -> TransportResult<()> {
    stream
        .set_read_timeout(Some(Duration::from_millis(timeouts.read_ms.max(1))))
        .and_then(|_| {
            stream.set_write_timeout(Some(Duration::from_millis(timeouts.write_ms.max(1))))
        })
        .and_then(|_| stream.set_nodelay(true))
        .map_err(map_io_error)
}

pub(crate) fn map_io_error(error: std::io::Error) -> TransportError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => TransportError::Timeout,
        std::io::ErrorKind::ConnectionRefused => TransportError::ConnectionRefused,
        _ => TransportError::Io(error.to_string()),
    }
}
