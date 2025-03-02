use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::select;
use tokio::signal;
use tokio::sync::mpsc;

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let stream = TcpStream::connect("127.0.0.1:12100").await?;
    println!("Connected to server!");

    // Split the stream into read and write parts
    let (mut reader, mut writer) = stream.into_split();

    // Channel for stdin input
    let (tx, mut rx) = mpsc::channel::<String>(32);

    // Spawn task to read from stdin
    tokio::spawn(async move {
        let mut stdin_reader = BufReader::new(tokio::io::stdin());
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = stdin_reader.read_line(&mut line).await.unwrap();

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

    // Spawn task to read from socket
    let socket_reader_handle = tokio::spawn(async move {
        let mut buffer = vec![0; 1024];

        loop {
            match reader.read(&mut buffer).await {
                Ok(0) => {
                    println!("Server closed the connection");
                    break;
                }
                Ok(n) => {
                    // Print the message from server
                    if let Ok(message) = String::from_utf8(buffer[..n].to_vec()) {
                        print!("{}", message); // No need for newline as server adds it
                    } else {
                        eprintln!("Received invalid UTF-8 data");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read from socket: {}", e);
                    break;
                }
            }
        }
    });

    // Main loop for handling user input and shutdown signal
    loop {
        select! {
            maybe_line = rx.recv() => {
                match maybe_line {
                    Some(line) => {
                        let line_with_newline = line + "\n";
                        if let Err(e) = writer.write_all(line_with_newline.as_bytes()).await {
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

    // Gracefully shutdown the writer
    let shutdown_result = tokio::time::timeout(Duration::from_secs(5), writer.shutdown()).await;
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

    // Wait for the socket reader to complete
    if let Err(e) = socket_reader_handle.await {
        eprintln!("Error waiting for socket reader: {}", e);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run().await
}