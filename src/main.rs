#![allow(dead_code)]
#![allow(unused_variables)]

use std::sync::Arc;

use std::collections::HashMap;
use std::collections::VecDeque;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};

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
                        let connections_lock = connections.lock().await;
                        for (client_id, sender) in connections_lock.iter() {
                            if id == *client_id {
                                continue;
                            }
                            
                            if let Err(errno) = sender.send(data.clone()).await {
                                eprintln!("[-] Failed to send message; Error = {:?} . . .", errno);
                            }
                        }
                    }
                    Message::Disconnect(id) => {
                        let mut connections_lock = connections.lock().await;
                        connections_lock.remove(&id);
                        for (client_id, sender) in connections_lock.iter() {
                            if id == *client_id {
                                continue;
                            }
                            
                            if let Err(errno) = sender
                                .send(format!("[/] {id} disconnected . . .").into_bytes())
                                .await
                            {
                                eprintln!("[-] Failed to send message; Error = {:?} . . .", errno);
                            }
                        }
                    }
                }
            }
        });

        while let Ok((socket, addr)) = self.listener.accept().await {
            let mut connections_lock = self.connections.lock().await;
            let client_id = connections_lock.len() + 1;
            println!("[+] New client {} connected from {}", client_id, addr);

            let (client_tx, client_rx) = mpsc::channel(100);
            connections_lock.insert(client_id, client_tx);

            let broadcast_tx = broadcast_tx.clone();
            let (reader, writer) = socket.into_split();

            let mut connection =
                Connection::new(client_id, reader, writer, broadcast_tx, client_rx);

            tokio::spawn(async move {
                if let Err(errno) = connection.run().await {
                    eprintln!(
                        "[-] Connection error for client {}; Error = {:?} . . .",
                        client_id, errno
                    );
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
    fn new(
        client_id: usize,
        reader: R,
        writer: W,
        sender: mpsc::Sender<Message>,
        receiver: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        let reader = BufReader::new(reader);
        let writer = BufWriter::new(writer);

        Self {
            client_id,
            reader,
            writer,
            sender,
            receiver,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut buffer = [0u8; 1024];

        let welcome = format!("[+] Welcome! You are client {}\n", self.client_id);
        self.writer.write_all(welcome.as_bytes()).await?;
        self.writer.flush().await?;

        loop {
            tokio::select! {
                Some (message) = self.receiver.recv() => {
                    self.writer.write_all(&message).await?;
                    self.writer.write_all(b"\n").await?;
                    self.writer.flush().await?;
                }

                result = self.reader.read(&mut buffer) => {
                    match result {
                        Ok(0) => {
                            self.sender.send(Message::Disconnect(self.client_id)).await?;
                            return Ok(());
                        }

                        Ok (nbytes) => {
                            if let Ok(message) = String::from_utf8(buffer[..nbytes].to_vec()) {
                                let message = message.trim().to_string();
                                if !message.is_empty() {
                                    self.sender.send(Message::Chat {
                                        id: self.client_id,
                                        data: message.into(),
                                    }).await?;
                                }
                            }
                        }
                        Err (errno) => {
                            eprintln!("[-] Error reading from client {}: {:?}", self.client_id, errno);
                            return Err(errno.into());
                        }
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut server = Server::new().await?;
    server.run().await
}
