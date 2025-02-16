use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};

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
        Self { reader, writer }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:12100").await?;

    loop {
        let (mut socket, _address) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buffer = [0; 1024];

            loop {
                let data = match socket.read(&mut buffer).await {
                    Ok(0) => return,
                    Ok(data) => data,
                    Err(errno) => {
                        eprintln!("[-] Failed to read from socket; Error = {:?} . . .", errno);
                        return;
                    }
                };

                eprintln!("[+] Data received {} . . .", std::str::from_utf8(&buffer).unwrap());

                if let Err(errno) = socket.write_all(&buffer[0..data]).await {
                    eprintln!("[-] Failed to write to socket; Error = {:?} . . .", errno);
                }
            }
        });
    }
}
