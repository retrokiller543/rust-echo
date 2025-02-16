use std::collections::VecDeque;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

type Result<T, E = Box<dyn std::error::Error>> = core::result::Result<T, E>;

pub struct Server {
    incoming: tokio::sync::mpsc::Receiver<Vec<u8>>,
    outgoing: tokio::sync::mpsc::Sender<Vec<u8>>,

    listener: TcpListener,
    data: VecDeque<u8>,
}

impl Server {
    pub async fn new() -> Result<Self> {
        let (outgoing, incoming) = mpsc::channel(100);

        let listener = TcpListener::bind("127.0.0.1:12100").await?;

        let data = VecDeque::new();

        Ok(Self {
            outgoing,
            incoming,
            listener,
            data,
        })
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            let (mut socket, _address) = self.listener.accept().await?;

            let (reader, writer) = socket.into_split();

            let mut client = Connection::new(reader, writer);
            client.run().await;
        }
    }
}

pub struct Connection<R, W>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    reader: BufReader<R>,
    writer: BufWriter<W>,
}

impl<R, W> Connection<R, W>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        let reader = BufReader::new(reader);
        let writer = BufWriter::new(writer);
        Self { reader, writer }
    }

    pub async fn run(&mut self) {
        let mut buffer = [0; 1024];

        loop {
            let data = match self.reader.read(&mut buffer).await {
                Ok(0) => return,
                Ok(data) => data,
                Err(errno) => {
                    eprintln!("[-] Failed to read from socket; Error = {:?} . . .", errno);
                    return;
                }
            };

            eprintln!(
                "[+] Data received {} . . .",
                std::str::from_utf8(&buffer).unwrap()
            );

            if let Err(errno) = self.writer.write_all(&buffer[0..data]).await {
                eprintln!("[-] Failed to write to socket; Error = {:?} . . .", errno);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let server = Server::new().await?;
    server.run().await
}
