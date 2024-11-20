use reqwest::Client;
use tokio_tungstenite::connect_async;
use futures_util::stream::StreamExt;
use serde_json::json;

#[tokio::main]
async fn main() {
    let client = Client::new();

    // Test Add Image - Image 1
    let add_image1_response = client.post("http://localhost:3030/add_image")
        .json(&json!({
            "id": "1",
            "client_id": "client1",
            "image_path": "/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/image1.jpg"
        }))
        .send()
        .await;

    match add_image1_response {
        Ok(res) => {
            println!("Add Image 1 Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error adding image 1: {:?}", e),
    }

    // Test Add Image - Image 2
    let add_image2_response = client.post("http://localhost:3030/add_image")
        .json(&json!({
            "id": "2",
            "client_id": "client1",
            "image_path": "/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/image2.jpg"
        }))
        .send()
        .await;

    match add_image2_response {
        Ok(res) => {
            println!("Add Image 2 Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error adding image 2: {:?}", e),
    }

    // Test Add Image - Image 3
    let add_image3_response = client.post("http://localhost:3030/add_image")
        .json(&json!({
            "id": "3",
            "client_id": "client1",
            "image_path": "/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/image3.jpg"
        }))
        .send()
        .await;

    match add_image3_response {
        Ok(res) => {
            println!("Add Image 3 Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error adding image 3: {:?}", e),
    }


    // Test Add Image - Image 4
    let add_image4_response = client.post("http://localhost:3030/add_image")
        .json(&json!({
            "id": "4",
            "client_id": "client1",
            "image_path": "/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/image4.jpg"
        }))
        .send()
        .await;

    match add_image4_response {
        Ok(res) => {
            println!("Add Image 4 Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error adding image 4: {:?}", e),
    }


    // Test Delete Image - Image 2
    let delete_image2_response = client.post("http://localhost:3030/delete_image")
        .json(&json!({
            "id": "2",  // Send id as a string
            "client_id": "client1"
        }))
        .send()
        .await;

    match delete_image2_response {
        Ok(res) => {
            println!("Delete Image 2 Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error deleting image 2: {:?}", e),
    }

    // Test Preview Combined Images
    println!("Requesting preview of combined images...");
    let preview_response = client.get("http://localhost:3030/preview")
        .send()
        .await;

    match preview_response {
        Ok(res) => {
            println!("Preview Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error generating preview: {:?}", e),
    }

    // Test List All Clients and Images
    let list_all_response = client.get("http://localhost:3030/list_all")
        .send()
        .await;

    match list_all_response {
        Ok(res) => {
            println!("List All Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error listing all: {:?}", e),
    }

    // Test WebSocket Connection
    let ws_url = "ws://localhost:3030/ws";
    match connect_async(ws_url).await {
        Ok((mut ws_stream, _)) => {
            println!("Connected to WebSocket!");

            // Listen for a single message
            if let Some(Ok(msg)) = ws_stream.next().await {
                println!("WebSocket Message: {}", msg);
            }

            // Close the WebSocket connection
            ws_stream.close(None).await.unwrap();
        }
        Err(e) => println!("WebSocket connection failed: {:?}", e),
    }
}
