use serde::{Serialize, Deserialize};
use tokio::net::{UdpSocket, TcpListener, TcpStream};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration, sleep};
use std::error::Error;
use steganography::util::file_as_dynamic_image;
use steganography::decoder::Decoder;
use image::open;
use reqwest::Client;
use tokio_tungstenite::connect_async;
use futures_util::stream::StreamExt;
use serde_json::json;
use image::{io::Reader as ImageReader, DynamicImage, ImageOutputFormat};
use base64::encode;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr,  UdpSocket as StdUdpSocket};
use std::io::{self, Write};
use tokio::runtime::Runtime;
use std::sync::{Arc, Mutex};

const SERVER_ADDRS: [&str; 3] = ["127.0.0.1:8080", "127.0.0.1:8081", "127.0.0.1:8082"];
const CLIENT_ADDR: &str = "127.0.0.1:0";
const CHUNK_SIZE: usize = 1024;
const TIMEOUT_DURATION: Duration = Duration::from_secs(10);
const ACK: &[u8] = b"ACK";
const CLIENT_ID: &str = "client1";


#[derive(Serialize, Deserialize, Debug)]
struct ImageRequest {
    from_client_id: String,
    requested_image_name: String,
    permitted_views: i32, // Number of views requested
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  
    //Interactive CLI
    let dos_address = "http://localhost:3030";

    // Hardcoded file paths for composite images
    let composite_image_path_by_client = "composite_by_client.png";
    let composite_image_path_all_clients = "composite_all_clients.png";

    const LISTEN_PORT: u16 = 9002; // Listening port for client1
    const TARGET_PORT: u16 = 9001; // Port for client2
    const CLIENT_ID: &str = "client2";

    let incoming_requests = Arc::new(Mutex::new(Vec::new()));

    // Start listening for incoming requests
    let listener_requests = incoming_requests.clone();
    tokio::spawn(async move {
        start_listener(LISTEN_PORT, listener_requests).await;
    });

    loop {
        println!("\n--- Directory of Service CLI ---");
        println!("1. Add Image");
        println!("2. Delete Image");
        println!("3. View Composite Image (by Client)");
        println!("4. View Composite Image (All Clients)");
        println!("5. Request Image");
        println!("6. View Incoming Requests");
        println!("7. Exit");
        print!("Enter your choice: ");
        std::io::stdout().flush()?;

        let mut choice = String::new();
        std::io::stdin().read_line(&mut choice)?;
        let choice = choice.trim();

        match choice {
            "1" => {
                print!("Enter your client ID: ");
                std::io::stdout().flush()?;
                let mut client_id = String::new();
                std::io::stdin().read_line(&mut client_id)?;
                let client_id = client_id.trim();

                print!("Enter the path to the image: ");
                std::io::stdout().flush()?;
                let mut image_path = String::new();
                std::io::stdin().read_line(&mut image_path)?;
                let image_path = image_path.trim();

                match add_image_to_dos(dos_address, client_id, image_path).await {
                    Ok(response) => println!("Success: {}", response),
                    Err(e) => println!("Failed to upload image: {}", e),
                }
            }
            "2" => {
                print!("Enter your client ID: ");
                std::io::stdout().flush()?;
                let mut client_id = String::new();
                std::io::stdin().read_line(&mut client_id)?;
                let client_id = client_id.trim();

                print!("Enter the name of the image to delete: ");
                std::io::stdout().flush()?;
                let mut image_name = String::new();
                std::io::stdin().read_line(&mut image_name)?;
                let image_name = image_name.trim();

                match delete_image_from_dos(dos_address, client_id, image_name).await {
                    Ok(response) => println!("Image deleted successfully: {}", response),
                    Err(e) => println!("Failed to delete image: {}", e),
                }
            }
            "3" => {
                print!("Enter the client ID: ");
                std::io::stdout().flush()?;
                let mut client_id = String::new();
                std::io::stdin().read_line(&mut client_id)?;
                let client_id = client_id.trim();

                match fetch_composite_image(dos_address, client_id, composite_image_path_by_client).await {
                    Ok(_) => println!(
                        "Composite image for client '{}' saved to {}",
                        client_id, composite_image_path_by_client
                    ),
                    Err(e) => println!("Failed to fetch composite image: {}", e),
                }
            }
            "4" => {
                match fetch_composite_image_all_clients(dos_address, composite_image_path_all_clients).await {
                    Ok(_) => println!(
                        "Composite image for all clients saved to {}",
                        composite_image_path_all_clients
                    ),
                    Err(e) => println!("Failed to fetch composite image for all clients: {}", e),
                }
            }
            "5" => {
                print!("Enter the name of the image you want to request: ");
                std::io::stdout().flush()?;
                let mut image_name = String::new();
                std::io::stdin().read_line(&mut image_name)?;
                let image_name = image_name.trim().to_string();

                print!("Enter the number of views you want: ");
                std::io::stdout().flush()?;
                let mut permitted_views = String::new();
                std::io::stdin().read_line(&mut permitted_views)?;
                let permitted_views: i32 = permitted_views.trim().parse().unwrap_or(0);

                let request = ImageRequest {
                    from_client_id: CLIENT_ID.to_string(),
                    requested_image_name: image_name.clone(),
                    permitted_views,
                };

                match send_request(TARGET_PORT, &request).await {
                    Ok(_) => println!("Request sent for image {} with {} views", image_name, permitted_views),
                    Err(e) => println!("Failed to send request: {}", e),
                }
            }
            "6" => {
                let mut requests = incoming_requests.lock().unwrap();

                if requests.is_empty() {
                    println!("No incoming requests.");
                } else {
                    println!("Incoming Requests:");
                    for (i, request) in requests.iter().enumerate() {
                        println!(
                            "{}. From Client: {}, Image: {}, Requested Views: {}",
                            i + 1,
                            request.from_client_id,
                            request.requested_image_name,
                            request.permitted_views,
                        );
                    }

                    print!("Enter the number of the request to handle (or 0 to exit): ");
                    std::io::stdout().flush()?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    let choice: usize = input.trim().parse().unwrap_or(0);

                    if choice > 0 && choice <= requests.len() {
                        let selected_request = requests.remove(choice - 1);
                        println!(
                            "Handling request from Client: {}, for Image: {}, Requested Views: {}",
                            selected_request.from_client_id,
                            selected_request.requested_image_name,
                            selected_request.permitted_views,
                        );

                        print!("Accept request? (yes/no): ");
                        std::io::stdout().flush()?;
                        let mut response = String::new();
                        std::io::stdin().read_line(&mut response)?;
                        let response = response.trim().to_lowercase();

                        if response == "yes" {
                            println!("Request accepted. Updating access in the DoS...");
                            let update_response = update_image_access(
                                dos_address,
                                CLIENT_ID,
                                &selected_request.requested_image_name,
                                &selected_request.from_client_id,
                                selected_request.permitted_views,
                            )
                            .await;

                            if update_response.is_ok() {
                                println!("Access updated. Sending acceptance message...");
                                notify_requester(
                                    TARGET_PORT,
                                    &selected_request.from_client_id,
                                    &selected_request.requested_image_name,
                                    "accepted",
                                )
                                .await?;
                            } else {
                                println!("Failed to update access.");
                            }
                        } else {
                            println!("Request denied. Sending denial message...");
                            notify_requester(
                                TARGET_PORT,
                                &selected_request.from_client_id,
                                &selected_request.requested_image_name,
                                "denied",
                            )
                            .await?;
                        }
                    }
                }
            }
            "7" => {
                println!("Exiting... Goodbye!");
                break;
            }
            _ => {
                println!("Invalid choice. Please try again.");
            }
        }
    }

    Ok(())


}

async fn update_image_access(
    dos_address: &str,
    owner: &str,
    image_name: &str,
    client_name: &str,
    permitted_views: i32,
) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    let payload = json!({
        "owner": owner,
        "image_name": image_name,
        "client_name": client_name,
        "permitted_views": permitted_views,
    });

    let response = client
        .post(format!("{}/update_access", dos_address))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err("Failed to update access".into())
    }
}


async fn notify_requester(
    target_port: u16,
    from_client_id: &str,
    image_name: &str,
    status: &str,
) -> Result<(), Box<dyn Error>> {
    let message = format!(
        "Request for image '{}' was {} by {}",
        image_name, status, from_client_id
    );

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", target_port)).await?;
    stream.write_all(message.as_bytes()).await?;
    Ok(())
}



async fn start_listener(port: u16, incoming_requests: Arc<Mutex<Vec<ImageRequest>>>) {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
    println!("Listening for requests on port {}", port);

    loop {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buffer = Vec::new();
            if let Ok(size) = socket.read_to_end(&mut buffer).await {
                let buffer = &buffer[..size];

                // Attempt to parse the incoming message as an ImageRequest
                if let Ok(request) = serde_json::from_slice::<ImageRequest>(buffer) {
                    println!(
                        "\nRequest received: From Client '{}', For Image '{}', Requested Views: {}",
                        request.from_client_id, 
                        request.requested_image_name, 
                        request.permitted_views // Include the requested views in the log
                    );
                    incoming_requests.lock().unwrap().push(request);
                } else if let Ok(message) = String::from_utf8(buffer.to_vec()) {
                    println!("\nNotification received: {}", message);
                } else {
                    println!("Received an unknown or malformed message.");
                }
            }
        }
    }
}




async fn send_request(target_port: u16, request: &ImageRequest) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", target_port)).await?;
    let data = serde_json::to_vec(request)?;
    stream.write_all(&data).await?;
    println!(
        "Request sent to port {} for image {}",
        target_port, request.requested_image_name
    );
    Ok(())
}

async fn wait_for_leader(socket: &UdpSocket) -> Result<String, Box<dyn Error>> {
    let mut buffer = [0; 1024];
    loop {
        if let Ok((len, addr)) = timeout(TIMEOUT_DURATION, socket.recv_from(&mut buffer)).await? {
            let response = String::from_utf8_lossy(&buffer[..len]);
            if response.starts_with("LEADER,") {
                let leader_address = response[7..].to_string();
                println!("Received leader confirmation from {}", leader_address);
                return Ok(leader_address);
            }
        }
    }
}

async fn send_image_data(socket: &UdpSocket, leader_addr: &str) -> tokio::io::Result<()> {
    let image_path = "original_image.jpg";
    let mut file = File::open(image_path).await?;
    let mut buffer = [0u8; CHUNK_SIZE];
    let mut chunk_number: u32 = 0;

    println!("Client: Sending image data to leader at {}", leader_addr);

    // Send the image in chunks to the elected leader
    while let Ok(bytes_read) = file.read(&mut buffer).await {
        if bytes_read == 0 { break; }

        let packet = [&chunk_number.to_be_bytes(), &buffer[..bytes_read]].concat();
        loop {
            socket.send_to(&packet, leader_addr).await?;
            if receive_ack(socket).await { break; }
            sleep(Duration::from_millis(100)).await;
        }
        chunk_number += 1;
    }

    socket.send_to(b"END", leader_addr).await?;
    println!("Client: Image transfer complete!");

    Ok(())
}

async fn receive_ack(socket: &UdpSocket) -> bool {
    let mut buffer = [0; 1024];
    match timeout(Duration::from_secs(2), socket.recv_from(&mut buffer)).await {
        Ok(Ok((len, _))) => {
            let response = String::from_utf8_lossy(&buffer[..len]);
            response.trim() == "ACK"
        },
        _ => false,
    }
}

async fn middleware_decrypt(socket: &UdpSocket) -> Result<(), Box<dyn Error>> {
    receive_image(socket).await?;
    decode_image("encoded_image_received.png").await?;
    Ok(())
}
async fn middleware_encrypt(socket: &UdpSocket, leader_addr: &str) -> tokio::io::Result<()> {
    let image_path = "original_image.jpg"; // Path to the image file
    let mut file = File::open(image_path).await?;
    let mut buffer = [0u8; CHUNK_SIZE];
    let mut chunk_number: u32 = 0;

    // Notify the server about the image transfer
    socket.send_to(b"IMAGE_TRANSFER", leader_addr).await?;
    println!("Client: Starting image transfer to {}", leader_addr);

    // Send the image in chunks
    while let Ok(bytes_read) = file.read(&mut buffer).await {
        if bytes_read == 0 { break; }

        loop {
            let packet = [&chunk_number.to_be_bytes(), &buffer[..bytes_read]].concat();
            socket.send_to(&packet, leader_addr).await?;
            if receive_ack(socket).await { break; } // Wait for ACK before proceeding
            sleep(Duration::from_millis(100)).await; // Retry if no ACK
        }
        chunk_number += 1;
    }

    // Signal end of transfer
    socket.send_to(b"END", leader_addr).await?;
    println!("Client: Image transfer complete!");

    Ok(())
}

/// Receive image data from the server in chunks and save it as a PNG file.
async fn receive_image(socket: &UdpSocket) -> tokio::io::Result<()> {
    let mut file = File::create("encoded_image_received.png").await?;
    let mut buffer = [0u8; CHUNK_SIZE + 4]; // 4 extra bytes for chunk number
    let mut expected_chunk_number: u32 = 0;

    println!("Client: Waiting to receive encrypted image...");

    loop {
        let (bytes_received, addr) = socket.recv_from(&mut buffer).await?;

        if &buffer[..bytes_received] == b"END" {
            println!("Client: Received end of transmission signal.");
            break;
        }

        let chunk_number = u32::from_be_bytes(buffer[..4].try_into().unwrap());

        if chunk_number == expected_chunk_number {
            file.write_all(&buffer[4..bytes_received]).await?;
            expected_chunk_number += 1;

            socket.send_to(ACK, addr).await?;
            println!("Client: Acknowledged chunk {}", chunk_number);
        } else {
            println!("Client: Unexpected chunk number. Expected {}, but received {}.", expected_chunk_number, chunk_number);
        }
    }

    println!("Client: Image successfully received and saved as PNG.");
    Ok(())
}



/// Adds an image to the Directory of Service (DoS) after resizing it and encoding it as Base64.
///
/// # Arguments
/// * `dos_address` - The base URL of the DoS server (e.g., "http://localhost:3030").
/// * `client_id` - The ID of the client uploading the image.
/// * `image_path` - The path to the image file.
///
/// # Returns
/// A `Result` containing the response from the DoS or an error if the request fails.
pub async fn add_image_to_dos(
    dos_address: &str,
    client_id: &str,
    image_path: &str,
) -> Result<String, Box<dyn Error>> {
    // Create a new HTTP client
    let client = Client::new();

    // Load and process the image (resize and encode as Base64)
    let resized_image = resize_image(image_path)?;
    let base64_image = encode_image_to_base64(&resized_image)?;

    // Construct the request payload
    let payload = json!({
        "client_id": client_id,
        "image_name": image_path, // Keep the original image name
        "image_data": base64_image // Include the Base64-encoded image
    });

    // Send the POST request to the DoS
    let response = client
        .post(format!("{}/add_image", dos_address))
        .json(&payload)
        .send()
        .await?;

    // Check if the response status is success
    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(format!(
            "Failed to add image (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}

/// Resizes the image to a lower resolution.
///
/// # Arguments
/// * `image_path` - The path to the image file.
///
/// # Returns
/// A `DynamicImage` containing the resized image.
fn resize_image(image_path: &str) -> Result<DynamicImage, Box<dyn Error>> {
    let image = ImageReader::open(image_path)?.decode()?;
    let resized_image = image.resize_exact(200, 200, image::imageops::FilterType::Triangle);
    Ok(resized_image)
}

/// Encodes a `DynamicImage` as a Base64 string in PNG format.
///
/// # Arguments
/// * `image` - The `DynamicImage` to encode.
///
/// # Returns
/// A Base64 string representation of the image.
fn encode_image_to_base64(image: &DynamicImage) -> Result<String, Box<dyn Error>> {
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer); // Wrap Vec<u8> in Cursor
    image.write_to(&mut cursor, ImageOutputFormat::Png)?;
    Ok(base64::encode(&buffer))
}


/// Deletes an image from the Directory of Service (DoS) by its name.
///
/// # Arguments
/// * `dos_address` - The base URL of the DoS server (e.g., "http://localhost:3030").
/// * `client_id` - The ID of the client deleting the image.
/// * `image_name` - The name of the image to delete.
///
/// # Returns
/// A `Result` containing the server's response message or an error if the request fails.
pub async fn delete_image_from_dos(
    dos_address: &str,
    client_id: &str,
    image_name: &str,
) -> Result<String, Box<dyn Error>> {
    // Create a new HTTP client
    let client = Client::new();

    // Construct the request payload
    let payload = json!({
        "client_id": client_id,
        "image_name": image_name,
    });

    // Send the POST request to the DoS
    let response = client
        .post(format!("{}/delete_image", dos_address))
        .json(&payload)
        .send()
        .await?;

    // Handle response based on HTTP status
    if response.status().is_success() {
        // Return the server's response message
        let response_text = response.text().await?;
        println!("Server response: {}", response_text);
        Ok(response_text)
    } else {
        // Handle errors with detailed message
        let status = response.status();
        let error_body = response.text().await.unwrap_or_else(|_| "No response body".to_string());
        Err(format!(
            "Failed to delete image (status: {}): {}",
            status, error_body
        )
        .into())
    }
}

/// Fetches the composite image for all clients from the DoS and saves it locally.
///
/// # Arguments
/// * `dos_address` - The base URL of the DoS server (e.g., "http://localhost:3030").
/// * `output_path` - The path where the composite image should be saved.
///
/// # Returns
/// A `Result` indicating success or failure.
pub async fn fetch_composite_image_all_clients(
    dos_address: &str,
    output_path: &str,
) -> Result<(), Box<dyn Error>> {
    // Create an HTTP client
    let client = Client::new();

    // Send the GET request to the endpoint for all clients
    let response = client.get(format!("{}/list_all_clients", dos_address)).send().await?;

    // Check if the response status is success
    if response.status().is_success() {
        let image_bytes = response.bytes().await?;
        // Save the image to the specified output path
        let mut file = File::create(output_path).await?;
        file.write_all(&image_bytes).await?;
        println!("Composite image for all clients saved to {}", output_path);
        Ok(())
    } else {
        Err(format!(
            "Failed to fetch composite image for all clients (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}



/// Fetches the composite image for a specific client from the DoS and saves it locally.
///
/// # Arguments
/// * `dos_address` - The base URL of the DoS server (e.g., "http://localhost:3030").
/// * `client_id` - The ID of the client whose images should be listed.
/// * `output_path` - The path where the composite image should be saved.
///
/// # Returns
/// A `Result` indicating success or failure.
pub async fn fetch_composite_image(
    dos_address: &str,
    client_id: &str,
    output_path: &str,
) -> Result<(), Box<dyn Error>> {
    // Create an HTTP client
    let client = Client::new();

    // Construct the URL with query parameters
    let url = format!("{}/list_by_client?client_id={}", dos_address, client_id);

    // Send the GET request
    let response = client.get(&url).send().await?;

    // Check if the response status is success
    if response.status().is_success() {
        let image_bytes = response.bytes().await?;
        // Save the image to the specified output path
        let mut file = File::create(output_path).await?;
        file.write_all(&image_bytes).await?;
        println!("Composite image saved to {}", output_path);
        Ok(())
    } else {
        Err(format!(
            "Failed to fetch composite image (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}



/// Returns the local IP address of the client.
///
/// This function is now asynchronous to support `.await`.
async fn get_local_ip() -> Result<IpAddr, Box<dyn Error>> {
    // Use a UDP socket to determine the local IP address
    let socket = StdUdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?; // Connect to a public DNS server to resolve the local address
    let local_addr = socket.local_addr()?;
    Ok(local_addr.ip())
}



/// Decode the received image and save the decoded output.
async fn decode_image(file_path: &str) -> Result<(), Box<dyn Error>> {
    let encoded_image = file_as_dynamic_image(file_path.to_string()).to_rgba();
    let decoder = Decoder::new(encoded_image);

    let decoded_data = decoder.decode_alpha();
    let output_path = "decoded_output.png";
    std::fs::write(output_path, &decoded_data)?;
    println!("Image decoded and saved to {}", output_path);
    Ok(())
}


