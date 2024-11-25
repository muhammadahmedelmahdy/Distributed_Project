use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor};
use warp::{Filter, reply};
use image::{DynamicImage, Rgba, ImageBuffer, GenericImage, ImageOutputFormat};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};
use base64::{engine::general_purpose, Engine};
use serde_json::json;

#[derive(Serialize, Deserialize, Clone)]
struct ImageMetadata {
    name: String,
    owner: String,
    data: String,
    access: Vec<(String, Option<i32>)>, // (client_name, permitted_views: None for unlimited)
}

#[derive(Serialize, Deserialize, Clone)]
struct Directory {
    clients: HashMap<String, Vec<ImageMetadata>>,
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

    fn update_access(&mut self, owner: &str, image_name: &str, client_name: &str, permitted_views: Option<i32>) -> bool {
        if let Some(images) = self.clients.get_mut(owner) {
            if let Some(image) = images.iter_mut().find(|img| img.name == image_name) {
                // Check if client already has access
                if let Some(access_entry) = image.access.iter_mut().find(|(name, _)| name == client_name) {
                    // Update permitted views
                    access_entry.1 = permitted_views;
                    return true;
                } else {
                    // Add new access entry
                    image.access.push((client_name.to_string(), permitted_views));
                    return true;
                }
            }
        }
        false
    }

    fn revoke_access(&mut self, owner: &str, image_name: &str, client_name: &str) -> bool {
        if let Some(images) = self.clients.get_mut(owner) {
            if let Some(image) = images.iter_mut().find(|img| img.name == image_name) {
                // Remove access entry
                image.access.retain(|(name, _)| name != client_name);
                return true;
            }
        }
        false
    }
    
    
}

type SharedDirectory = Arc<Mutex<Directory>>;

fn with_directory(
    directory: SharedDirectory,
) -> impl Filter<Extract = (SharedDirectory,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || directory.clone())
}

fn create_composite_image(images: &[ImageMetadata]) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    const FONT_PATH: &str = "/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/Roboto-Bold.ttf";
    const PLACEHOLDER_IMAGE_PATH: &str = "/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/NoImagePlaceholder.jpg";

    if images.is_empty() {
        eprintln!("No images found, returning placeholder image.");
        return Ok(image::open(PLACEHOLDER_IMAGE_PATH)?);
    }

    let font_data = std::fs::read(FONT_PATH)?;
    let font = Font::try_from_vec(font_data).ok_or("Failed to load font")?;

    let mut decoded_images = Vec::new();
for image in images {
    match general_purpose::STANDARD.decode(&image.data) {
        Ok(decoded_bytes) => {
            if let Ok(decoded_image) = image::load_from_memory(&decoded_bytes) {
                decoded_images.push((image.name.clone(), image.owner.clone(), decoded_image));
            } else {
                eprintln!("ERROR: Invalid image data for '{}'", image.name);
            }
        }
        Err(e) => {
            eprintln!("ERROR: Failed to decode Base64 for '{}': {}", image.name, e);
        }
    }
}

if decoded_images.is_empty() {
    eprintln!("ERROR: No valid images found. Returning placeholder.");
    return Ok(image::open(PLACEHOLDER_IMAGE_PATH)?);
}


    let total_width = decoded_images.iter().map(|(_, _, img)| img.width()).max().unwrap_or(0);
    let total_height: u32 = decoded_images.iter().map(|(_, _, img)| img.height()).sum();

    let mut composite = ImageBuffer::new(total_width, total_height);
    let mut y_offset = 0;

    for (image_name, image_owner, img) in decoded_images {
        composite.copy_from(&img.to_rgba8(), 0, y_offset)?;

        let scale = Scale { x: 20.0, y: 20.0 };
        let text_color = Rgba([255, 255, 255, 255]);
        draw_text_mut(
            &mut composite,
            text_color,
            10,
            y_offset as i32 + 10,
            scale,
            &font,
            &format!("Name: {}", image_name),
        );

        draw_text_mut(
            &mut composite,
            text_color,
            10,
            y_offset as i32 + 40,
            scale,
            &font,
            &format!("Owner: {}", image_owner),
        );

        y_offset += img.height();
    }

    Ok(DynamicImage::ImageRgba8(composite))
}


#[tokio::main]
async fn main() {
    let file_path = "directory.json";
    let directory: SharedDirectory = Arc::new(Mutex::new(Directory::load_from_file(file_path)));

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--clear" {
        let mut dir = directory.lock().unwrap();
        dir.clear();
        dir.save_to_file(file_path);
        println!("Directory cleared.");
    }

    let add_image = warp::path("add_image")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_directory(directory.clone()))
        .map(|body: HashMap<String, String>, dir: SharedDirectory| {
            let client_id = body.get("client_id").unwrap();
            let image_name = body.get("image_name").unwrap();
            let image_data = body.get("image_data").unwrap();
            let mut dir = dir.lock().unwrap();

            let images = dir.clients.entry(client_id.clone()).or_insert_with(Vec::new);
            images.push(ImageMetadata {
                name: image_name.clone(),
                owner: client_id.clone(),
                data: image_data.clone(),
                access: vec![(client_id.clone(), None)],
            });

            dir.save_to_file("directory.json");
            reply::json(&json!({"status": "success"}))
        });

    let delete_image = warp::path("delete_image")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_directory(directory.clone()))
        .map(|body: HashMap<String, String>, dir: SharedDirectory| {
            let client_id = body.get("client_id").unwrap();
            let image_name = body.get("image_name").unwrap();
            let mut dir = dir.lock().unwrap();
    
            if let Some(images) = dir.clients.get_mut(client_id) {
                if let Some(pos) = images.iter().position(|img| img.name == *image_name) {
                    images.remove(pos);
                    dir.save_to_file("directory.json");
                    return reply::json(&json!({"status": "success"}));
                }
            }
    
            reply::json(&json!({
                "status": "failed",
                "error": format!("Image {} not found for client {}", image_name, client_id)
            }))
        });

        let list_images = warp::path("list_images")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and(with_directory(directory.clone()))
            .map(|query: HashMap<String, String>, dir: SharedDirectory| {
                let client_id = query.get("client_id").unwrap();
                let dir = dir.lock().unwrap();

                let images = dir.clients.get(client_id).cloned().unwrap_or_default();
                warp::reply::json(&images)
            });
        
        let get_image_access = warp::path("get_image_access")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and(with_directory(directory.clone()))
            .map(|query: HashMap<String, String>, dir: SharedDirectory| {
                let owner = query.get("owner").unwrap();
                let image_name = query.get("image_name").unwrap();
                let dir = dir.lock().unwrap();
        
                if let Some(images) = dir.clients.get(owner) {
                    if let Some(image) = images.iter().find(|img| img.name == *image_name) {
                        warp::reply::json(&image.access)
                    } else {
                        warp::reply::json(&json!({
                            "status": "failed",
                            "error": "Image not found."
                        }))
                    }
                } else {
                    warp::reply::json(&json!({
                        "status": "failed",
                        "error": "Owner not found."
                    }))
                }
            });
        
    
    
        let update_access = warp::path("update_access")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_directory(directory.clone()))
            .map(|body: HashMap<String, serde_json::Value>, dir: SharedDirectory| {
                let owner = body.get("owner").and_then(|v| v.as_str()).unwrap();
                let image_name = body.get("image_name").and_then(|v| v.as_str()).unwrap();
                let client_name = body.get("client_name").and_then(|v| v.as_str()).unwrap();
                let permitted_views = body.get("permitted_views").and_then(|v| v.as_i64()).map(|v| v as i32);
        
                let mut dir = dir.lock().unwrap();
        
                if dir.update_access(owner, image_name, client_name, permitted_views) {
                    dir.save_to_file("directory.json");
                    warp::reply::json(&json!({"status": "success"}))
                } else {
                    warp::reply::json(&json!({"status": "failed"}))
                }
            });
        
        let revoke_access = warp::path("revoke_access")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_directory(directory.clone()))
            .map(|body: HashMap<String, serde_json::Value>, dir: SharedDirectory| {
                let owner = body.get("owner").and_then(|v| v.as_str()).unwrap();
                let image_name = body.get("image_name").and_then(|v| v.as_str()).unwrap();
                let client_name = body.get("client_name").and_then(|v| v.as_str()).unwrap();

                let mut dir = dir.lock().unwrap();

                if dir.revoke_access(owner, image_name, client_name) {
                    dir.save_to_file("directory.json");
                    warp::reply::json(&json!({"status": "success"}))
                } else {
                    warp::reply::json(&json!({"status": "failed"}))
                }
            });

    
    

        let list_by_client = warp::path("list_by_client")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(with_directory(directory.clone()))
        .map(|query: HashMap<String, String>, dir: SharedDirectory| {
            let client_id = query.get("client_id").map(String::as_str).unwrap_or("");
            let dir = dir.lock().unwrap();
    
            // Fetch images for the given client
            let images = dir.clients.get(client_id).cloned().unwrap_or_else(Vec::new);
    
            eprintln!("DEBUG: Found {} images for client '{}'", images.len(), client_id);
    
            match create_composite_image(&images) {
                Ok(composite_image) => {
                    let mut buffer = Cursor::new(Vec::new());
                    if composite_image
                        .write_to(&mut buffer, ImageOutputFormat::Png)
                        .is_err()
                    {
                        eprintln!("ERROR: Failed to encode composite image.");
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
                Err(e) => {
                    eprintln!("ERROR: Failed to create composite image: {}", e);
                    warp::http::Response::builder()
                        .status(500)
                        .header("Content-Type", "text/plain")
                        .body(format!("Failed to create composite image: {}", e).into_bytes())
                        .unwrap()
                }
            }
        });

        let validate_image_request = warp::path("validate_image_request")
    .and(warp::get())
    .and(warp::query::<HashMap<String, String>>())
    .and(with_directory(directory.clone()))
    .map(|query: HashMap<String, String>, dir: SharedDirectory| {
        let image_name = query.get("image_name").map(String::as_str).unwrap_or("");
        let client_id = query.get("client_id").map(String::as_str).unwrap_or("");

        let dir = dir.lock().unwrap();

        // Check if the image exists and ensure the requester is not the owner
        if let Some((owner, _)) = dir.clients.iter().find(|(_, images)| {
            images.iter().any(|image| image.name == image_name)
        }) {
            if owner == client_id {
                return warp::http::Response::builder()
                    .status(400)
                    .header("Content-Type", "text/plain")
                    .body("Request denied: You are the owner of this image.".to_string().into_bytes())
                    .unwrap();
            }
            warp::http::Response::builder()
                .status(200)
                .header("Content-Type", "text/plain")
                .body("Validation successful".to_string().into_bytes())
                .unwrap()
        } else {
            warp::http::Response::builder()
                .status(404)
                .header("Content-Type", "text/plain")
                .body("Image not found.".to_string().into_bytes())
                .unwrap()
        }
    });

        


    let list_all_clients = warp::path("list_all_clients")
        .and(warp::get())
        .and(with_directory(directory.clone()))
        .map(|dir: SharedDirectory| {
            let dir = dir.lock().unwrap();
    
            // Collect all images from all clients
            let images: Vec<ImageMetadata> = dir
                .clients
                .values()
                .flat_map(|client_images| client_images.clone())
                .collect();
    
            eprintln!("DEBUG: Total images to process: {}", images.len()); // Log number of images
    
            match create_composite_image(&images) {
                Ok(composite_image) => {
                    let mut buffer = Cursor::new(Vec::new());
                    if composite_image
                        .write_to(&mut buffer, ImageOutputFormat::Png)
                        .is_err()
                    {
                        eprintln!("ERROR: Failed to encode composite image.");
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
                Err(e) => {
                    eprintln!("ERROR: Failed to create composite image: {}", e); // Log the error
                    warp::http::Response::builder()
                        .status(500)
                        .header("Content-Type", "text/plain")
                        .body(format!("Failed to create composite image: {}", e).into_bytes())
                        .unwrap()
                }
            }
        });
    

    
        let routes = add_image
            .or(update_access)
            .or(list_all_clients)
            .or(list_by_client)
            .or(delete_image)
            .or(list_images)
            .or(get_image_access)
            .or(validate_image_request)
            .or(revoke_access);
    
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
