use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use warp::{Filter, reply};
use warp::ws::{WebSocket, Message};
use tokio::sync::broadcast;
use futures_util::{StreamExt, SinkExt};
use image::{DynamicImage, ImageBuffer, GenericImageView, imageops::FilterType};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Directory {
    clients: HashMap<String, Vec<ImageData>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ImageData {
    id: u32,
    owner: String,
    path: String, // Changed to String to make serialization easier
    #[serde(skip)] // Skip DynamicImage in serialization
    #[serde(default = "default_dynamic_image")]
    image: Option<DynamicImage>, // Optional to simplify serialization
}

// Default function for DynamicImage
fn default_dynamic_image() -> Option<DynamicImage> {
    None
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

// Global Mutex to hold directory
type SharedDirectory = Arc<Mutex<Directory>>;

fn prepare_low_quality_image(image: &DynamicImage) -> DynamicImage {
    let preview = image.resize_exact(100, 100, FilterType::Triangle);
    DynamicImage::ImageRgba8(preview.to_rgba8())
}

fn add_image(directory: &mut Directory, id: u32, owner: &str, image_path: &Path) -> Result<(), image::ImageError> {
    let image = image::open(image_path)?;
    let processed_image = prepare_low_quality_image(&image);
    let image_data = ImageData {
        id,
        owner: owner.to_string(),
        path: image_path.to_string_lossy().to_string(),
        image: Some(processed_image),
    };

    directory.clients.entry(owner.to_string()).or_insert_with(Vec::new).push(image_data);
    Ok(())
}

fn delete_image(directory: &mut Directory, owner: &str, id: u32) {
    if let Some(images) = directory.clients.get_mut(owner) {
        images.retain(|img| img.id != id);
    }
}

fn combine_images_horizontally(directory: &Directory) -> Result<DynamicImage, image::ImageError> {
    let images: Vec<&DynamicImage> = directory
        .clients
        .values()
        .flat_map(|v| v.iter().filter_map(|data| data.image.as_ref()))
        .collect();

    let total_width: u32 = images.iter().map(|img| img.width()).sum();
    let max_height: u32 = images.iter().map(|img| img.height()).max().unwrap_or(0);

    let mut new_img = ImageBuffer::new(total_width, max_height);
    let mut x_offset = 0;

    for img in images {
        for (x, y, pixel) in img.pixels() {
            if y < max_height {
                new_img.put_pixel(x + x_offset, y, pixel);
            }
        }
        x_offset += img.width();
    }

    Ok(DynamicImage::ImageRgba8(new_img))
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
    let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file(file_path)));
    let (notifier_tx, _) = broadcast::channel(100);

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_notifier(notifier_tx.clone()))
        .map(|ws: warp::ws::Ws, notifier: broadcast::Sender<String>| {
            ws.on_upgrade(move |socket| handle_ws_connection(socket, notifier.subscribe()))
        });

    let add_image_route = warp::path("add_image")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_directory(directory.clone()))
        .and(with_notifier(notifier_tx.clone()))
        .map(|body: HashMap<String, String>, dir: SharedDirectory, notifier: broadcast::Sender<String>| {
            let id = body.get("id").unwrap().parse().unwrap_or(0);
            let client_id = body.get("client_id").unwrap();
            let image_path = Path::new(body.get("image_path").unwrap());
            let mut dir = dir.lock().unwrap();

            if let Err(err) = add_image(&mut dir, id, client_id, image_path) {
                return reply::json(&format!("Failed to add image: {}", err));
            }

            dir.save_to_file("directory.json");
            let notification = format!("Client {} added image {}", client_id, image_path.display());
            let _ = notifier.send(notification.clone());
            reply::json(&notification)
        });

    let delete_image_route = warp::path("delete_image")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_directory(directory.clone()))
        .and(with_notifier(notifier_tx.clone()))
        .map(|body: HashMap<String, String>, dir: SharedDirectory, notifier: broadcast::Sender<String>| {
            let id = body.get("id").unwrap().parse().unwrap_or(0);
            let client_id = body.get("client_id").unwrap();
            let mut dir = dir.lock().unwrap();

            delete_image(&mut dir, client_id, id);

            dir.save_to_file("directory.json");
            let notification = format!("Client {} deleted image with ID {}", client_id, id);
            let _ = notifier.send(notification.clone());
            reply::json(&notification)
        });

    // Preview combined images
    let preview_route = warp::path("preview")
        .and(warp::get())
        .and(with_directory(directory.clone()))
        .map(|dir: SharedDirectory| {
            let dir = dir.lock().unwrap();

            // Generate the combined preview image
            match combine_images_horizontally(&dir) {
                Ok(combined_image) => {
                    // Save the preview image
                    let preview_path = "preview.jpg";
                    if let Err(err) = combined_image.save_with_format(preview_path, image::ImageFormat::Jpeg) {
                        return reply::json(&format!("Failed to save preview: {}", err));
                    }

                    reply::json(&format!("Preview saved at {}", preview_path))
                }
                Err(err) => reply::json(&format!("Failed to generate preview: {}", err)),
            }
        });


    let list_all_route = warp::path("list_all")
        .and(warp::get())
        .and(with_directory(directory.clone()))
        .map(|dir: SharedDirectory| {
            let dir = dir.lock().unwrap();
            reply::json(&dir.clients)
        });

    let routes = add_image_route
        .or(delete_image_route)
        .or(list_all_route)
        .or(preview_route) // Include the preview route
        .or(ws_route);
    

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

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
