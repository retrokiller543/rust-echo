use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::select;
use tokio::signal;
use tokio::sync::mpsc;

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:12100").await?;
    println!("Connected to server!");

    let (tx, mut rx) = mpsc::channel::<String>(32);

    tokio::spawn(async move {
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await.unwrap();

            if bytes_read == 0 {
                println!("EOF reached on stdin, closing input.");
                break;
            }

            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }

            if let Err(e) = tx.send(line.clone()).await {
                eprintln!("Failed to send line to channel: {}", e);
                break;
            }
        }
        println!("Stdin reader task finished.");
    });

    loop {
        select! {
            maybe_line = rx.recv() => {
                match maybe_line {
                    Some(line) => {
                        let line_with_newline = line + "\n";
                        if let Err(e) = stream.write_all(line_with_newline.as_bytes()).await {
                            eprintln!("Failed to write to socket: {}", e);
                            break;
                        }
                    }
                    None => {
                        println!("Channel closed, exiting main loop.");
                        break;
                    }
                }
            }

            _ = signal::ctrl_c() => {
                println!("Received Ctrl+C, shutting down...");
                break;
            }
        }
    }

    println!("Closing connection.");

    let shutdown_result = tokio::time::timeout(Duration::from_secs(5), stream.shutdown()).await;

    match shutdown_result {
        Ok(Ok(_)) => {
            println!("Shutdown completed successfully.");
        }
        Ok(Err(e)) => {
            eprintln!("Shutdown failed: {}", e);
        }
        Err(_elapsed) => {
            eprintln!("Shutdown timed out after 5 seconds.");
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run().await
}
