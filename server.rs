use sysinfo::{CpuExt, System, SystemExt};
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration, sleep};
use std::error::Error;
use std::sync::Arc;
use std::collections::HashMap;
use steganography::util::{file_as_dynamic_image, save_image_buffer};
use steganography::encoder::*;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs::{File as OtherFile, OpenOptions};
use std::io::{BufReader, BufWriter};
use warp::{Filter, reply};
use warp::ws::{WebSocket, Message};
use tokio::sync::broadcast;
use futures_util::{StreamExt, SinkExt};
use serde_json::json;
use image::{DynamicImage, Rgba, ImageBuffer, GenericImage};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};
use base64::{engine::general_purpose, Engine};
use std::sync::Mutex;
use image::ImageOutputFormat;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex as new_Mutex; // Import tokio's Mutex instead of std::sync::Mutex
use itertools::Itertools;

const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";
static IS_LEADER: AtomicBool = AtomicBool::new(false);
#[derive(Serialize, Deserialize, Clone)]
struct Directory {
    clients: HashMap<String, Vec<String>>,
}
impl Directory {
    fn new() -> Self {
        Directory {
            clients: HashMap::new(),
        }
    }

    fn load_from_file(file_path: &str) -> Self {
        let file = match std::fs::File::open(file_path) {
            Ok(file) => file,
            Err(_) => std::fs::File::create(file_path).unwrap(),
        };

        let reader = BufReader::new(file);
        serde_json::from_reader(reader).unwrap_or_else(|_| Directory::new())
    }

    fn clear(&mut self) {
        self.clients.clear();
    }

    fn save_to_file(&self, file_path: &str) {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)
            .unwrap();
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self).unwrap();
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct ClientInfo {
    id: String,
    password: String,
    current_ip: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ClientDirectory {
    clients: HashMap<String, ClientInfo>,
}

impl ClientDirectory {
    fn new() -> Self {
        ClientDirectory {
            clients: HashMap::new(),
        }
    }

    fn load_from_file(file_path: &str) -> Self {
        let file = match std::fs::File::open(file_path) {
            Ok(file) => file,
            Err(_) => std::fs::File::create(file_path).unwrap(),
        };

        let reader = BufReader::new(file);
        serde_json::from_reader(reader).unwrap_or_else(|_| ClientDirectory::new())
    }

    fn save_to_file(&self, file_path: &str) {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)
            .unwrap();
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self).unwrap();
    }
}

type SharedClientDirectory = Arc<Mutex<ClientDirectory>>;
type SharedDirectory = Arc<Mutex<Directory>>;
fn with_directory(
    directory: SharedDirectory,
) -> impl Filter<Extract = (SharedDirectory,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || directory.clone())
}
fn with_client_directory(
    client_directory: SharedClientDirectory,
) -> impl Filter<Extract = (SharedClientDirectory,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || client_directory.clone())
}
fn with_notifier(
    notifier: broadcast::Sender<String>,
) -> impl Filter<Extract = (broadcast::Sender<String>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || notifier.clone())
}
async fn handle_ws_connection(ws: WebSocket, mut rx: broadcast::Receiver<String>) {
    let (mut ws_tx, _ws_rx) = ws.split();
    while let Ok(msg) = rx.recv().await {
        if ws_tx.send(Message::text(msg)).await.is_err() {
            break; // Exit if the WebSocket connection is closed
        }
    }
}
fn create_composite_image(images: &[String]) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    const FONT_PATH: &str = "/home/muhammadelmahdy/Distributed_Project/Roboto-Bold.ttf"; // Update to point to a valid .ttf font
    let font_data = std::fs::read(FONT_PATH)?;
    let font = Font::try_from_vec(font_data).ok_or("Failed to load font")?;

    let mut decoded_images = Vec::new();
    for img_data in images {
        let image_json: serde_json::Value = serde_json::from_str(img_data)?;
        let image_name = image_json["name"].as_str().unwrap_or("Unknown");
        let image_base64 = image_json["data"].as_str().unwrap();
        let decoded_bytes = general_purpose::STANDARD.decode(image_base64)?;
        let decoded_image = image::load_from_memory(&decoded_bytes)?;

        decoded_images.push((image_name.to_string(), decoded_image));
    }

    let total_width = decoded_images.iter().map(|(_, img)| img.width()).max().unwrap_or(0);
    let total_height: u32 = decoded_images.iter().map(|(_, img)| img.height()).sum();

    let mut composite = ImageBuffer::new(total_width, total_height);
    let mut y_offset = 0;

    for (image_name, img) in decoded_images {
        composite.copy_from(&img.to_rgba8(), 0, y_offset)?;
        let scale = Scale { x: 20.0, y: 20.0 };
        let text_color = Rgba([255, 255, 255, 255]);
        draw_text_mut(&mut composite, text_color, 10, y_offset as i32 + 10, scale, &font, &image_name);

        y_offset += img.height();
    }

    Ok(DynamicImage::ImageRgba8(composite))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let own_address = "127.0.0.1:8081";
    let peer_addresses = vec!["127.0.0.1:8080", "127.0.0.1:8082"];
    let socket = Arc::new(UdpSocket::bind(own_address).await?);

    // Load shared directory and client directory
    let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file("directory.json")));
    let client_directory: SharedClientDirectory = Arc::new(Mutex::new(ClientDirectory::load_from_file("clients.json")));
    println!("Server running at {}", own_address);

    // Leader Election Task
    let socket_clone = Arc::clone(&socket);
    let server_task = tokio::spawn(leader_election(socket_clone, peer_addresses));

    // Synchronization Socket
    let sync_socket = Arc::new(UdpSocket::bind("127.0.0.1:5002").await?); // Ensure this port is available

    // Initialize the chunk buffer for synchronization
    let chunk_buffer: ChunkBuffer = Arc::new(new_Mutex::new(HashMap::new())); // Use tokio::sync::Mutex

    let sync_task = tokio::spawn(synchronize_files(sync_socket, chunk_buffer.clone()));

    // DOS Server Task
    let ip = [127, 0, 0, 1];
    let port = 8084;
    let dos_task = run_dos(ip, port).await;

    // // Periodic Synchronization Task for JSONs
    // let jsons_task = tokio::spawn(periodic_synchronize(
    //     vec!["127.0.0.1:8080", "127.0.0.1:8082"], // List of peers
    //     Arc::clone(&socket),                     // Shared socket
    //     Arc::clone(&directory),                  // Shared directory
    //     Arc::clone(&client_directory),           // Shared client directory
    // ));

    // Run all tasks concurrently
    tokio::try_join!(server_task, dos_task, sync_task)?;

    Ok(())
}




pub async fn run_dos(ip: [u8; 4], port: u16) -> tokio::task::JoinHandle<()> {
    let file_path = "directory.json";
    let clients_file_path = "clients.json";
    let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file(file_path)));
    let client_directory: SharedClientDirectory = Arc::new(Mutex::new(ClientDirectory::load_from_file(clients_file_path)));
    //  Directory::new().save_to_file("directory.json");
    // ClientDirectory::new().save_to_file("clients.json");

    let (notifier_tx, _) = broadcast::channel(100);

    // Define routes
    let add_image = warp::path("add_image")
    .and(warp::post())
    .and(warp::body::json())
    .and(with_directory(directory.clone()))
    .and(with_client_directory(client_directory.clone()))
    .and(with_notifier(notifier_tx.clone()))
    .map(|body: HashMap<String, String>, dir: SharedDirectory, client_dir: SharedClientDirectory, notifier: broadcast::Sender<String>| {
        let client_id = body.get("client_id").unwrap();
        let password = body.get("password").unwrap();
        let image_name = body.get("image_name").unwrap();
        let image_data = body.get("image_data").unwrap();

        // Authenticate the client
        let client_dir = client_dir.lock().unwrap();
        if let Some(client_info) = client_dir.clients.get(client_id) {
            if client_info.password != *password {
                return warp::reply::json(&json!({
                    "error": "Authentication failed"
                }));
            }
        } else {
            return warp::reply::json(&json!({
                "error": "Client ID not found"
            }));
        }

        // Add the image to the directory
        let mut dir = dir.lock().unwrap();
        let images = dir.clients.entry(client_id.clone()).or_insert_with(Vec::new);
        images.push(
            json!({
                "name": image_name,
                "data": image_data
            })
            .to_string(),
        );

        dir.save_to_file("directory.json");
        let notification = format!("Client {} added image {}", client_id, image_name);
        let _ = notifier.send(notification.clone());
        

        warp::reply::json(&notification)
    });

    let delete_image = warp::path("delete_image")
    .and(warp::post())
    .and(warp::body::json())
    .and(with_directory(directory.clone()))
    .and(with_client_directory(client_directory.clone()))
    .and(with_notifier(notifier_tx.clone()))
    .map(|body: HashMap<String, String>, dir: SharedDirectory, client_dir: SharedClientDirectory, notifier: broadcast::Sender<String>| {
        let client_id = body.get("client_id").unwrap();
        let password = body.get("password").unwrap();
        let image_name = body.get("image_name").unwrap();

        // Authenticate the client
        let client_dir = client_dir.lock().unwrap();
        if let Some(client_info) = client_dir.clients.get(client_id) {
            if client_info.password != *password {
                return warp::reply::json(&json!({
                    "error": "Authentication failed"
                }));
            }
        } else {
            return warp::reply::json(&json!({
                "error": "Client ID not found"
            }));
        }

        // Delete the image from the directory
        let mut dir = dir.lock().unwrap();
        if let Some(images) = dir.clients.get_mut(client_id) {
            if let Some(pos) = images.iter().position(|img| {
                let image_data: serde_json::Value = serde_json::from_str(img).unwrap();
                image_data["name"] == *image_name
            }) {
                images.remove(pos);
                dir.save_to_file("directory.json");
                let notification = json!({
                    "message": format!("Client {} deleted image {}", client_id, image_name),
                    "client_id": client_id,
                    "image_name": image_name
                });
                let _ = notifier.send(notification.to_string());
                return warp::reply::json(&notification);
            }
        }

        warp::reply::json(&json!({
            "error": format!("Image {} not found for client {}", image_name, client_id)
        }))
    });


    let list_all = warp::path("list_all")
        .and(warp::get())
        .and(with_directory(directory.clone()))
        .map(|dir: SharedDirectory| {
            let dir = dir.lock().unwrap();
            warp::reply::json(&dir.clients)
        });

    let list_by_client = warp::path("list_by_client")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(with_directory(directory.clone()))
        .map(|query: HashMap<String, String>, dir: SharedDirectory| {
            let default_client_id = String::from("");
            let client_id = query.get("client_id").unwrap_or(&default_client_id);

            let dir = dir.lock().unwrap();

            let images = match dir.clients.get(client_id) {
                Some(images) => images.clone(),
                None => {
                    return warp::http::Response::builder()
                        .status(404)
                        .header("Content-Type", "text/plain")
                        .body("No images found".to_string().into_bytes())
                        .unwrap();
                }
            };

            match create_composite_image(&images) {
                Ok(composite_image) => {
                    let mut buffer = Cursor::new(Vec::new());
                    if composite_image
                        .write_to(&mut buffer, ImageOutputFormat::Png)
                        .is_err()
                    {
                        return warp::http::Response::builder()
                            .status(500)
                            .header("Content-Type", "text/plain")
                            .body("Failed to encode composite image".to_string().into_bytes())
                            .unwrap();
                    }

                    warp::http::Response::builder()
                        .header("Content-Type", "image/png")
                        .body(buffer.into_inner())
                        .unwrap()
                }
                Err(e) => warp::http::Response::builder()
                    .status(500)
                    .header("Content-Type", "text/plain")
                    .body(format!("Failed to create composite image: {}", e).into_bytes())
                    .unwrap(),
            }
        });
        // Register a new client
    let register_client = warp::path("register_client")
    .and(warp::post())
    .and(warp::body::json())
    .and(with_client_directory(client_directory.clone()))
    .map(|body: HashMap<String, String>, client_dir: SharedClientDirectory| {
        let client_id = body.get("id").unwrap().to_string();
        let password = body.get("password").unwrap().to_string();

        let mut dir = client_dir.lock().unwrap();

        if dir.clients.contains_key(&client_id) {
            return warp::reply::json(&json!({
                "error": "Client ID already exists"
            }));
        }

        let client_info = ClientInfo {
            id: client_id.clone(),
            password,
            current_ip: None,
        };

        dir.clients.insert(client_id.clone(), client_info);
        dir.save_to_file("clients.json");

        warp::reply::json(&json!({
            "message": "Client registered successfully",
            "client_id": client_id
        }))
    });

// Update client's current IP
let update_ip = warp::path("update_ip")
    .and(warp::post())
    .and(warp::body::json())
    .and(with_client_directory(client_directory.clone()))
    .map(|body: HashMap<String, String>, client_dir: SharedClientDirectory| {
        let client_id = body.get("id").unwrap();
        let new_ip = body.get("current_ip").unwrap();

        let mut dir = client_dir.lock().unwrap();

        if let Some(client) = dir.clients.get_mut(client_id) {
            client.current_ip = Some(new_ip.clone());
            dir.save_to_file("clients.json");

            return warp::reply::json(&json!({
                "message": "IP updated successfully",
                "client_id": client_id,
                "current_ip": new_ip
            }));
        }

        warp::reply::json(&json!({
            "error": "Client ID not found"
        }))
    });
 
    let routes = register_client
        .or(update_ip)
        .or(add_image)
        .or(delete_image)
        .or(list_all)
        .or(list_by_client);

    tokio::spawn(async move {
            loop {
                if is_leader() {
                    warp::serve(routes.clone()).run((ip, port)).await;
                } else {
                    println!("Not the leader. Waiting for leadership...");
                    sleep(Duration::from_secs(5)).await;
                }
            }
        })
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
        let random_number = rand::thread_rng().gen_range(0..=10);
        
        // Simulate failure if the random number is 5
        if random_number == 5 {
            println!("{}: Simulated failure occurred, skipping this election round.", own_address);
            sleep(Duration::from_secs(10)).await;  // Adjust sleep duration as needed
            continue;
        }


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
                    set_leader_status(true); // This server is the leader
                    leader_count += 1;
                    if let Some(client) = client_addr {
                        socket.send_to(format!("LEADER,{}", own_address).as_bytes(), client).await?;
                        println!("{} (leader) sent leader confirmation to client at {}", own_address, client);
                    }
                }
                else {
                    set_leader_status(false); // This server is not the leader
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
fn set_leader_status(is_leader: bool) {
    IS_LEADER.store(is_leader, Ordering::SeqCst);
}

fn is_leader() -> bool {
    IS_LEADER.load(Ordering::SeqCst)
}


const MAX_UDP_PAYLOAD: usize = 1024; // Maximum safe size for UDP payloads

/// Sends updates to all peers in smaller chunks if necessary.
async fn send_update_to_peers(peers: &[&str], socket: &Arc<UdpSocket>, file_name: &str, data: &str) {
    let data_bytes = data.as_bytes();
    let data_len = data_bytes.len();
    let chunks = (data_len + MAX_UDP_PAYLOAD - 1) / MAX_UDP_PAYLOAD; // Calculate number of chunks

    for peer in peers {
        for chunk_id in 0..chunks {
            let start = chunk_id * MAX_UDP_PAYLOAD;
            let end = std::cmp::min(start + MAX_UDP_PAYLOAD, data_len);
            let chunk_data = &data_bytes[start..end];

            let chunk_message = json!({
                "type": "update",
                "file": file_name,
                "chunk_id": chunk_id,
                "total_chunks": chunks,
                "data": base64::encode(chunk_data)
            })
            .to_string();

            if let Err(err) = socket.send_to(chunk_message.as_bytes(), peer).await {
                eprintln!("Failed to send {} chunk {} to {}: {}", file_name, chunk_id, peer, err);
            } else {
                println!("Sent {} chunk {} to {}", file_name, chunk_id, peer);
            }
        }
    }
}
async fn periodic_synchronize(
    peers: Vec<&str>,
    socket: Arc<UdpSocket>,
    directory: SharedDirectory,
    client_directory: SharedClientDirectory,
) {
    loop {
        let dir_data = serde_json::to_string(&*directory.lock().unwrap()).unwrap_or_default();
        let client_data = serde_json::to_string(&*client_directory.lock().unwrap()).unwrap_or_default();

        send_update_to_peers(&peers, &socket, "directory.json", &dir_data).await;
        send_update_to_peers(&peers, &socket, "clients.json", &client_data).await;

        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

type ChunkBuffer = Arc<new_Mutex<HashMap<String, HashMap<usize, Option<String>>>>>;
async fn synchronize_files(socket: Arc<UdpSocket>, chunk_buffer: ChunkBuffer) {
    let mut buffer = [0; 1024];

    loop {
        match socket.recv_from(&mut buffer).await {
            Ok((len, addr)) => {
                let message = String::from_utf8_lossy(&buffer[..len]);
                match serde_json::from_str::<serde_json::Value>(&message) {
                    Ok(sync_data) => {
                        if let Some(file) = sync_data["file"].as_str() {
                            let chunk_index = sync_data["chunk"].as_u64().unwrap_or(0) as usize;
                            let total_chunks = sync_data["total_chunks"].as_u64().unwrap_or(1) as usize;
                            let chunk_data = sync_data["data"].as_str().unwrap_or("").to_string();

                            let mut buffer = chunk_buffer.lock().await;

                            // Initialize the buffer for this file if it doesn't exist
                            let file_chunks = buffer
                                .entry(file.to_string())
                                .or_insert_with(HashMap::new);

                            // Insert the received chunk
                            file_chunks.insert(chunk_index, Some(chunk_data));

                            // Check if all chunks are received
                            if file_chunks.len() == total_chunks
                                && file_chunks.values().all(|chunk| chunk.is_some())
                            {
                                let complete_data: String = file_chunks
                                    .iter()
                                    .sorted_by_key(|&(index, _)| index)
                                    .filter_map(|(_, chunk)| chunk.clone())
                                    .collect();

                                println!("Received complete file: {}", file);

                                // Process the complete file
                                match file {
                                    "directory.json" => {
                                        if let Ok(dir_clients) = serde_json::from_str::<HashMap<String, Vec<String>>>(&complete_data) {
                                            let mut dir = Directory::new();
                                            dir.clients = dir_clients;
                                            dir.save_to_file("directory.json");
                                            println!("Synchronized directory.json from {}", addr);
                                        } else {
                                            eprintln!("Failed to parse directory.json data");
                                        }
                                    }
                                    "clients.json" => {
                                        if let Ok(client_data) = serde_json::from_str::<HashMap<String, ClientInfo>>(&complete_data) {
                                            let mut client_dir = ClientDirectory::new();
                                            client_dir.clients = client_data;
                                            client_dir.save_to_file("clients.json");
                                            println!("Synchronized clients.json from {}", addr);
                                        } else {
                                            eprintln!("Failed to parse clients.json data");
                                        }
                                    }
                                    _ => {
                                        eprintln!("Unknown file type received: {}", file);
                                    }
                                }

                                // Clear the buffer for this file after processing
                                buffer.remove(file);
                            }
                        } else {
                            eprintln!("Missing 'file' field in sync data: {}", message);
                        }
                    }
                    Err(err) => {
                        eprintln!(
                            "Failed to parse synchronization message from {}: {}. Error: {}",
                            addr, message, err
                        );
                    }
                }
            }
            Err(err) => {
                eprintln!("Error receiving data on synchronization socket: {}", err);
            }
        }
    }
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