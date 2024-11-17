use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use warp::{Filter, reply};
use warp::ws::{WebSocket, Message};
use tokio::sync::broadcast;
use futures_util::{StreamExt, SinkExt};

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
        let file = File::open(file_path).unwrap_or_else(|_| File::create(file_path).unwrap());
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).unwrap_or_else(|_| Directory::new())
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

type SharedDirectory = Arc<Mutex<Directory>>;

fn with_directory(
    directory: SharedDirectory,
) -> impl Filter<Extract = (SharedDirectory,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || directory.clone())
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

#[tokio::main]
async fn main() {
    let file_path = "directory.json";

    // Load or initialize the directory
    let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file(file_path)));

    // Create a broadcast channel for notifications
    let (notifier_tx, _) = broadcast::channel(100);

    // WebSocket endpoint for real-time notifications
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_notifier(notifier_tx.clone()))
        .map(|ws: warp::ws::Ws, notifier: broadcast::Sender<String>| {
            ws.on_upgrade(move |socket| handle_ws_connection(socket, notifier.subscribe()))
        });

    // Add an image for a client
    let add_image = warp::path("add_image")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_directory(directory.clone()))
        .and(with_notifier(notifier_tx.clone()))
        .map(|body: HashMap<String, String>, dir: SharedDirectory, notifier: broadcast::Sender<String>| {
            let client_id = body.get("client_id").unwrap();
            let image_name = body.get("image_name").unwrap();
            let mut dir = dir.lock().unwrap();

            let images = dir.clients.entry(client_id.clone()).or_insert_with(Vec::new);
            images.push(image_name.clone());

            dir.save_to_file("directory.json");
            let notification = format!("Client {} added image {}", client_id, image_name);
            let _ = notifier.send(notification.clone());
            
            reply::json(&notification)
        });

    // Delete an image for a client
    let delete_image = warp::path("delete_image")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_directory(directory.clone()))
        .and(with_notifier(notifier_tx.clone()))
        .map(|body: HashMap<String, String>, dir: SharedDirectory, notifier: broadcast::Sender<String>| {
            let client_id = body.get("client_id").unwrap();
            let image_name = body.get("image_name").unwrap();
            let mut dir = dir.lock().unwrap();

            if let Some(images) = dir.clients.get_mut(client_id) {
                if let Some(pos) = images.iter().position(|x| x == image_name) {
                    images.remove(pos);
                    dir.save_to_file("directory.json");
                    let notification = format!("Client {} deleted image {}", client_id, image_name);
                    let _ = notifier.send(notification.clone());
                    return reply::json(&notification);
                }
            }

            reply::json(&format!("Image {} not found for client {}", image_name, client_id))
        });

    // List all clients and their images
    let list_all = warp::path("list_all")
        .and(warp::get())
        .and(with_directory(directory.clone()))
        .map(|dir: SharedDirectory| {
            let dir = dir.lock().unwrap();
            reply::json(&dir.clients)
        });

    // Combine all routes
    let routes = add_image.or(delete_image).or(list_all).or(ws_route);

    // Start the server
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
