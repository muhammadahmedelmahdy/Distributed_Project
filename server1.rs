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
use rand::Rng;
use tokio::time::sleep;
use tokio::sync::Mutex;

const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let own_address = "127.0.0.1:8080";
    let peer_addresses = vec!["127.0.0.1:8081", "127.0.0.1:8082"];
    let socket = Arc::new(UdpSocket::bind(own_address).await?);
    println!("Server running at {}", own_address);

    let election_socket = Arc::clone(&socket);
    let request_socket = Arc::clone(&socket);

    let cpu_data = Arc::new(Mutex::new(HashMap::new()));
    let leader_count = Arc::new(Mutex::new(0u32));
    let handling_request = Arc::new(Mutex::new(false));

    // Start tasks for heartbeat sending and client request handling
    let election_task = tokio::spawn(heartbeat_task(
        election_socket,
        peer_addresses.clone(),
        cpu_data.clone(),
        leader_count.clone(),
    ));
    let client_task = tokio::spawn(client_request_handler(
        request_socket,
        cpu_data.clone(),
        handling_request.clone(),
        peer_addresses.clone(),
    ));

    tokio::try_join!(election_task, client_task)?;

    Ok(())
}

// Task to send heartbeat messages periodically
async fn heartbeat_task(
    socket: Arc<UdpSocket>,
    peers: Vec<&str>,
    cpu_data: Arc<Mutex<HashMap<String, (f32, u32)>>>,
    leader_count: Arc<Mutex<u32>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let own_address = socket.local_addr()?.to_string();
    
    loop {
        let mut sys = System::new_all();
        sys.refresh_cpu();
        let own_cpu_usage = sys.global_cpu_info().cpu_usage();

        {
            // Update own CPU data
            let mut cpu_data = cpu_data.lock().await;
            cpu_data.insert(own_address.clone(), (own_cpu_usage, *leader_count.lock().await));
        }

        let heartbeat_message = format!("{},{},{}", own_address, own_cpu_usage, *leader_count.lock().await);
        for &peer in &peers {
            socket.send_to(heartbeat_message.as_bytes(), peer).await?;
            println!("{} sent heartbeat to {}", own_address, peer);
        }

        // Send heartbeats every 10 seconds
        sleep(Duration::from_secs(10)).await;
    }
}

async fn client_request_handler(
    socket: Arc<UdpSocket>,
    cpu_data: Arc<Mutex<HashMap<String, (f32, u32)>>>,
    handling_request: Arc<Mutex<bool>>,
    peers: Vec<&str>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut buffer = [0; 1024];
    let own_address = socket.local_addr()?.to_string();

    loop {
        let (len, addr) = socket.recv_from(&mut buffer).await?;
        let message = String::from_utf8_lossy(&buffer[..len]);

        // Step 1: Check if the message is a heartbeat message
        let parts: Vec<&str> = message.split(',').collect();
        if parts.len() == 3 {
            // Parse the heartbeat components: "<address>,<cpu_usage>,<leader_count>"
            if let (Ok(cpu_usage), Ok(peer_leader_count)) = (parts[1].parse::<f32>(), parts[2].parse::<u32>()) {
                let peer_address = parts[0].to_string();
                
                // Update the `cpu_data` map with the peer's CPU usage and leader count
                let mut cpu_data = cpu_data.lock().await;
                cpu_data.insert(peer_address.clone(), (cpu_usage, peer_leader_count));
                
                println!("Received heartbeat from {} with CPU usage: {:.2}% and leader count: {}", peer_address, cpu_usage, peer_leader_count);
                continue; // Skip further processing for heartbeat messages
            }
        }

        // Step 2: Process other types of messages
        match message.as_ref() {
            // Handle election trigger from client
            "IMAGE_TRANSFER" => {
                let mut handling = handling_request.lock().await;
                if *handling {
                    println!("{} is already handling a request, ignoring new request.", own_address);
                    continue;
                }

                // Start election process based on the latest CPU usage data
                let cpu_data = cpu_data.lock().await;
                if let Some((leader_address, &(leader_cpu, _))) = cpu_data.iter().min_by(|a, b| a.1.0.partial_cmp(&b.1.0).unwrap()) {
                    if &own_address == leader_address {
                        println!("{} elected as leader with CPU usage: {:.2}%", own_address, leader_cpu);
                        *handling = true; // Mark as handling the request

                        // Notify client that this server is the leader
                        socket.send_to(format!("LEADER,{}", own_address).as_bytes(), addr).await?;
                        println!("{} notified client as leader", own_address);

                        // Handle the image transfer
                        receive_image(Arc::clone(&socket), addr).await?;
                        encode_received_image().await?;
                        send_encoded_image(Arc::clone(&socket), addr).await?;

                        // Reset handling state after completing the request
                        *handling = false;
                    }
                }
            }
            // Unknown or malformed requests
            _ => println!("Received unknown request from {}: {}", addr, message),
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
    let mask_image_path = "mask.jpg";
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
                continue;
            }
        }

        chunk_number += 1;
    }

    socket.send_to(b"END", dest).await?;
    println!("Server: Encoded image transfer complete.");

    Ok(())
}
