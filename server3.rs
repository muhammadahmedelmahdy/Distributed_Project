use sysinfo::{CpuExt, System, SystemExt};
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use std::error::Error;
use std::sync::Arc;
use std::collections::HashMap;
use steganography::util::{file_as_dynamic_image, save_image_buffer};
use steganography::encoder::*;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let own_address = "127.0.0.1:8082";
    let peer_addresses = vec!["127.0.0.1:8080", "127.0.0.1:8081"];
    let socket = Arc::new(UdpSocket::bind(own_address).await?);
    println!("Server running at {}", own_address);

    let socket_clone = Arc::clone(&socket);
    let server_task = tokio::spawn(leader_election(socket_clone, peer_addresses));

    tokio::try_join!(server_task)?;

    Ok(())
}

async fn leader_election(
    socket: Arc<UdpSocket>,
    peers: Vec<&str>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut buffer = [0; 1024];
    let mut cpu_data: HashMap<String, (f32, u32)> = HashMap::new();
    let own_address = socket.local_addr()?.to_string();
    let mut leader_count: u32 = 0;
    let mut client_addr: Option<std::net::SocketAddr> = None;

    loop {
        let (len, addr) = socket.recv_from(&mut buffer).await?;
        let message = String::from_utf8_lossy(&buffer[..len]);

        if message == "REQUEST_LEADER" {
            client_addr = Some(addr);
            println!("{} received REQUEST_LEADER from client {}", own_address, addr);

            let mut sys = System::new_all();
            sys.refresh_cpu();
            let own_cpu_usage = sys.global_cpu_info().cpu_usage();
            println!("{} calculated CPU usage: {:.8}%", own_address, own_cpu_usage);

            let broadcast_message = format!("{},{},{}", own_address, own_cpu_usage, leader_count);
            for &peer in &peers {
                socket.send_to(broadcast_message.as_bytes(), peer).await?;
                println!("{} sent CPU usage and leader count to {}", own_address, peer);
            }

            cpu_data.insert(own_address.clone(), (own_cpu_usage, leader_count));

            for _ in 0..peers.len() {
                if let Ok(Ok((len, peer_addr))) = timeout(Duration::from_secs(2), socket.recv_from(&mut buffer)).await {
                    let received = String::from_utf8_lossy(&buffer[..len]);
                    let parts: Vec<&str> = received.split(',').collect();

                    if parts.len() == 3 {
                        let peer_address = parts[0].to_string();
                        if let (Ok(cpu_usage), Ok(peer_leader_count)) = (parts[1].parse::<f32>(), parts[2].parse::<u32>()) {
                            cpu_data.insert(peer_address.clone(), (cpu_usage, peer_leader_count));
                            println!("{} received CPU usage and leader count from {}: {:.8}% and {}", own_address, peer_address, cpu_usage, peer_leader_count);
                        }
                    } else {
                        println!("{} received an invalid message from {}: {}", own_address, peer_addr, received);
                    }
                } else {
                    println!("{}: Timeout waiting for response from peers", own_address);
                    break;
                }
            }

            if let Some((leader_address, &(leader_cpu, _))) = cpu_data.iter().min_by(|a, b| {
                match a.1.0.partial_cmp(&b.1.0).unwrap() {
                    std::cmp::Ordering::Equal => match a.1.1.cmp(&b.1.1) {
                        std::cmp::Ordering::Equal => a.0.cmp(&b.0),
                        other => other,
                    },
                    other => other,
                }
            }) {
                println!("{} elected {} as leader with CPU usage: {:.8}% and leader count: {}", own_address, leader_address, leader_cpu, cpu_data[leader_address].1);
            
                if &own_address == leader_address {
                    leader_count += 1;
                    if let Some(client) = client_addr {
                        socket.send_to(format!("LEADER,{}", own_address).as_bytes(), client).await?;
                        println!("{} (leader) sent leader confirmation to client at {}", own_address, client);
                    }
                }
            }

            cpu_data.clear();
        } else if message == "IMAGE_TRANSFER" {
            println!("{} received IMAGE_TRANSFER request from {}", own_address, addr);
            receive_image(Arc::clone(&socket), addr).await?;
            encode_received_image().await?; // Call encoding after receiving the image
            send_encoded_image(Arc::clone(&socket), addr).await?; // Send encoded image back to client

        }
    }
}

async fn receive_image(socket: Arc<UdpSocket>, src: std::net::SocketAddr) -> tokio::io::Result<()> {
    let mut file = File::create("received_image_1.jpg").await?;
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

    Ok(())
}

async fn encode_received_image() -> tokio::io::Result<()> {
    let received_image_path = "received_image_1.jpg";
    let mask_image_path = "mask.jpeg";
    let encoded_image_path = "encrypted_image_1.png";

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
    let encoded_image_path = "encrypted_image_1.png";
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