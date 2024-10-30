use sysinfo::{CpuExt, System, SystemExt};
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use std::error::Error;
use std::sync::Arc;
use std::collections::HashMap;


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Define the IP addresses for each server
    let own_address = "127.0.0.1:8081"; // Adjust this for each server instance (e.g., 8081, 8082)
    let peer_addresses = vec!["127.0.0.1:8080", "127.0.0.1:8082"];

    let socket = Arc::new(UdpSocket::bind(own_address).await?);
    println!("Server running at {}", own_address);

    // Leader count for this server
    let mut leader_count = 0;

    // Task to handle incoming requests and broadcasts
    let socket_clone = Arc::clone(&socket);
    let server_task = tokio::spawn(leader_election(socket_clone, peer_addresses, leader_count));

    tokio::try_join!(server_task)?;

    Ok(())
}

async fn leader_election(
    socket: Arc<UdpSocket>,
    peers: Vec<&str>,
    mut leader_count: u32,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut buffer = [0; 1024];
    let mut cpu_data: HashMap<String, (f32, u32)> = HashMap::new(); // Map to store (CPU, leader count) for each server
    let own_address = socket.local_addr()?.to_string();

    loop {
        // Step 1: Wait for a client request
        let (len, addr) = socket.recv_from(&mut buffer).await?;
        let message = String::from_utf8_lossy(&buffer[..len]);

        if message == "GET_CPU_USAGE" {
            // Step 2: Calculate CPU usage upon client request
            let mut sys = System::new_all();
            sys.refresh_cpu();
            let own_cpu_usage = sys.global_cpu_info().cpu_usage();
            println!("{} calculated CPU usage: {:.8}%", own_address, own_cpu_usage);

            // Step 3: Broadcast our CPU utilization and leader count to all other servers
            let broadcast_message = format!("{},{},{}", own_address, own_cpu_usage, leader_count);
            for &peer in &peers {
                socket.send_to(broadcast_message.as_bytes(), peer).await?;
                println!("{} sent CPU usage and leader count to {}", own_address, peer);
            }

            // Step 4: Add our own CPU usage and leader count to the map
            cpu_data.insert(own_address.clone(), (own_cpu_usage, leader_count));

            // Step 5: Wait with a timeout to collect CPU data and leader count from all servers (including itself)
            for _ in 0..peers.len() {
                if let Ok(Ok((len, peer_addr))) = timeout(Duration::from_secs(2), socket.recv_from(&mut buffer)).await {
                    let received = String::from_utf8_lossy(&buffer[..len]);
                    let parts: Vec<&str> = received.split(',').collect();

                    if parts.len() == 3 {
                        let peer_address = parts[0].to_string();
                        if let (Ok(cpu_usage), Ok(peer_leader_count)) = (parts[1].parse::<f32>(), parts[2].parse::<u32>()) {
                            // Insert or update CPU data and leader count in the map
                            cpu_data.insert(peer_address.clone(), (cpu_usage, peer_leader_count));
                            println!("{} received CPU usage and leader count from {}: {:.8}% and {}", own_address, peer_address, cpu_usage, peer_leader_count);
                        }
                    } else {
                        println!("{} received an invalid message from {}: {}", own_address, peer_addr, received);
                    }
                } else {
                    println!("Timeout waiting for response from peers");
                    break;
                }
            }

            // Step 6: Determine the leader based on the provided rules
            if let Some((leader_address, &(leader_cpu, _))) = cpu_data.iter().min_by(|a, b| {
                match a.1.0.partial_cmp(&b.1.0).unwrap() {
                    std::cmp::Ordering::Equal => {
                        // CPU utilization is equal, compare leader counts
                        match a.1.1.cmp(&b.1.1) {
                            std::cmp::Ordering::Equal => {
                                // Leader count is also equal, apply tie-breaking rule by IP address
                                a.0.cmp(&b.0)
                            }
                            other => other,
                        }
                    }
                    other => other,
                }
            }) {
                println!("{} has elected {} as the leader with CPU usage: {:.8}% and leader count: {}", own_address, leader_address, leader_cpu, cpu_data[leader_address].1);

                // Increment our leader count if we are the elected leader
                if &own_address == leader_address {
                    leader_count += 1;
                }

                // Broadcast leader information to peers and client
                broadcast_leader(&socket, &peers, &addr, leader_address).await?;
            }

            // Clear CPU data for the next request
            cpu_data.clear();
        }
    }
}

// Function to broadcast leader information
async fn broadcast_leader(
    socket: &UdpSocket,
    peers: &[&str],
    client_addr: &std::net::SocketAddr,
    leader_address: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let leader_message = format!("LEADER,{}", leader_address);
    for &peer in peers {
        socket.send_to(leader_message.as_bytes(), peer).await?;
        println!("Broadcasted leader information to peer: {}", peer);
    }

    // Send leader info to the client that requested it
    socket.send_to(leader_message.as_bytes(), client_addr).await?;
    println!("Sent leader information to client: {}", client_addr);

    Ok(())
}

