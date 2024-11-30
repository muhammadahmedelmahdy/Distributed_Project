use tokio::net::UdpSocket;
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
use std::collections::HashMap;

const SERVER_ADDRS: [&str; 3] = ["127.0.0.1:8080", "127.0.0.1:8081", "127.0.0.1:8082"];
const CLIENT_ADDR: &str = "127.0.0.1:0";
const CHUNK_SIZE: usize = 1024;
const TIMEOUT_DURATION: Duration = Duration::from_secs(10);
const ACK: &[u8] = b"ACK";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind(CLIENT_ADDR).await?;
    let leader_address = request_leader(&socket).await?;

    println!("Welcome! Choose an option:");
    println!("1: Register");
    println!("2: Login");

    let mut option = String::new();
    let mut dos_address = String::new();
    let mut client_id = String::new();
    let mut password = String::new();

    std::io::stdin().read_line(&mut option).expect("Failed to read line");

    match option.trim() {
        "1" => {
            let details = register_client_to_dos(&leader_address).await?;
            dos_address = details.0;
            client_id = details.1;
            password = details.2;
        },
        "2" => {

            match login(&leader_address).await {
                Ok((returned_dos_address, returned_client_id, returned_password)) => {
                    dos_address = returned_dos_address;
                    client_id = returned_client_id;
                    password = returned_password;
                    println!("Login successful!");
                }
                Err(e) => {
                    eprintln!("Login failed: {}", e);
                    return Ok(()); // Exit on failed login
                }
            }
        },
        _ => println!("Invalid option! Please try again."),  // Catch-all pattern
    }

    loop {
        println!("Choose an option:");
        println!("1: Add an image");
        println!("2: Delete an image");
        println!("3: View the gallery");
        println!("4: Exit");

        option.clear(); // Clear the previous input
        std::io::stdin().read_line(&mut option).expect("Failed to read line");

        match option.trim() {
            "1" => {
                println!("Enter image path:");
                let mut image_path = String::new();
                std::io::stdin().read_line(&mut image_path).expect("Failed to read line");
                let image_path = image_path.trim();
                match add_image_to_dos(&dos_address, &client_id, &password, image_path).await {
                    Ok(msg) => println!("Image added successfully: {}", msg),
                    Err(e) => println!("Failed to add image: {}", e),
                }
            },
            "2" => {
                println!("Enter image name:");
                let mut image_name = String::new();
                std::io::stdin().read_line(&mut image_name).expect("Failed to read line");
                let image_name = image_name.trim();
                match delete_image_from_dos(&dos_address, &client_id, &password, image_name).await {
                    Ok(msg) => println!("Image deleted successfully: {}", msg),
                    Err(e) => println!("Failed to delete image: {}", e),
                }
            },
            "3" => 
            {
                let output_path = "composite_image.png";
                match fetch_composite_image(&dos_address, &client_id, output_path).await {
                    Ok(_) => println!("Composite image fetched and saved to {}", output_path),
                    Err(e) => eprintln!("Error fetching composite image: {}", e),
                }
            },
            "4" => break,
            _ => println!("Invalid choice! Please select again."), 
        }
    }

    Ok(())
}


async fn login(leader_address: &str) -> Result<(String, String, String), Box<dyn std::error::Error>> {
    // Determine DOS port based on leader address
    let leader_port: u16 = leader_address.split(':')
        .nth(1) // Get the port part of the address
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);

    let dos_port = match leader_port {
        8080 => Some(8083),
        8081 => Some(8084),
        8082 => Some(8085),
        _ => {
            eprintln!("Unknown leader port: {}", leader_port);
            None
        }
    };

    if let Some(port) = dos_port {
        let client = Client::new();
        let dos_address = format!("http://localhost:{}", port);

        let login_endpoint = format!("{}/login", dos_address);

        // Get client_id and password from the user
        let mut client_id = String::new();
        print!("Enter client ID: ");
        std::io::stdout().flush()?; // Ensure prompt is visible
        std::io::stdin().read_line(&mut client_id)?;
        let client_id = client_id.trim().to_string(); // Remove newline and convert to String

        let mut password = String::new();
        print!("Enter password: ");
        std::io::stdout().flush()?; // Ensure prompt is visible
        std::io::stdin().read_line(&mut password)?;
        let password = password.trim().to_string(); // Remove newline and convert to String

        // Prepare the request payload
        let mut payload = HashMap::new();
        payload.insert("client_id", client_id.clone());
        payload.insert("password", password.clone());

        // Send the POST request
        let response = client.post(&login_endpoint)
            .json(&payload)
            .send()
            .await?;

        // Extract the status before consuming the body
        let status = response.status();
        let response_text = response.text().await?;

        // Print the status and raw response for debugging
        println!("Response Status: {}", status);
        println!("Raw Response: {}", response_text);

        // Provide messages based on response status code
        match status.as_u16() {
            200 => {
                if let Ok(response_body) = serde_json::from_str::<serde_json::Value>(&response_text) {
                    if let Some(message) = response_body.get("message") {
                        println!("{}", message);

                        // Fetch the local IP address
                        let local_ip = get_local_ip().await?;
                        println!("Updating client IP to: {}", local_ip);

                        // Update the IP address on the server
                        let update_response = update_client_ip(&dos_address, &client_id, &local_ip.to_string()).await;
                        match update_response {
                            Ok(response) => {
                                println!("IP updated successfully: {}", response);
                                // Return the details
                                return Ok((dos_address, client_id, password));
                            }
                            Err(err) => {
                                eprintln!("Failed to update IP: {}", err);
                                return Err("Failed to update IP after login.".into());
                            }
                        }
                    } else {
                        println!("Login successful, but no message received.");
                        return Err("No message received in response.".into());
                    }
                } else {
                    println!("Login successful, but response could not be parsed.");
                    return Err("Unable to parse response.".into());
                }
            }
            401 => {
                println!("Login failed: Invalid password. Please try again.");
                return Err("Invalid password.".into());
            }
            404 => {
                println!("Login failed: Client ID not found. Please check your input.");
                return Err("Client ID not found.".into());
            }
            500 => {
                println!("Server error: Something went wrong on the server. Please try again later.");
                return Err("Server error.".into());
            }
            _ => {
                println!(
                    "Unexpected response: {}. Please contact support if the issue persists.",
                    status
                );
                return Err(format!("Unexpected response: {}", status).into());
            }
        }
    } else {
        return Err("DOS port was not determined. Exiting...".into());
    }
}




async fn view_gallery() -> Result<(), Box<dyn Error>> {
    println!("Gallery viewing logic here");

    loop {
        println!("Choose an option:");
        println!("1: Send a request");
        println!("2: Return to main menu");

        let mut gallery_choice = String::new();
        io::stdin().read_line(&mut gallery_choice).expect("Failed to read line");

        match gallery_choice.trim() {
            "1" => {
                println!("Send request logic here");
                // Implement logic to send a request
            },
            "2" => break,
            _ => println!("Invalid choice! Please select again."),
        }
    }

    Ok(())
}

pub async fn register_client_to_dos(leader_address: &str) -> Result<(String, String, String), Box<dyn Error>> {
    let leader_port: u16 = leader_address.split(':')
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);

    if let Some(port) = match leader_port {
        8080 => Some(8083),
        8081 => Some(8084),
        8082 => Some(8085),
        _ => {
            eprintln!("Unknown leader port: {}", leader_port);
            None
        }
    } {
        let dos_address = format!("http://localhost:{}", port);
        println!("DOS Address determined: {}", dos_address);

        let mut client_id = String::new();
        print!("Enter client ID: ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut client_id)?;
        let client_id = client_id.trim_end().to_string();

        let mut password = String::new();
        print!("Enter password: ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut password)?;
        let password = password.trim_end().to_string();

        register_client(&dos_address, &client_id, &password).await?;
        if let Ok(local_ip) = get_local_ip().await {
            match update_client_ip(&dos_address, &client_id, &local_ip.to_string()).await {
                Ok(response) => println!("Client IP updated successfully: {}", response),
                Err(e) => eprintln!("Error updating client IP: {}", e),
            }
        }
        Ok((dos_address, client_id, password))
    } else {
        Err("DOS port was not determined. Exiting...".into())
    }
}


pub async fn update_client_ip(
    dos_address: &str,
    client_id: &str,
    current_ip: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    let payload = json!({
        "id": client_id,
        "current_ip": current_ip
    });

    let response = client
        .post(format!("{}/update_ip", dos_address))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(format!(
            "Failed to update IP (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}

async fn get_local_ip() -> Result<IpAddr, Box<dyn Error>> {
    // Use a UDP socket to determine the local IP address
    let socket = StdUdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?; // Connect to a public DNS server to resolve the local address
    let local_addr = socket.local_addr()?;
    Ok(local_addr.ip())
}


async fn request_leader(socket: &UdpSocket) -> Result<String, Box<dyn Error>> {
    let request_message = "REQUEST_LEADER";

    // Send request to all servers
    for server in &SERVER_ADDRS {
        socket.send_to(request_message.as_bytes(), server).await?;
        println!("Sent CPU usage request to {}", server);
    }

    // Wait to receive the leader address
    let mut buffer = [0; 1024];
    match timeout(TIMEOUT_DURATION, socket.recv_from(&mut buffer)).await {
        Ok(Ok((len, addr))) => {
            let response = String::from_utf8_lossy(&buffer[..len]);
            if response.starts_with("LEADER,") {
                let leader_address = response[7..].to_string(); // Extracting the leader address
                println!("Received leader address: {} from {}", leader_address, addr);
                Ok(leader_address)
            } else {
                Err("Unexpected response received".into())
            }
        },
        Ok(Err(e)) => Err(format!("Failed to receive response: {}", e).into()),
        Err(_) => Err("Timeout waiting for leader response".into()),
    }
}

async fn middleware_encrypt(socket: &UdpSocket, leader_addr: &str) -> tokio::io::Result<()> {
    let image_path = "image2.jpg"; // Path to the image file
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

async fn receive_ack(socket: &UdpSocket) -> bool {
    let mut buffer = [0; 1024];

    match timeout(Duration::from_secs(2), socket.recv_from(&mut buffer)).await {
        Ok(Ok((len, _))) => {
            let response = String::from_utf8_lossy(&buffer[..len]);
            return response.trim() == "ACK";
        },
        _ => false, // Timeout or error
    }
}

/// Receive and decode an image from the server.
async fn middleware_decrypt(socket: &UdpSocket) -> Result<(), Box<dyn Error>> {
    receive_image(socket).await?;
    decode_image("encoded_image_received.png").await?;
    Ok(())
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

pub async fn add_image_to_dos(
    dos_address: &str,
    client_id: &str,
    password: &str,
    image_path: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    let resized_image = resize_image(image_path)?;
    let base64_image = encode_image_to_base64(&resized_image)?;

    let payload = json!({
        "client_id": client_id,
        "password": password,
        "image_name": image_path,
        "image_data": base64_image
    });

    let response = client
        .post(format!("{}/add_image", dos_address))
        .json(&payload)
        .send()
        .await?;

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

fn resize_image(image_path: &str) -> Result<DynamicImage, Box<dyn Error>> {
    let image = ImageReader::open(image_path)?.decode()?;
    let resized_image = image.resize_exact(200, 200, image::imageops::FilterType::Triangle);
    Ok(resized_image)
}

fn encode_image_to_base64(image: &DynamicImage) -> Result<String, Box<dyn Error>> {
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer); // Wrap Vec<u8> in Cursor
    image.write_to(&mut cursor, ImageOutputFormat::Png)?;
    Ok(base64::encode(&buffer))
}

pub async fn register_client(
    dos_address: &str,
    client_id: &str,
    password: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    let payload = json!({
        "id": client_id,
        "password": password
    });

    let response = client
        .post(format!("{}/register_client", dos_address))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(format!(
            "Failed to register client (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}

pub async fn delete_image_from_dos(
    dos_address: &str,
    client_id: &str,
    password: &str,
    image_name: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    let payload = json!({
        "client_id": client_id,
        "password": password,
        "image_name": image_name,
    });

    let response = client
        .post(format!("{}/delete_image", dos_address))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(format!(
            "Failed to delete image (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}

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