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

const SERVER_ADDRS: [&str; 3] = ["127.0.0.1:8080", "127.0.0.1:8081", "127.0.0.1:8082"];
const CLIENT_ADDR: &str = "127.0.0.1:0";
const CHUNK_SIZE: usize = 1024;
const TIMEOUT_DURATION: Duration = Duration::from_secs(10);
const ACK: &[u8] = b"ACK";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind(CLIENT_ADDR).await?;

    // Request leader from servers
    let leader_address = request_leader(&socket).await?;


    println!("Welcome! Choose an option:");
    println!("1: Register");
    println!("2: Login");

    let mut option = String::new();
    std::io::stdin().read_line(&mut option).expect("Failed to read line");

    match option.trim() {
        "1" => register_client_to_dos(&leader_address).await?,
        "2" => login().await?,
        _ => println!("Invalid option!"),
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
            "1" => add_image().await?,
            "2" => delete_image().await?,
            "3" => view_gallery().await?,
            "4" => break,
            _ => println!("Invalid choice! Please select again."),
        }
    }

    Ok(())
}

async fn login() -> Result<(), Box<dyn Error>> {
    println!("Login logic here");
    // Implement login logic
    Ok(())
}

async fn add_image() -> Result<(), Box<dyn Error>> {
    println!("Add an image logic here");
    // Implement logic to add an image
    Ok(())
}

async fn delete_image() -> Result<(), Box<dyn Error>> {
    println!("Delete an image logic here");
    // Implement logic to delete an image
    Ok(())
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

async fn register_client_to_dos(leader_address: &str) -> Result<(), Box<dyn Error>> {
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
        let dos_address = format!("http://localhost:{}", port);
        println!("DOS Address determined: {}", dos_address);

        // Get client_id and password from the user
        let mut client_id = String::new();
        print!("Enter client ID: ");
        io::stdout().flush()?; // Ensure prompt is visible before user input
        io::stdin().read_line(&mut client_id)?;
        let client_id = client_id.trim_end(); // Remove any trailing newline

        let mut password = String::new();
        print!("Enter password: ");
        io::stdout().flush()?; // Ensure prompt is visible before user input
        io::stdin().read_line(&mut password)?;
        let password = password.trim_end(); // Remove any trailing newline

        register_client(&dos_address, client_id, password).await?;
        if let Ok(local_ip) = get_local_ip().await {
            match update_client_ip(&dos_address, client_id, &local_ip.to_string()).await {
                Ok(response) => println!("Client IP updated successfully: {}", response),
                Err(e) => eprintln!("Error updating client IP: {}", e),
            }
        }
    } else {
        return Err("DOS port was not determined. Exiting...".into());
    }

    Ok(())
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