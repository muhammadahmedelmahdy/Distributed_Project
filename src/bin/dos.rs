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
            println!("DEBUG: Found owner '{}'", owner);
            if let Some(image) = images.iter_mut().find(|img| img.name == image_name) {
                println!("DEBUG: Found image '{}'", image_name);
                if !image.access.iter().any(|(name, _)| name == client_name) {
                    println!(
                        "DEBUG: Granting access to '{}' with permitted_views={:?}",
                        client_name, permitted_views
                    );
                    image.access.push((client_name.to_string(), permitted_views));
                    return true; // Successfully updated access
                } else {
                    println!("DEBUG: Client '{}' already has access", client_name);
                }
            } else {
                println!("ERROR: Image '{}' not found for owner '{}'", image_name, owner);
            }
        } else {
            println!("ERROR: Owner '{}' not found", owner);
        }
        false // Failed to update access
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
    
    
        let update_access = warp::path("update_access")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_directory(directory.clone()))
        .map(|body: HashMap<String, serde_json::Value>, dir: SharedDirectory| {
            println!("Received update_access request: {:?}", body); // Debug log
    
            let owner = body.get("owner").unwrap().as_str().unwrap();
            let image_name = body.get("image_name").unwrap().as_str().unwrap();
            let client_name = body.get("client_name").unwrap().as_str().unwrap();
            let permitted_views = body.get("permitted_views").unwrap().as_i64().unwrap() as i32;
    
            println!("Parsed payload - Owner: {}, Image: {}, Client: {}, Views: {}", owner, image_name, client_name, permitted_views);
    
            let mut dir = dir.lock().unwrap();
    
            if dir.update_access(owner, image_name, client_name, Some(permitted_views)) {
                println!("Access updated successfully.");
                dir.save_to_file("directory.json");
                warp::reply::json(&json!({"status": "success"}))
            } else {
                println!("Failed to update access.");
                warp::reply::json(&json!({"status": "failed"}))
            }
        });
    
    
    

    let list_by_client = warp::path("list_by_client")
        .and(warp::get())
        .and(warp::query::<HashMap<String, String>>())
        .and(with_directory(directory.clone()))
        .map(|query: HashMap<String, String>, dir: SharedDirectory| {
            let default_client_id = String::from("");
            let client_id = query.get("client_id").unwrap_or(&default_client_id);

            eprintln!("DEBUG: Fetching images for client '{}'", client_id); // Log the client_id

            let dir = dir.lock().unwrap();

            // Retrieve images for the specified client_id
            let images = match dir.clients.get(client_id) {
                Some(images) if !images.is_empty() => images.clone(),
                _ => {
                    eprintln!("ERROR: No images found for client '{}'", client_id); // Log error
                    return warp::http::Response::builder()
                        .status(404)
                        .header("Content-Type", "text/plain")
                        .body(format!("No images found for client '{}'", client_id).into_bytes())
                        .unwrap();
                }
            };

            eprintln!("DEBUG: Found {} images for client '{}'", images.len(), client_id);

            match create_composite_image(&images) {
                Ok(composite_image) => {
                    let mut buffer = Cursor::new(Vec::new());
                    if composite_image
                        .write_to(&mut buffer, ImageOutputFormat::Png)
                        .is_err()
                    {
                        eprintln!("ERROR: Failed to encode composite image for client '{}'", client_id);
                        return warp::http::Response::builder()
                            .status(500)
                            .header("Content-Type", "text/plain")
                            .body("Failed to encode composite image".to_string().into_bytes())
                            .unwrap();
                    }

                    eprintln!("DEBUG: Successfully created composite image for client '{}'", client_id);

                    warp::http::Response::builder()
                        .header("Content-Type", "image/png")
                        .body(buffer.into_inner())
                        .unwrap()
                }
                Err(e) => {
                    eprintln!("ERROR: Failed to create composite image for client '{}': {}", client_id, e);
                    warp::http::Response::builder()
                        .status(500)
                        .header("Content-Type", "text/plain")
                        .body(format!("Failed to create composite image: {}", e).into_bytes())
                        .unwrap()
                }
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
        .or(delete_image);
    
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
