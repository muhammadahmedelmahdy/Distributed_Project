use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?; // Bind to any available port for client
    let server_addr = "127.0.0.1:8080";
    
    let image_path = Path::new("hitler.jpeg");
    let mut file = File::open(&image_path).await?;
    println!("Opened image: {:?}", image_path);

    // Send "START" signal before sending the image
    socket.send_to(b"START", server_addr).await?;
    println!("Sent start of transfer signal.");
    
    let mut buffer = [0u8; 1024];
    let mut total_bytes_sent = 0;

    // Read and send the file data in chunks
    loop {
        let size = file.read(&mut buffer).await?;

        if size == 0 {
            println!("Finished reading the image.");
            break;
        }

        socket.send_to(&buffer[..size], server_addr).await?;
        total_bytes_sent += size;
        println!("Sent {} bytes", size);
    }

    // Send "END" signal to tell the server that the image transfer is complete
    socket.send_to(b"END", server_addr).await?;
    println!("Sent end of transfer signal. Total bytes sent: {}", total_bytes_sent);

    Ok(())
}
