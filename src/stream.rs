use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, UnixStream},
};

#[derive(Debug)]
pub enum Stream {
    Tcp(TcpStream),
    // Udp(UdpSocket, Cache<..., ...>),
    Unix(UnixStream),
}

impl From<TcpStream> for Stream {
    fn from(value: TcpStream) -> Self {
        Self::Tcp(value)
    }
}

impl From<UnixStream> for Stream {
    fn from(value: UnixStream) -> Self {
        Self::Unix(value)
    }
}

impl Stream {
    pub fn into_split(
        self,
    ) -> (
        Box<dyn AsyncRead + Send + Unpin>,
        Box<dyn AsyncWrite + Send + Unpin>,
    ) {
        match self {
            Self::Tcp(x) => {
                let (read_half, write_half) = x.into_split();
                (
                    Box::new(read_half) as Box<dyn AsyncRead + Send + Unpin>,
                    Box::new(write_half) as Box<dyn AsyncWrite + Send + Unpin>,
                )
            }
            Self::Unix(x) => {
                let (read_half, write_half) = x.into_split();
                (
                    Box::new(read_half) as Box<dyn AsyncRead + Send + Unpin>,
                    Box::new(write_half) as Box<dyn AsyncWrite + Send + Unpin>,
                )
            }
        }
    }
}
