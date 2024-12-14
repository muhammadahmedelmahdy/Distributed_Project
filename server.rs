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
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use flate2::read::GzDecoder;
use std::io::Read;
use tokio::task;
use chrono::Utc;
use tokio::io::{self};
use tokio::net::TcpStream;
use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader as OtherBufReader};
use serde_json::Value;
use reqwest::Client;
use tokio::time;
const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";
static IS_LEADER: AtomicBool = AtomicBool::new(false);
#[derive(Serialize, Deserialize, Clone)]
struct Notification {
    image_owner: String,
    image_name: String,
    requester: String,
    access_rights: u32,
}

#[derive(Serialize, Deserialize, Clone)]
struct NotificationDirectory {
    notifications: HashMap<String, Vec<Notification>>, // Keyed by client ID
}

impl NotificationDirectory {
    fn new() -> Self {
        NotificationDirectory {
            notifications: HashMap::new(),
        }
    }

    fn load_from_file(file_path: &str) -> Self {
        let file = match std::fs::File::open(file_path) {
            Ok(file) => file,
            Err(_) => std::fs::File::create(file_path).unwrap(),
        };

        let reader = BufReader::new(file);
        serde_json::from_reader(reader).unwrap_or_else(|_| NotificationDirectory::new())
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

fn create_composite_image(
    images: &[String],
) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    const FONT_PATH: &str = "Roboto-Bold.ttf"; // Update to the correct font path
    let font_data = std::fs::read(FONT_PATH)?;
    let font = Font::try_from_vec(font_data).ok_or("Failed to load font")?;

    let mut decoded_images = Vec::new();
    for img_data in images {
        // Deserialize the image JSON string
        let image_json: serde_json::Value = serde_json::from_str(img_data)?;
        let image_name = image_json["name"].as_str().unwrap_or("Unknown");
        let client_id = image_json["client_id"].as_str().unwrap_or("Unknown");
        let image_base64 = image_json["data"].as_str().unwrap();
        let decoded_bytes = general_purpose::STANDARD.decode(image_base64)?;
        let decoded_image = image::load_from_memory(&decoded_bytes)?;

        decoded_images.push((client_id.to_string(), image_name.to_string(), decoded_image));
    }

    let total_width = decoded_images.iter().map(|(_, _, img)| img.width()).max().unwrap_or(0);
    let total_height: u32 = decoded_images.iter().map(|(_, _, img)| img.height()).sum();

    let mut composite = ImageBuffer::new(total_width, total_height);
    let mut y_offset = 0;

    for (client_id, image_name, img) in decoded_images {
        composite.copy_from(&img.to_rgba8(), 0, y_offset)?;

       // Draw client ID and image name line by line
       let scale = Scale { x: 20.0, y: 20.0 };
       let text_color = Rgba([255, 255, 255, 255]);
       let lines = vec![
           format!("Client: {}", client_id),
           format!("Name: {}", image_name),
       ];
       let mut text_y_offset = y_offset as i32 + 10; // Starting position for text

       for line in lines {
           draw_text_mut(
               &mut composite,
               text_color,
               10,
               text_y_offset,
               scale,
               &font,
               &line,
           );
           text_y_offset += scale.y as i32 + 5; // Add line height and some padding
       }

       y_offset += img.height();
    }

    Ok(DynamicImage::ImageRgba8(composite))
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let own_address = "10.7.17.50:8081";
    let peer_addresses = vec!["10.7.17.88:8080", "10.7.17.155:8082"];
    let socket = Arc::new(UdpSocket::bind(own_address).await?);
    let jsons_addresses = vec!["10.7.17.88:5000", "10.7.17.155:5002"];
    //  Directory::new().save_to_file("directory.json");
    // ClientDirectory::new().save_to_file("clients.json");

    // Load shared directory and client directory
    let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file("directory.json")));
    let client_directory: SharedClientDirectory = Arc::new(Mutex::new(ClientDirectory::load_from_file("clients.json")));
    println!("Server running at {}", own_address);

    // Leader Election Task
    let socket_clone = Arc::clone(&socket);
    let server_task = tokio::spawn(leader_election(socket_clone, peer_addresses));

    // Define addresses for the JSON listener
    let listener_addresses = "10.7.17.50:5001";

    // Spawn the listener task
    let listener_task = task::spawn(async move {
        if let Err(err) = listen_and_save_json(&listener_addresses).await {
            eprintln!("Error in listener task: {}", err);
        }
    });

    // DOS Server Task
    let ip = [10, 7, 17, 50];
    let port = 8084;
    let dos_task = run_dos(ip, port).await;

    // Sender Task (periodically sends JSON files to peers)
    let sender_task = tokio::spawn(async move {
        send_json_files_to_peers(
            &jsons_addresses,
            "directory.json",
            "clients.json"
        ).await
    });

   

    // Run all tasks concurrently
    tokio::try_join!(server_task, dos_task, listener_task,sender_task)?;

    Ok(())
}




pub async fn run_dos(ip: [u8; 4], port: u16) -> tokio::task::JoinHandle<()> {
    let file_path = "directory.json";
    let clients_file_path = "clients.json";
    let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file(file_path)));
    let client_directory: SharedClientDirectory = Arc::new(Mutex::new(ClientDirectory::load_from_file(clients_file_path)));
    let notifications_file_path = "notifications.json";
    let notification_directory: Arc<Mutex<NotificationDirectory>> =
        Arc::new(Mutex::new(NotificationDirectory::load_from_file(notifications_file_path)));
    // Directory::new().save_to_file("directory.json");
    // ClientDirectory::new().save_to_file("clients.json");

    let (notifier_tx, _) = broadcast::channel(100);
    tokio::spawn(async move {
        loop {
            let file_path = "directory.json";
            let clients_file_path = "clients.json";
            let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file(file_path)));
            let client_directory: SharedClientDirectory = Arc::new(Mutex::new(ClientDirectory::load_from_file(clients_file_path)));
           
    
            // Wait for 10 seconds before the next iteration
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
    

    
    let add_image = warp::path("add_image")
    .and(warp::post())
    .and(warp::body::json())
    .and(with_notifier(notifier_tx.clone()))
    .map(|body: HashMap<String, serde_json::Value>, notifier: broadcast::Sender<String>| {
        let mut directory = Directory::load_from_file("directory.json");
        let client_directory = ClientDirectory::load_from_file("clients.json");

        // Extract and validate required fields
        let client_id = body.get("client_id").and_then(|v| v.as_str()).unwrap_or_default();
        let password = body.get("password").and_then(|v| v.as_str()).unwrap_or_default();
        let image_name = body.get("image_name").and_then(|v| v.as_str()).unwrap_or_default();
        let image_data = body.get("image_data").and_then(|v| v.as_str()).unwrap_or_default();

        // Extract access_users as a map (client_id -> allowed views)
        let access_users: HashMap<String, u32> = match body.get("access_users") {
            Some(users) => serde_json::from_value(users.clone()).unwrap_or_else(|_| HashMap::new()),
            None => HashMap::new(),
        };

        // Authenticate client
        if let Some(client_info) = client_directory.clients.get(client_id) {
            if client_info.password != password {
                return warp::reply::json(&json!({ "error": "Authentication failed" }));
            }
        } else {
            return warp::reply::json(&json!({ "error": "Client ID not found" }));
        }

        // Add image to directory
        let images = directory.clients.entry(client_id.to_string()).or_insert_with(Vec::new);
        images.push(
            json!({
                "name": image_name,
                "data": image_data,
                "access_users": access_users
            })
            .to_string(),
        );

        // Save updated directory to file
        directory.save_to_file("directory.json");

        // Send notification
        let notification = format!("Client {} added image {}", client_id, image_name);
        let _ = notifier.send(notification.clone());

        warp::reply::json(&notification)
    });



    
    let delete_image = warp::path("delete_image")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_notifier(notifier_tx.clone()))
        .map(|body: HashMap<String, String>, notifier: broadcast::Sender<String>| {
            let mut directory = Directory::load_from_file("directory.json");
            let client_directory = ClientDirectory::load_from_file("clients.json");

            let client_id = body.get("client_id").unwrap();
            let password = body.get("password").unwrap();
            let image_name = body.get("image_name").unwrap();

            if let Some(client_info) = client_directory.clients.get(client_id) {
                if client_info.password != *password {
                    return warp::reply::json(&json!({ "error": "Authentication failed" }));
                }
            } else {
                return warp::reply::json(&json!({ "error": "Client ID not found" }));
            }

            if let Some(images) = directory.clients.get_mut(client_id) {
                if let Some(pos) = images.iter().position(|img| {
                    let image_data: serde_json::Value = serde_json::from_str(img).unwrap();
                    image_data["name"] == *image_name
                }) {
                    images.remove(pos);
                    directory.save_to_file("directory.json");
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
        .map(|| {
            let directory = Directory::load_from_file("directory.json");

            let mut all_images = Vec::new();
            for (client_id, images) in &directory.clients {
                println!("Client ID: {}", client_id);
                for image in images {
                    if let Ok(mut image_data) = serde_json::from_str::<serde_json::Value>(image) {
                        image_data["client_id"] = serde_json::Value::String(client_id.clone());
                        all_images.push(image_data.to_string());
                    }
                }
            }

            match create_composite_image(&all_images) {
                Ok(composite_image) => {
                    let mut buffer = Cursor::new(Vec::new());
                    if composite_image.write_to(&mut buffer, ImageOutputFormat::Png).is_err() {
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

    
    let list_by_client = warp::path("list_by_client")
    .and(warp::get())
    .and(warp::query::<HashMap<String, String>>())
    .map(|query: HashMap<String, String>| {
        let directory = Directory::load_from_file("directory.json"); // Dynamically load JSON on every request

        let default_client_id = String::new();
        let client_id = query.get("client_id").unwrap_or(&default_client_id);

        let images = match directory.clients.get(client_id) {
            Some(images) => {
                let mut client_images = Vec::new();
                for image in images {
                    if let Ok(mut image_data) = serde_json::from_str::<serde_json::Value>(image) {
                        image_data["client_id"] = serde_json::Value::String(client_id.clone());
                        client_images.push(image_data.to_string());
                    }
                }
                client_images
            }
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

      
        let fetch_clients = warp::path("fetch_clients")
    .and(warp::get())
    .map(|| {
        let client_directory = ClientDirectory::load_from_file("clients.json"); // Dynamically load JSON on every request

        let clients_with_ips: HashMap<String, Option<String>> = client_directory
            .clients
            .iter()
            .map(|(client_id, client_info)| {
                (client_id.clone(), client_info.current_ip.clone())
            })
            .collect();

        warp::reply::json(&clients_with_ips)
    });


    
    let login = warp::path("login")
    .and(warp::post())
    .and(warp::body::json())
    .map(|body: HashMap<String, String>| {
        // Dynamically load the client directory from the JSON file
        let client_directory = ClientDirectory::load_from_file("clients.json");

        // Extract client_id and password from the request body
        let default_client_id = String::new();
        let client_id = body.get("client_id").unwrap_or(&default_client_id);

        let default_password = String::new();
        let password = body.get("password").unwrap_or(&default_password);

        // Check if the client exists and validate the password
        if let Some(client_info) = client_directory.clients.get(client_id) {
            if &client_info.password == password {
                warp::reply::with_status(
                    warp::reply::json(&json!({
                        "message": "Login successful",
                        "client_id": client_id
                    })),
                    warp::http::StatusCode::OK,
                )
            } else {
                warp::reply::with_status(
                    warp::reply::json(&json!({
                        "error": "Invalid password"
                    })),
                    warp::http::StatusCode::UNAUTHORIZED,
                )
            }
        } else {
            warp::reply::with_status(
                warp::reply::json(&json!({
                    "error": "Client ID not found"
                })),
                warp::http::StatusCode::NOT_FOUND,
            )
        }
    });

       
        let register_client = warp::path("register_client")
    .and(warp::post())
    .and(warp::body::json())
    .map(|body: HashMap<String, String>| {
        // Dynamically load the JSON files
        let mut client_directory = ClientDirectory::load_from_file("clients.json");
        let mut directory = Directory::load_from_file("directory.json");

        let client_id = body.get("id").unwrap().to_string();
        let password = body.get("password").unwrap().to_string();

        // Check if the client already exists
        if client_directory.clients.contains_key(&client_id) {
            return warp::reply::json(&json!({
                "error": "Client ID already exists"
            }));
        }

        // Add the client to `clients.json`
        let client_info = ClientInfo {
            id: client_id.clone(),
            password,
            current_ip: None,
        };
        client_directory.clients.insert(client_id.clone(), client_info);
        client_directory.save_to_file("clients.json");

        // Add the client to `directory.json`
        directory.clients.entry(client_id.clone()).or_insert_with(Vec::new);
        directory.save_to_file("directory.json");

        warp::reply::json(&json!({
            "message": "Client registered successfully",
            "client_id": client_id
        }))
    });



let update_ip = warp::path("update_ip")
    .and(warp::post())
    .and(warp::body::json())
    .map(|body: HashMap<String, String>| {
        // Dynamically load the JSON file
        let mut client_directory = ClientDirectory::load_from_file("clients.json");

        let client_id = body.get("id").unwrap();
        let new_ip = body.get("current_ip").unwrap();

        // Update the client's IP address
        if let Some(client) = client_directory.clients.get_mut(client_id) {
            client.current_ip = Some(new_ip.clone());
            client_directory.save_to_file("clients.json");

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
    let remove_access = warp::path("remove_access")
    .and(warp::post())
    .and(warp::body::json())
    .map(|body: HashMap<String, serde_json::Value>| {
        // Extract required fields
        let client_id = body.get("client_id").and_then(|v| v.as_str()).unwrap_or_default();
        let password = body.get("password").and_then(|v| v.as_str()).unwrap_or_default();
        let image_name = body.get("image_name").and_then(|v| v.as_str()).unwrap_or_default();
        let users_to_remove: Vec<String> = match body.get("users_to_remove") {
            Some(users) => serde_json::from_value(users.clone()).unwrap_or_default(),
            None => Vec::new(),
        };

        // Load directories
        let mut directory = Directory::load_from_file("directory.json");
        let client_directory = ClientDirectory::load_from_file("clients.json");

        // Authenticate client
        if let Some(client_info) = client_directory.clients.get(client_id) {
            if client_info.password != password {
                return warp::reply::json(&json!({ "error": "Authentication failed" }));
            }
        } else {
            return warp::reply::json(&json!({ "error": "Client ID not found" }));
        }

        // Find the image
        if let Some(images) = directory.clients.get_mut(client_id) {
            if let Some(image_entry) = images.iter_mut().find(|img| {
                let image_data: serde_json::Value = serde_json::from_str(img).unwrap_or_default();
                image_data["name"] == image_name
            }) {
                let mut image_data: serde_json::Value = serde_json::from_str(image_entry).unwrap();

                // Remove users from access list
                if let Some(access_users) = image_data.get_mut("access_users").and_then(|v| v.as_object_mut()) {
                    for user in users_to_remove {
                        access_users.remove(&user);
                    }
                }

                *image_entry = serde_json::to_string(&image_data).unwrap();
                directory.save_to_file("directory.json");

                return warp::reply::json(&json!({
                    "message": "Users removed successfully from access list",
                    "client_id": client_id,
                    "image_name": image_name
                }));
            }
        }

        warp::reply::json(&json!({
            "error": format!("Image '{}' not found for client '{}'", image_name, client_id)
        }))
    });


    let modify_access = warp::path("modify_access")
    .and(warp::post())
    .and(warp::body::json())
    .map(|body: HashMap<String, serde_json::Value>| {
        // Extract required fields
        let client_id = body.get("client_id").and_then(|v| v.as_str()).unwrap_or_default();
        let password = body.get("password").and_then(|v| v.as_str()).unwrap_or_default();
        let image_name = body.get("image_name").and_then(|v| v.as_str()).unwrap_or_default();
        let access_rights: HashMap<String, u32> = match body.get("access_rights") {
            Some(rights) => serde_json::from_value(rights.clone()).unwrap_or_default(),
            None => HashMap::new(),
        };

        // Load directories
        let mut directory = Directory::load_from_file("directory.json");
        let client_directory = ClientDirectory::load_from_file("clients.json");

        // Authenticate client
        if let Some(client_info) = client_directory.clients.get(client_id) {
            if client_info.password != password {
                return warp::reply::json(&json!({ "error": "Authentication failed" }));
            }
        } else {
            return warp::reply::json(&json!({ "error": "Client ID not found" }));
        }

        // Find the image
        if let Some(images) = directory.clients.get_mut(client_id) {
            if let Some(image_entry) = images.iter_mut().find(|img| {
                let image_data: serde_json::Value = serde_json::from_str(img).unwrap_or_default();
                image_data["name"] == image_name
            }) {
                let mut image_data: serde_json::Value = serde_json::from_str(image_entry).unwrap();

                // Update or initialize access rights
                if let Some(existing_access) = image_data.get_mut("access_users").and_then(|v| v.as_object_mut()) {
                    // Add or update the provided access rights
                    for (other_client, allowed_views) in access_rights {
                        existing_access.insert(other_client, serde_json::Value::from(allowed_views));
                    }
                } else {
                    // Initialize access_users if it doesn't exist
                    let new_access: serde_json::Map<String, serde_json::Value> = access_rights
                        .into_iter()
                        .map(|(client, views)| (client, serde_json::Value::from(views)))
                        .collect();
                    image_data["access_users"] = serde_json::Value::Object(new_access);
                }

                // Save the updated image metadata
                *image_entry = serde_json::to_string(&image_data).unwrap();
                directory.save_to_file("directory.json");

                return warp::reply::json(&json!({
                    "message": "Access rights updated successfully",
                    "client_id": client_id,
                    "image_name": image_name
                }));
            }
        }

        warp::reply::json(&json!({
            "error": format!("Image '{}' not found for client '{}'", image_name, client_id)
        }))
    });

    let edit_views = warp::path("edit_views")
    .and(warp::post())
    .and(warp::body::json())
    .map(|body: HashMap<String, serde_json::Value>| {
        // Extract required fields
        let client_id = body.get("client_id").and_then(|v| v.as_str()).unwrap_or_default();
        let password = body.get("password").and_then(|v| v.as_str()).unwrap_or_default();
        let image_name = body.get("image_name").and_then(|v| v.as_str()).unwrap_or_default();
        let new_views: HashMap<String, u32> = match body.get("new_views") {
            Some(views) => serde_json::from_value(views.clone()).unwrap_or_default(),
            None => HashMap::new(),
        };

        // Load directories
        let mut directory = Directory::load_from_file("directory.json");
        let client_directory = ClientDirectory::load_from_file("clients.json");

        // Authenticate client
        if let Some(client_info) = client_directory.clients.get(client_id) {
            if client_info.password != password {
                return warp::reply::json(&json!({ "error": "Authentication failed" }));
            }
        } else {
            return warp::reply::json(&json!({ "error": "Client ID not found" }));
        }

        // Find the image
        if let Some(images) = directory.clients.get_mut(client_id) {
            if let Some(image_entry) = images.iter_mut().find(|img| {
                let image_data: serde_json::Value = serde_json::from_str(img).unwrap_or_default();
                image_data["name"] == image_name
            }) {
                let mut image_data: serde_json::Value = serde_json::from_str(image_entry).unwrap();

                // Update views for existing users
                if let Some(access_users) = image_data.get_mut("access_users").and_then(|v| v.as_object_mut()) {
                    for (user, views) in new_views {
                        if access_users.contains_key(&user) {
                            access_users.insert(user, serde_json::Value::from(views));
                        }
                    }
                }

                *image_entry = serde_json::to_string(&image_data).unwrap();
                directory.save_to_file("directory.json");

                return warp::reply::json(&json!({
                    "message": "Number of views updated successfully",
                    "client_id": client_id,
                    "image_name": image_name
                }));
            }
        }

        warp::reply::json(&json!({
            "error": format!("Image '{}' not found for client '{}'", image_name, client_id)
        }))
    });
    let get_access = warp::path("get_access")
    .and(warp::get())
    .and(warp::query::<HashMap<String, String>>())
    .map(|query: HashMap<String, String>| {
        // Create persistent bindings for default values
        let default_client_id = String::new();
        let default_password = String::new();
        let default_image_name = String::new();

        // Extract required fields from the query parameters
        let client_id = query.get("client_id").unwrap_or(&default_client_id);
        let password = query.get("password").unwrap_or(&default_password);
        let image_name = query.get("image_name").unwrap_or(&default_image_name);

        // Load directories
        let directory = Directory::load_from_file("directory.json");
        let client_directory = ClientDirectory::load_from_file("clients.json");

        // Authenticate client
        if let Some(client_info) = client_directory.clients.get(client_id) {
            if client_info.password != *password {
                return warp::reply::json(&json!({ "error": "Authentication failed" }));
            }
        } else {
            return warp::reply::json(&json!({ "error": "Client ID not found" }));
        }

        // Find the image and return its access rights
        if let Some(images) = directory.clients.get(client_id) {
            if let Some(image_entry) = images.iter().find(|img| {
                let image_data: serde_json::Value = serde_json::from_str(img).unwrap_or_default();
                image_data["name"] == *image_name
            }) {
                let image_data: serde_json::Value = serde_json::from_str(image_entry).unwrap();
                if let Some(access_rights) = image_data.get("access_users") {
                    return warp::reply::json(&json!({
                        "access_rights": access_rights,
                        "client_id": client_id,
                        "image_name": image_name
                    }));
                } else {
                    return warp::reply::json(&json!({
                        "error": "No access rights found for this image"
                    }));
                }
            }
        }

        warp::reply::json(&json!({
            "error": format!("Image '{}' not found for client '{}'", image_name, client_id)
        }))
    });

    let add_notification = warp::path("add_notification")
    .and(warp::post())
    .and(warp::body::json())
    .map(|body: HashMap<String, serde_json::Value>| {
        // Dynamically load the notification directory
        let notifications_file_path = "notifications.json";
        let mut notification_directory = NotificationDirectory::load_from_file(notifications_file_path);

        // Extract required fields
        let image_owner = body
            .get("image_owner")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let image_name = body
            .get("image_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let requester = body
            .get("requester")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let access_rights = body
            .get("access_rights")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        // Create a new notification
        let notification = Notification {
            image_owner: image_owner.to_string(),
            image_name: image_name.to_string(),
            requester: requester.to_string(),
            access_rights,
        };

        // Add the notification to the directory
        notification_directory
            .notifications
            .entry(image_owner.to_string())
            .or_insert_with(Vec::new)
            .push(notification);

        // Save the updated directory back to the file
        notification_directory.save_to_file(notifications_file_path);

        warp::reply::json(&json!({ "message": "Notification added successfully" }))
    });
    let get_notifications = warp::path("get_notifications")
    .and(warp::get())
    .and(warp::query::<HashMap<String, String>>())
    .map(|query: HashMap<String, String>| {
        // Dynamically load the notification directory
        let notifications_file_path = "notifications.json";
        let notification_directory = NotificationDirectory::load_from_file(notifications_file_path);

        // Create a persistent binding for the default value
        let default_client_id = String::new();
        let client_id = query.get("client_id").unwrap_or(&default_client_id);

        // Retrieve notifications for the client
        if let Some(notifications) = notification_directory.notifications.get(client_id) {
            let result: HashMap<String, Vec<serde_json::Value>> = notifications
                .iter()
                .group_by(|n| n.requester.clone())
                .into_iter()
                .map(|(requester, group)| {
                    let data = group
                        .map(|n| {
                            json!({
                                "image": n.image_name,
                                "access_rights": n.access_rights,
                            })
                        })
                        .collect();
                    (requester, data)
                })
                .collect();

            warp::reply::json(&result)
        } else {
            warp::reply::json(&json!({ "error": "No notifications found" }))
        }
    });








 
    let routes = register_client
        .or(fetch_clients)
        .or(update_ip)
        .or(add_image)
        .or(delete_image)
        .or(list_all)
        .or(list_by_client)
        .or(modify_access)
        .or(edit_views)
        .or(remove_access)
        .or(get_access)
        .or(get_notifications)
        .or(add_notification)
        .or(login);

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


const MAX_UDP_PAYLOAD: usize = 4096; // Maximum safe size for UDP payloads

// /// Sends updates to all peers in smaller chunks if necessary.
// async fn send_update_to_peers(peers: &[&str], socket: &Arc<UdpSocket>, file_name: &str, data: &str) {
//     let data_bytes = data.as_bytes();
//     let data_len = data_bytes.len();
//     let chunks = (data_len + MAX_UDP_PAYLOAD - 1) / MAX_UDP_PAYLOAD; // Calculate number of chunks

//     for peer in peers {
//         for chunk_id in 0..chunks {
//             let start = chunk_id * MAX_UDP_PAYLOAD;
//             let end = std::cmp::min(start + MAX_UDP_PAYLOAD, data_len);
//             let chunk_data = &data_bytes[start..end];

//             let chunk_message = json!({
//                 "type": "update",
//                 "file": file_name,
//                 "chunk_id": chunk_id,
//                 "total_chunks": chunks,
//                 "data": base64::encode(chunk_data)
//             })
//             .to_string();

//             if let Err(err) = socket.send_to(chunk_message.as_bytes(), peer).await {
//                 eprintln!("Failed to send {} chunk {} to {}: {}", file_name, chunk_id, peer, err);
//             } else {
//                 println!("Sent {} chunk {} to {}", file_name, chunk_id, peer);
//             }
//         }
//     }
// }
async fn send_json_files_to_peers(
    peers: &[&str], 
    directory_path: &str, 
    clients_path: &str
) -> io::Result<()> {
    loop {
        
            println!("This instance is the leader. Sending JSON files to peers...");

            // Read directory.json
            let mut dir_file = File::open(directory_path).await?;
            let mut dir_json = String::new();
            dir_file.read_to_string(&mut dir_json).await?;
            println!("Read directory.json successfully!");

            // Read clients.json
            let mut clients_file = File::open(clients_path).await?;
            let mut clients_json = String::new();
            clients_file.read_to_string(&mut clients_json).await?;
            println!("Read clients.json successfully!");
            
            // Iterate through peers and send JSON files
            for &addr in peers {
                match TcpStream::connect(addr).await {
                    Ok(mut stream) => {
                        println!("Connected to {}", addr);

                        // Send directory.json with file_name
                        let directory_message = json!({
                            "file_name": "directory.json",
                            "data": dir_json
                        })
                        .to_string();
                        println!("Sending directory.json to {}...", addr);
                        if let Err(err) = stream.write_all(directory_message.as_bytes()).await {
                            eprintln!("Failed to send directory.json to {}: {}", addr, err);
                            continue;
                        }
                        stream.write_all(b"\n").await?;

                        // Send clients.json with file_name
                        let clients_message = json!({
                            "file_name": "clients.json",
                            "data": clients_json
                        })
                        .to_string();
                        println!("Sending clients.json to {}...", addr);
                        if let Err(err) = stream.write_all(clients_message.as_bytes()).await {
                            eprintln!("Failed to send clients.json to {}: {}", addr, err);
                            continue;
                        }
                        stream.write_all(b"\n").await?;

                        println!("JSON files sent to {}", addr);
                    }
                    Err(err) => {
                        eprintln!("Failed to connect to {}: {}", addr, err);
                    }
                }
            }
        

        time::sleep(Duration::from_secs(10)).await;
    }
}




pub async fn listen_and_save_json(address: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let listener = TcpListener::bind(address).await?;
    println!("Listening on {}", address);

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);

        tokio::spawn(async move {
            let mut reader = OtherBufReader::new(socket);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                match serde_json::from_str::<Value>(&line) {
                    Ok(json) => {
                        if let Some(file_name) = json.get("file_name").and_then(|v| v.as_str()) {
                            if let Some(data) = json.get("data").and_then(|v| v.as_str()) {
                                println!("Received JSON for file: {}", file_name);

                                if let Err(err) = append_json_to_file(file_name, data).await {
                                    eprintln!("Failed to append JSON to {}: {}", file_name, err);
                                }
                            } else {
                                eprintln!("Missing 'data' field in received JSON");
                            }
                        } else {
                            eprintln!("Missing 'file_name' field in received JSON");
                        }
                    }
                    Err(err) => {
                        eprintln!("Failed to parse JSON: {}", err);
                    }
                }
                line.clear();
            }
        });
    }
}

async fn save_json_to_file(filename: &str, content: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut file = File::create(filename).await?;
    file.write_all(content.as_bytes()).await?;
    println!("Saved JSON to {}", filename);
    Ok(())
}
async fn append_json_to_file(filename: &str, new_content: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Step 1: Read the existing file content, if the file exists
    let mut existing_content = String::new();
    if let Ok(mut file) = File::open(filename).await {
        file.read_to_string(&mut existing_content).await?;
    }

    // Step 2: Parse the existing and new content as JSON values
    let mut existing_json: Value = serde_json::from_str(&existing_content).unwrap_or_else(|_| json!({}));
    let new_json: Value = serde_json::from_str(new_content)?;

    // Step 3: Merge the new JSON into the existing JSON with conflict resolution
    merge_json(&mut existing_json, &new_json);

    // Step 4: Write the updated JSON back to the file
    let mut file = File::create(filename).await?;
    file.write_all(existing_json.to_string().as_bytes()).await?;
    println!("Appended new JSON data to {}", filename);

    Ok(())
}

fn merge_json(existing_json: &mut Value, new_json: &Value) {
    match (existing_json, new_json) {
        // Case 1: Both are objects - merge keys recursively
        (Value::Object(existing_map), Value::Object(new_map)) => {
            for (key, new_value) in new_map {
                match existing_map.get_mut(key) {
                    Some(existing_value) => {
                        merge_json(existing_value, new_value); // Recursive merge for nested structures
                    }
                    None => {
                        existing_map.insert(key.clone(), new_value.clone());
                    }
                }
            }
        }
        // Case 2: Both are arrays - concatenate unique elements
        (Value::Array(existing_array), Value::Array(new_array)) => {
            for new_item in new_array {
                if !existing_array.contains(new_item) {
                    existing_array.push(new_item.clone());
                }
            }
        }
        // Case 3: Conflicting types or simple values - new value overwrites the old
        (existing_value, new_value) => {
            *existing_value = new_value.clone();
        }
    }
}




// async fn synchronize_files(socket: Arc<UdpSocket>, chunk_buffer: ChunkBuffer) {
//     let mut buffer = [0; 4096]; // Increased buffer size

//     loop {
//         match socket.recv_from(&mut buffer).await {
//             Ok((len, addr)) => {
//                 let message = String::from_utf8_lossy(&buffer[..len]);
//                 match serde_json::from_str::<serde_json::Value>(&message) {
//                     Ok(sync_data) => {
//                         if let (Some(file), Some(chunk_index), Some(total_chunks), Some(chunk_data)) =
//                             (
//                                 sync_data["file"].as_str(),
//                                 sync_data["chunk"].as_u64(),
//                                 sync_data["total_chunks"].as_u64(),
//                                 sync_data["data"].as_str(),
//                             )
//                         {
//                             let mut buffer = chunk_buffer.lock().await;

//                             let file_chunks = buffer
//                                 .entry(file.to_string())
//                                 .or_insert_with(HashMap::new);

//                             file_chunks.insert(chunk_index as usize, Some(chunk_data.to_string()));

//                             if file_chunks.len() == total_chunks as usize
//                                 && (0..total_chunks).all(|i| file_chunks.contains_key(&(i as usize)))
//                             {
//                                 let complete_data: String = file_chunks
//                                     .iter()
//                                     .sorted_by_key(|&(index, _)| index)
//                                     .filter_map(|(_, chunk)| chunk.clone())
//                                     .collect();

//                                 println!("Received complete file: {}", file);

//                                 match file {
//                                     "directory.json" => { /* Process directory.json */ }
//                                     "clients.json" => { /* Process clients.json */ }
//                                     _ => eprintln!("Unknown file type received: {}", file),
//                                 }

//                                 buffer.remove(file);
//                             } else {
//                                 println!("Waiting for more chunks for file: {}", file);
//                             }
//                         } else {
//                             eprintln!("Invalid or incomplete sync data: {}", message);
//                         }
//                     }
//                     Err(err) => {
//                         eprintln!(
//                             "Failed to parse synchronization message from {}: {}. Error: {}",
//                             addr, message, err
//                         );
//                     }
//                 }
//             }
//             Err(err) => {
//                 eprintln!("Error receiving data on synchronization socket: {}", err);
//             }
//         }
//     }
// }

const API_BASE_URL: &str = "http://127.0.0.1:3030";
const LOCAL_SAVE_PATH: &str = "temp_received.json";
/// Downloads files using the `receive-file` endpoint
async fn download_files(client: &Client) -> Result<(), Box<dyn Error>> {
    let response = client
        .post(format!("{}/receive-file", API_BASE_URL))
        .json(&json!({ "save_path": LOCAL_SAVE_PATH }))
        .send()
        .await?;

    if response.status().is_success() {
        println!("Files downloaded successfully.");
    } else {
        eprintln!("Failed to download files: {:?}", response.text().await?);
    }
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
