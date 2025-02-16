use std::sync::Arc;

use std::collections::HashMap;
use std::collections::VecDeque;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};

type Result<T, E = Box<dyn std::error::Error>> = core::result::Result<T, E>;

enum Message {
    Chat { id: usize, data: Vec<u8> },
    Disconnect(usize),
}

pub struct Server {
    listener: TcpListener,
    data: VecDeque<u8>,

    connections: Arc<Mutex<HashMap<usize, mpsc::Sender<Vec<u8>>>>>,
}

impl Server {
    pub async fn new() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:12100").await?;

        let data = VecDeque::new();

        let connections = Arc::new(Mutex::new(HashMap::new()));

        Ok(Self {
            listener,
            data,
            connections,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let (broadcast_tx, mut broadcast_rx) = mpsc::channel::<Message>(100);
        let connections = self.connections.clone();

        tokio::spawn(async move {
            while let Some(message) = broadcast_rx.recv().await {
                match message {
                    Message::Chat { id, data } => {
                        let connectionsLock = connections.lock().await;
                        for (clientId, sender) in connectionsLock.iter() {
                            if let Err(errno) = sender.send(data.clone()).await {
                                eprintln!("[-] Failed to send message; Error = {:?} . . .", errno);
                            }
                        }
                    }
                    Message::Disconnect(id) => {
                        let mut connectionsLock = connections.lock().await;
                        connectionsLock.remove(&id);
                        for (clientId, sender) in connectionsLock.iter() {
                            if let Err(errno) = sender.send(format!("[/] {id} disconnected . . .").into_bytes()).await {
                                eprintln!("[-] Failed to send message; Error = {:?} . . .", errno);
                            }
                        }
                    }
                }
            }
        });

        while let Ok((socket, addr)) = self.listener.accept().await {
            let mut connectionsLock = self.connections.lock().await;
            let client_id = connectionsLock.len()+1;
            println!("[+] New client {} connected from {}", client_id, addr);

            let (client_tx, client_rx) = mpsc::channel(100);
            connectionsLock.insert(client_id, client_tx);

            let broadcast_tx = broadcast_tx.clone();
            let (reader, writer) = socket.into_split();

            let mut connection = Connection::new(client_id,
                                                 reader, writer,
                                                 broadcast_tx, client_rx);

            tokio::spawn(async move {
                if let Err(errno) = connection.run().await {
                    eprintln!("[-] Connection error for client {}; Error = {:?} . . .", client_id, errno);
                }
            });
        }
        Ok(())
    }
}

pub struct Connection<R, W>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    client_id: usize,

    reader: BufReader<R>,
    writer: BufWriter<W>,

    receiver: mpsc::Receiver<Vec<u8>>,
    sender: mpsc::Sender<Message>,
}

impl<R, W> Connection<R, W>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    pub fn new(client_id: usize,
               reader: R, writer: W, 
               sender: mpsc::Sender<Message>, receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        let reader = BufReader::new(reader);
        let writer = BufWriter::new(writer);

        Self { client_id, reader, writer, sender, receiver }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut buffer = [0; 1024];

        loop {
            let data = match self.reader.read(&mut buffer).await {
                Ok(0) => return Ok(()),
                Ok(data) => data,
                Err(errno) => {
                    eprintln!("[-] Failed to read from socket; Error = {:?} . . .", errno);
                    return Err(Box::new(errno));
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
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut server = Server::new().await?;
    server.run().await
}
