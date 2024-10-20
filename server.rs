use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080...");

    loop {
        let mut buffer = [0u8; 1024];
        let mut file = None; // File will be initialized when we receive the "START" signal

        loop {
            let (size, _addr) = socket.recv_from(&mut buffer).await?;

            // Check for "START" signal
            if &buffer[..size] == b"START" {
                // Create/truncate the file only when "START" signal is received
                file = Some(File::create("received_image.png").await?);
                println!("Received start of transfer signal. File created.");
                continue;
            }

            // Check for "END" signal
            if &buffer[..size] == b"END" {
                println!("End of image transfer signal received.");
                break;
            }

            // Write the received data to the file if it exists
            if let Some(f) = &mut file {
                f.write_all(&buffer[..size]).await?;
                println!("Received and wrote {} bytes to file", size);
            } else {
                println!("No file open. Waiting for START signal.");
            }
        }

        println!("Image transfer completed and saved as 'received_image.png'.");
    }
}
