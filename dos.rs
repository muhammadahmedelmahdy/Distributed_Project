// use serde::{Deserialize, Serialize};
// use std::collections::HashMap;
// use std::sync::{Arc, Mutex};
// use std::fs::{File, OpenOptions};
// use std::io::{BufReader, BufWriter};
// use warp::{Filter, reply};
// use warp::ws::{WebSocket, Message};
// use tokio::sync::broadcast;
// use futures_util::{StreamExt, SinkExt};
// use serde_json::json;
// #[derive(Serialize, Deserialize, Clone)]
// struct Directory {
//     clients: HashMap<String, Vec<String>>,
// }

// impl Directory {
//     fn new() -> Self {
//         Directory {
//             clients: HashMap::new(),
//         }
//     }

//     fn load_from_file(file_path: &str) -> Self {
//         let file = File::open(file_path).unwrap_or_else(|_| File::create(file_path).unwrap());
//         let reader = BufReader::new(file);
//         serde_json::from_reader(reader).unwrap_or_else(|_| Directory::new())
//     }
//     fn clear(&mut self) {
//         self.clients.clear();
//     }

//     fn save_to_file(&self, file_path: &str) {
//         let file = OpenOptions::new()
//             .write(true)
//             .create(true)
//             .truncate(true)
//             .open(file_path)
//             .unwrap();
//         let writer = BufWriter::new(file);
//         serde_json::to_writer(writer, &self).unwrap();
//     }
// }

// type SharedDirectory = Arc<Mutex<Directory>>;

// fn with_directory(
//     directory: SharedDirectory,
// ) -> impl Filter<Extract = (SharedDirectory,), Error = std::convert::Infallible> + Clone {
//     warp::any().map(move || directory.clone())
// }

// fn with_notifier(
//     notifier: broadcast::Sender<String>,
// ) -> impl Filter<Extract = (broadcast::Sender<String>,), Error = std::convert::Infallible> + Clone {
//     warp::any().map(move || notifier.clone())
// }

// async fn handle_ws_connection(ws: WebSocket, mut rx: broadcast::Receiver<String>) {
//     let (mut ws_tx, _ws_rx) = ws.split();
//     while let Ok(msg) = rx.recv().await {
//         if ws_tx.send(Message::text(msg)).await.is_err() {
//             break; // Exit if the WebSocket connection is closed
//         }
//     }
// }

// #[tokio::main]
// async fn main() {
//     let file_path = "directory.json";

//     // Load or initialize the directory
//     let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file(file_path)));

//     // directory.lock().unwrap().clear();
//     // directory.lock().unwrap().save_to_file(file_path);
//     // println!("Directory cleared on startup.");

//     // Create a broadcast channel for notifications
//     let (notifier_tx, _) = broadcast::channel(100);

//     // WebSocket endpoint for real-time notifications
//     let ws_route = warp::path("ws")
//         .and(warp::ws())
//         .and(with_notifier(notifier_tx.clone()))
//         .map(|ws: warp::ws::Ws, notifier: broadcast::Sender<String>| {
//             ws.on_upgrade(move |socket| handle_ws_connection(socket, notifier.subscribe()))
//         });

//     // Add an image for a client
//     let add_image = warp::path("add_image")
//     .and(warp::post())
//     .and(warp::body::json())
//     .and(with_directory(directory.clone()))
//     .and(with_notifier(notifier_tx.clone()))
//     .map(|body: HashMap<String, String>, dir: SharedDirectory, notifier: broadcast::Sender<String>| {
//         let client_id = body.get("client_id").unwrap();
//         let image_name = body.get("image_name").unwrap();
//         let image_data = body.get("image_data").unwrap();
//         let mut dir = dir.lock().unwrap();

//         let images = dir.clients.entry(client_id.clone()).or_insert_with(Vec::new);
//         images.push(json!({
//             "name": image_name,
//             "data": image_data // Store the Base64-encoded image data
//         }).to_string());

//         dir.save_to_file("directory.json");
//         let notification = format!("Client {} added image {}", client_id, image_name);
//         let _ = notifier.send(notification.clone());
        
//         reply::json(&notification)
//     });


//     // Delete an image for a client
//     let delete_image = warp::path("delete_image")
//     .and(warp::post())
//     .and(warp::body::json())
//     .and(with_directory(directory.clone()))
//     .and(with_notifier(notifier_tx.clone()))
//     .map(|body: HashMap<String, String>, dir: SharedDirectory, notifier: broadcast::Sender<String>| {
//         let client_id = body.get("client_id").unwrap();
//         let image_name = body.get("image_name").unwrap();
//         let mut dir = dir.lock().unwrap();

//         if let Some(images) = dir.clients.get_mut(client_id) {
//             // Find the index of the image to delete based on the `name` field
//             if let Some(pos) = images.iter().position(|img| {
//                 let image_data: serde_json::Value = serde_json::from_str(img).unwrap();
//                 image_data["name"] == *image_name
//             }) {
//                 images.remove(pos); // Remove the image
//                 dir.save_to_file("directory.json"); // Save the updated directory
//                 let notification = json!({
//                     "message": format!("Client {} deleted image {}", client_id, image_name),
//                     "client_id": client_id,
//                     "image_name": image_name
//                 });
//                 let _ = notifier.send(notification.to_string());
//                 return reply::json(&notification);
//             }
//         }

//         let error_message = json!({
//             "error": format!("Image {} not found for client {}", image_name, client_id)
//         });
//         reply::json(&error_message)
//     });



//     // List all clients and their images
//     let list_all = warp::path("list_all")
//     .and(warp::get())
//     .and(with_directory(directory.clone()))
//     .map(|dir: SharedDirectory| {
//         let dir = dir.lock().unwrap();
//         // Parse and format the directory for consistent output
//         let clients: HashMap<String, Vec<serde_json::Value>> = dir
//             .clients
//             .iter()
//             .map(|(client_id, images)| {
//                 let parsed_images: Vec<serde_json::Value> = images
//                     .iter()
//                     .filter_map(|img| serde_json::from_str(img).ok())
//                     .collect();
//                 (client_id.clone(), parsed_images)
//             })
//             .collect();
//         reply::json(&clients)
//     });


//     // Combine all routes
//     let routes = add_image.or(delete_image).or(list_all).or(ws_route);

//     // Start the server
//     warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
// }

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor};
use warp::{Filter, reply};
use warp::ws::{WebSocket, Message};
use tokio::sync::broadcast;
use futures_util::{StreamExt, SinkExt};
use serde_json::json;
use image::{DynamicImage, Rgba, ImageBuffer, GenericImage, ImageOutputFormat};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};
use base64::{engine::general_purpose, Engine};

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
async fn main() {
    let file_path = "directory.json";
    let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file(file_path)));

    let (notifier_tx, _) = broadcast::channel(100);

    // Add an image for a client
    let add_image = warp::path("add_image")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_directory(directory.clone()))
        .and(with_notifier(notifier_tx.clone()))
        .map(|body: HashMap<String, String>, dir: SharedDirectory, notifier: broadcast::Sender<String>| {
            let client_id = body.get("client_id").unwrap();
            let image_name = body.get("image_name").unwrap();
            let image_data = body.get("image_data").unwrap();
            let mut dir = dir.lock().unwrap();

            let images = dir.clients.entry(client_id.clone()).or_insert_with(Vec::new);
            images.push(json!({
                "name": image_name,
                "data": image_data
            }).to_string());

            dir.save_to_file("directory.json");
            let notification = format!("Client {} added image {}", client_id, image_name);
            let _ = notifier.send(notification.clone());
            
            warp::reply::json(&notification)
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

    // List all clients and their images
    let list_all = warp::path("list_all")
        .and(warp::get())
        .and(with_directory(directory.clone()))
        .map(|dir: SharedDirectory| {
            let dir = dir.lock().unwrap();
            warp::reply::json(&dir.clients)
        });

    // List by client with composite image
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

    // Combine all routes
    let routes = add_image
        .or(delete_image)
        .or(list_all)
        .or(list_by_client);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
