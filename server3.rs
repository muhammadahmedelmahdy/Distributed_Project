use tokio::net::UdpSocket;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::fs::File;
use tokio::time::{timeout, Duration};
use rand::Rng;
use steganography::encoder::*;
use steganography::util::{file_as_dynamic_image, save_image_buffer};

const SERVER_ADDR: &str = "127.0.0.1:8082"; // in server3.rs

static IS_LEADER: AtomicBool = AtomicBool::new(false);
const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";
const SERVER_ADDRS: [&str; 3] = ["127.0.0.1:8080", "127.0.0.1:8081", "127.0.0.1:8082"];

// Function to run the server logic
async fn run_operations(server_addr: &str, is_active: Arc<AtomicBool>) -> tokio::io::Result<()> {
    let socket = Arc::new(UdpSocket::bind(server_addr).await?);
    println!("Server {}: Waiting for client request...", server_addr);

    let mut buffer = [0u8; CHUNK_SIZE + 4];

    loop {
        // Only process requests if the server is active
        if is_active.load(Ordering::SeqCst) {
            let (bytes_received, src) = socket.recv_from(&mut buffer).await?;
            let message = String::from_utf8_lossy(&buffer[..bytes_received]);

            choose_leader_per_request();

            if message == "REQUEST_LEADER" && IS_LEADER.load(Ordering::SeqCst) {
                println!("Server {}: Confirming leader role to client.", server_addr);
                socket.send_to(b"LEADER_CONFIRM", src).await?;
                continue;
            }

            if bytes_received > 0 && &buffer[..bytes_received] == b"IMAGE_TRANSFER" {
                println!("Server {}: Starting image transfer from client.", server_addr);
                receive_image(socket.clone(), src).await?;
            }
        } else {
            println!("Server is in failed state, ignoring requests.");
            tokio::time::sleep(Duration::from_secs(1)).await; // Prevent busy-waiting
        }
    }
}

// Function to simulate failure
async fn simulate_failure(is_active: Arc<AtomicBool>) {
    thread::spawn(move || {
        let mut rng = rand::thread_rng();
        loop {
            let failure_duration = rng.gen_range(2..6);
            let active_duration = rng.gen_range(5..15);
            is_active.store(false, Ordering::SeqCst);
            println!("Simulating server failure...");
            thread::sleep(Duration::from_secs(failure_duration));
            is_active.store(true, Ordering::SeqCst);
            println!("Server back online.");
            thread::sleep(Duration::from_secs(active_duration));
        }
    });
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let is_active = Arc::new(AtomicBool::new(true));
    
    // Start the failure simulation
    simulate_failure(is_active.clone()).await;

    // Run the server logic in a new thread
    let is_active_clone = is_active.clone();
    let operations_thread = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(run_operations(SERVER_ADDR, is_active_clone)).unwrap();
    });

    operations_thread.join().unwrap(); // Wait for the server thread to finish

    Ok(())
}

fn choose_leader_per_request() {
    let leader_index = rand::thread_rng().gen_range(0..SERVER_ADDRS.len());
    if SERVER_ADDR == SERVER_ADDRS[leader_index] {
        IS_LEADER.store(true, Ordering::SeqCst);
        println!("Server {}: Elected as the leader for this request.", SERVER_ADDR);
    } else {
        IS_LEADER.store(false, Ordering::SeqCst);
        println!("Server {}: Not elected as the leader for this request.", SERVER_ADDR);
    }
}

async fn receive_image(socket: Arc<UdpSocket>, src: std::net::SocketAddr) -> tokio::io::Result<()> {
    let mut file = File::create("received_image_3.jpg").await?;
    let mut expected_chunk: u32 = 0;
    let mut buffer = [0u8; CHUNK_SIZE + 4];

    loop {
        let (bytes_received, _) = socket.recv_from(&mut buffer).await?;
        if bytes_received == 3 && &buffer[..3] == b"END" {
            println!("Server: Image transfer complete.");
            break;
        }

        if bytes_received < 4 {
            println!("Server: Received a malformed packet, skipping.");
            continue;
        }

        let chunk_number = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        let data = &buffer[4..bytes_received];

        if chunk_number == expected_chunk {
            println!("Server: Writing chunk {}", chunk_number);
            file.write_all(data).await?;
            expected_chunk += 1;
        }

        socket.send_to(ACK, src).await?;
        println!("Server: Sent ACK for chunk {}", chunk_number);
    }

    // Encode and send the encoded image back to the client
    encode_received_image().await?;
    send_encoded_image(socket, src).await?;

    Ok(())
}

async fn encode_received_image() -> tokio::io::Result<()> {
    let received_image_path = "received_image_3.jpg";
    let mask_image_path = "/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/mask.jpeg";
    let encoded_image_path = "encrypted_image_3.png";

    let mask_image = file_as_dynamic_image(mask_image_path.to_string());

    let mut received_file = File::open(received_image_path).await?;
    let mut received_bytes = Vec::new();
    received_file.read_to_end(&mut received_bytes).await?;

    let encoder = Encoder::new(&received_bytes, mask_image);
    let encoded_image = encoder.encode_alpha();

    save_image_buffer(encoded_image, encoded_image_path.to_string());
    println!("Server: Encoded received image and saved as {}", encoded_image_path);

    Ok(())
}

async fn send_encoded_image(socket: Arc<UdpSocket>, dest: std::net::SocketAddr) -> tokio::io::Result<()> {
    let encoded_image_path = "encrypted_image_3.png";
    let mut file = File::open(encoded_image_path).await?;
    let mut buffer = [0u8; CHUNK_SIZE];
    let mut chunk_number: u32 = 0;

    println!("Server: Sending encoded image to client...");

    while let Ok(bytes_read) = file.read(&mut buffer).await {
        if bytes_read == 0 {
            break;
        }

        let packet = [&chunk_number.to_be_bytes(), &buffer[..bytes_read]].concat();
        socket.send_to(&packet, dest).await?;
        println!("Server: Sent chunk {}", chunk_number);

        // Wait for acknowledgment
        let mut ack_buf = [0; 3];
        match timeout(Duration::from_secs(5), socket.recv_from(&mut ack_buf)).await {
            Ok(Ok((_, _))) if &ack_buf == ACK => {
                println!("Server: Received ACK for chunk {}", chunk_number);
            }
            _ => {
                println!("Server: No ACK received for chunk {}, retrying...", chunk_number);
                continue; // Retry sending this chunk
            }
        }

        chunk_number += 1;
    }

    // Send end-of-transfer signal
    socket.send_to(b"END", dest).await?;
    println!("Server: Encoded image transfer complete.");

    Ok(())
}
