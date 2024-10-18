use tokio::net::UdpSocket;
use tokio::io::AsyncReadExt;
use std::fs::File;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let server_addr = "127.0.0.1:8080";
    
    let image_path = Path::new("image_to_send.png");
    let mut file = File::open(&image_path).await?;
    
    let mut buffer = [0u8; 1024];

    loop {
        let size = file.read(&mut buffer).await?;

        if size == 0 {
            println!("Finished sending the image.");
            break;
        }

        socket.send_to(&buffer[..size], server_addr).await?;
        println!("Sent {} bytes", size);
    }

    Ok(())
}
