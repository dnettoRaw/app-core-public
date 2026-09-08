// =============================================================================
//        #######
//     ###       ###     F: connection.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounded connection reads before an HTTP request reaches middleware.

use axum::serve::Listener;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{Instant, Sleep};

const HTTP_READ_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn with_read_timeout<L>(listener: L) -> ReadTimeoutListener<L> {
    ReadTimeoutListener::new(listener, HTTP_READ_INACTIVITY_TIMEOUT)
}

pub(super) struct ReadTimeoutListener<L> {
    listener: L,
    timeout: Duration,
}

impl<L> ReadTimeoutListener<L> {
    fn new(listener: L, timeout: Duration) -> Self {
        Self { listener, timeout }
    }
}

impl<L> Listener for ReadTimeoutListener<L>
where
    L: Listener,
{
    type Io = ReadTimeoutIo<L::Io>;
    type Addr = L::Addr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let (io, address) = self.listener.accept().await;
        (ReadTimeoutIo::new(io, self.timeout), address)
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

pub(super) struct ReadTimeoutIo<I> {
    io: I,
    timeout: Duration,
    deadline: Pin<Box<Sleep>>,
}

impl<I> ReadTimeoutIo<I> {
    fn new(io: I, timeout: Duration) -> Self {
        Self {
            io,
            timeout,
            deadline: Box::pin(tokio::time::sleep(timeout)),
        }
    }
}

impl<I> AsyncRead for ReadTimeoutIo<I>
where
    I: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buffer.filled().len();
        match Pin::new(&mut self.io).poll_read(context, buffer) {
            Poll::Ready(result) => {
                if result.is_ok() && buffer.filled().len() > before {
                    let next = Instant::now() + self.timeout;
                    self.deadline.as_mut().reset(next);
                }
                Poll::Ready(result)
            }
            Poll::Pending => match self.deadline.as_mut().poll(context) {
                Poll::Ready(()) => Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "HTTP connection read inactivity timeout",
                ))),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

impl<I> AsyncWrite for ReadTimeoutIo<I>
where
    I: AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(context, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};

    #[test]
    fn incomplete_headers_time_out_without_stopping_listener() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let listener = ReadTimeoutListener::new(listener, Duration::from_millis(25));
                let router = Router::new().route("/", get(|| async { "ok" }));
                let server = tokio::spawn(async move { axum::serve(listener, router).await });

                let partial = tokio::net::TcpStream::connect(address).await.unwrap();
                let request = b"GET / HTTP/1.1\r\nHost:";
                partial.writable().await.unwrap();
                assert_eq!(partial.try_write(request).unwrap(), request.len());
                let mut response = [0_u8; 64];
                let read = tokio::time::timeout(Duration::from_secs(1), async {
                    loop {
                        partial.readable().await.unwrap();
                        match partial.try_read(&mut response) {
                            Ok(read) => break read,
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                                ) =>
                            {
                                break 0;
                            }
                            Err(error) => panic!("TCP response read failed: {error}"),
                        }
                    }
                })
                .await
                .unwrap();
                assert_eq!(read, 0);

                let complete = tokio::net::TcpStream::connect(address).await.unwrap();
                let request = b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
                complete.writable().await.unwrap();
                assert_eq!(complete.try_write(request).unwrap(), request.len());
                let read = tokio::time::timeout(Duration::from_secs(1), async {
                    loop {
                        complete.readable().await.unwrap();
                        match complete.try_read(&mut response) {
                            Ok(read) => break read,
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                            Err(error) => panic!("TCP response read failed: {error}"),
                        }
                    }
                })
                .await
                .unwrap();
                assert!(String::from_utf8_lossy(&response[..read]).starts_with("HTTP/1.1 200"));

                server.abort();
                assert!(server.await.unwrap_err().is_cancelled());
            });
    }
}
