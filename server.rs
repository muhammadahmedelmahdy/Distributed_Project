// use tokio::net::UdpSocket; //to provide UDP socket functionality
// use tokio::fs::File; //to enable asynchronous file operations
// use tokio::io::{AsyncWriteExt, BufWriter}; //to enable asynchronous writing to a file
// use std::collections::BTreeMap; //to store received packets in order
// use std::path::Path; //for handling file paths in a platform-independent way
// use std::error::Error; //to handle errors

// #[tokio::main] //to enable the use of async/await in the main function
// async fn main() -> Result<(), Box<dyn Error>> {
//     let socket = initialize_socket("172.18.0.1:8080").await?;
//     let encoded_image_path = Path::new("received_encoded_image.png");
//     let mut file = initialize_file(encoded_image_path).await?;
//     let mut received_packets = BTreeMap::new();
//     let total_bytes_received = receive_data(&socket, &mut received_packets).await?;
//     save_data(&mut file, received_packets).await?;
//     println!(
//         "Image transfer complete. Total bytes received: {}. File saved to {:?}",
//         total_bytes_received, encoded_image_path
//     );
//     Ok(())
// }

// // initialize_socket creates a UDP socket bound to a specified IP address.
// async fn initialize_socket(address: &str) -> Result<UdpSocket, Box<dyn Error>> {
//     let socket = UdpSocket::bind(address).await?;
//     println!("Server listening on {}", address);
//     Ok(socket)
// }

// // initialize_file creates a new file for writing.
// async fn initialize_file(path: &Path) -> Result<BufWriter<File>, Box<dyn Error>> {
//     let file = BufWriter::new(File::create(path).await?);
//     Ok(file)
// }

// // receive_data receives packets from the client and stores them in a BTreeMap.
// // async fn receive_data(socket: &UdpSocket,received_packets: &mut BTreeMap<u32, Vec<u8>>,) -> Result<usize, Box<dyn Error>> {
// //     let mut buffer = [0u8; 1024];
// //     let mut total_bytes_received = 0;

// //     loop {
// //         let (size, src) = socket.recv_from(&mut buffer).await?;
// //         let message = &buffer[..size];

// //         match message {
// //             b"START" => {
// //                 println!("Received START signal from {:?}", src);
// //             }
// //             b"END" => {
// //                 println!("Received END signal from {:?}", src);
// //                 break;
// //             }
// //             _ => {
// //                 let seq_number = extract_sequence_number(message);
// //                 received_packets.insert(seq_number, message[4..].to_vec());
// //                 total_bytes_received += size - 4;
// //                 println!("Received packet with sequence number: {}", seq_number);
// //             }
// //         }
// //     }

// //     Ok(total_bytes_received)
// // }

// // receive_data receives packets from the client and stores them in a BTreeMap.
// async fn receive_data(socket: &UdpSocket, received_packets: &mut BTreeMap<u32, Vec<u8>>,) -> Result<usize, Box<dyn Error>> {
//     let mut buffer = [0u8; 1024];
//     let mut total_bytes_received = 0;
//     let mut end_received = false;

//     loop {
//         let (size, src) = socket.recv_from(&mut buffer).await?;
//         let message = &buffer[..size];

//         match message {
//             b"START" => {
//                 println!("Received START signal from {:?}", src);
//             }
//             b"END" => {
//                 println!("Received END signal from {:?}", src);
//                 socket.send_to(b"END_ACK", src).await?; // Acknowledge END signal
//                 end_received = true;
//                 break;
//             }
//             _ => {
//                 let seq_number = extract_sequence_number(message);
//                 received_packets.insert(seq_number, message[4..].to_vec());
//                 total_bytes_received += size - 4;
//                 println!("Received packet with sequence number: {}", seq_number);
//             }
//         }
//     }

//     // Wait for end acknowledgment before closing
//     if !end_received {
//         println!("WARNING: END signal not received properly.");
//     }

//     Ok(total_bytes_received)
// }


// // extract_sequence_number extracts the sequence number from the packet.
// fn extract_sequence_number(message: &[u8]) -> u32 {
//     u32::from_be_bytes([message[0], message[1], message[2], message[3]])
// }

// // save_data writes the received packets to a file.
// async fn save_data(
//     file: &mut BufWriter<File>,
//     received_packets: BTreeMap<u32, Vec<u8>>,
// ) -> Result<(), Box<dyn Error>> {
//     for (_seq, data) in received_packets {
//         file.write_all(&data).await?;
//     }

//     file.flush().await?;
//     file.shutdown().await?;
//     Ok(())
// }


//NEW CODE FOR HANDLING MISSED PACKETS

use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::error::Error;
use tokio::time::{timeout, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let socket = initialize_socket("172.18.0.1:8080").await?;
    let encoded_image_path = Path::new("received_encoded_image.png");
    let mut file = initialize_file(encoded_image_path).await?;
    let mut received_packets = BTreeMap::new();

    // Track all packets received
    let total_bytes_received = receive_data(&socket, &mut received_packets).await?;

    // Identify missing packets if any and request them
    if let Some(missing_packets) = identify_missing_packets(&received_packets) {
        request_missing_packets(&socket, &missing_packets).await?;
        receive_missing_data(&socket, &mut received_packets, &missing_packets).await?;
    }

    save_data(&mut file, received_packets).await?;
    println!(
        "Image transfer complete. Total bytes received: {}. File saved to {:?}",
        total_bytes_received, encoded_image_path
    );
    Ok(())
}

async fn initialize_socket(address: &str) -> Result<UdpSocket, Box<dyn Error>> {
    let socket = UdpSocket::bind(address).await?;
    println!("Server listening on {}", address);
    Ok(socket)
}

async fn initialize_file(path: &Path) -> Result<BufWriter<File>, Box<dyn Error>> {
    let file = BufWriter::new(File::create(path).await?);
    Ok(file)
}

async fn receive_data(socket: &UdpSocket, received_packets: &mut BTreeMap<u32, Vec<u8>>,) -> Result<usize, Box<dyn Error>> {
    let mut buffer = [0u8; 1024];
    let mut total_bytes_received = 0;

    loop {
        let (size, src) = socket.recv_from(&mut buffer).await?;
        let message = &buffer[..size];

        match message {
            b"START" => {
                println!("Received START signal from {:?}", src);
            }
            b"END" => {
                println!("Received END signal from {:?}", src);
                socket.send_to(b"END_ACK", src).await?;
                break;
            }
            _ => {
                let seq_number = extract_sequence_number(message);
                received_packets.insert(seq_number, message[4..].to_vec());
                total_bytes_received += size - 4;
                println!("Received packet with sequence number: {}", seq_number);
            }
        }
    }

    Ok(total_bytes_received)
}

// Extract missing sequence numbers
fn identify_missing_packets(received_packets: &BTreeMap<u32, Vec<u8>>) -> Option<Vec<u32>> {
    let mut missing = Vec::new();
    let mut expected_seq = 0;

    for seq in received_packets.keys() {
        while expected_seq < *seq {
            missing.push(expected_seq);
            expected_seq += 1;
        }
        expected_seq += 1;
    }
    if !missing.is_empty() { Some(missing) } else { None }
}

// Request retransmission of missing packets
async fn request_missing_packets(socket: &UdpSocket, missing_packets: &[u32]) -> Result<(), Box<dyn Error>> {
    for &seq_number in missing_packets {
        let mut message = vec![0u8; 4];
        message[..4].copy_from_slice(&seq_number.to_be_bytes());
        socket.send(&message).await?;
        println!("Requested missing packet with sequence number: {}", seq_number);
    }
    Ok(())
}

// Receive missing packets based on server request
async fn receive_missing_data(
    socket: &UdpSocket,
    received_packets: &mut BTreeMap<u32, Vec<u8>>,
    missing_packets: &[u32]
) -> Result<(), Box<dyn Error>> {
    let mut buffer = [0u8; 1024];
    let missing_set: HashSet<_> = missing_packets.iter().cloned().collect();

    for _ in 0..missing_packets.len() {
        let (size, _) = socket.recv_from(&mut buffer).await?;
        let seq_number = extract_sequence_number(&buffer[..size]);

        if missing_set.contains(&seq_number) {
            received_packets.insert(seq_number, buffer[4..size].to_vec());
            println!("Received missing packet with sequence number: {}", seq_number);
        }
    }
    Ok(())
}

fn extract_sequence_number(message: &[u8]) -> u32 {
    u32::from_be_bytes([message[0], message[1], message[2], message[3]])
}

async fn save_data(
    file: &mut BufWriter<File>,
    received_packets: BTreeMap<u32, Vec<u8>>,
) -> Result<(), Box<dyn Error>> {
    for (_seq, data) in received_packets {
        file.write_all(&data).await?;
    }

    file.flush().await?;
    file.shutdown().await?;
    Ok(())
}
