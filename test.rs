use reqwest::Client;
use tokio_tungstenite::connect_async;
use futures_util::stream::StreamExt;
use serde_json::json;

#[tokio::main]
async fn main() {
    let client = Client::new();

    // Test Add Image
    let add_image_response = client.post("http://localhost:3030/add_image")
        .json(&json!({
            "client_id": "client1",
            "image_name": "image1.jpg"
        }))
        .send()
        .await;

    match add_image_response {
        Ok(res) => {
            println!("Add Image Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error adding image: {:?}", e),
    }

    // Add another image for the same client
    let add_another_image_response = client.post("http://localhost:3030/add_image")
        .json(&json!({
            "client_id": "client1",
            "image_name": "image2.jpg"
        }))
        .send()
        .await;

    match add_another_image_response {
        Ok(res) => {
            println!("Add Another Image Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error adding another image: {:?}", e),
    }

    // Test Delete Image
    let delete_image_response = client.post("http://localhost:3030/delete_image")
        .json(&json!({
            "client_id": "client1",
            "image_name": "image1.jpg"
        }))
        .send()
        .await;

    match delete_image_response {
        Ok(res) => {
            println!("Delete Image Response: {:?}", res.text().await.unwrap());
        }
        Err(e) => println!("Error deleting image: {:?}", e),
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
