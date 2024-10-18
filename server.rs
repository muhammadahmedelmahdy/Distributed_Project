use tokio::net::UdpSocket;
use tokio::io::AsyncWriteExt;
use std::fs::File;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080...");

    let mut buffer = [0u8; 1024];
    let mut file = File::create("received_image.png").await?;

    loop {
        let (size, _addr) = socket.recv_from(&mut buffer).await?;
        
        if size == 0 {
            println!("No more data to receive.");
            break;
        }

        file.write_all(&buffer[..size]).await?;
        println!("Received {} bytes", size);
    }

    println!("Image successfully received.");
    Ok(())
}
