use std::process::Command;
use std::fs;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input_file_path = "/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/image6copy.jpg"; // Replace with your input image path
    let output_file_path = "mock_output_image.png";
    let mut views = 5;

    // Ensure the input image exists
    if !fs::metadata(input_file_path).is_ok() {
        eprintln!("Input image not found. Please provide a valid image file.");
        return Err("Input image not found".into());
    }

    println!("Image decoded successfully and saved to {}", output_file_path);
    views -= 1;

    // Simulate embedding views metadata
    embed_views_metadata(input_file_path, output_file_path, views).await?;

    // Open the image based on the operating system
    if cfg!(target_os = "windows") {
        Command::new("cmd")
            .arg("/C")
            .arg(format!("start {}", output_file_path))
            .spawn()
            .expect("Failed to open image");
    } else if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(output_file_path)
            .spawn()
            .expect("Failed to open image");
    } else if cfg!(target_os = "linux") {
        Command::new("xdg-open")
            .arg(output_file_path)
            .spawn()
            .expect("Failed to open image");
    } else {
        eprintln!("Unsupported platform for image viewer");
    }

    Ok(())
}

// Simulate embedding metadata into the output image
async fn embed_views_metadata(
    input_file_path: &str,
    output_file_path: &str,
    views: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Embedding metadata: input file = {}, output file = {}, views = {}",
        input_file_path, output_file_path, views
    );

    // Copy the input file to the output file as a mock operation
    let mut input_data = tokio::fs::File::open(input_file_path).await?;
    let mut output_data = tokio::fs::File::create(output_file_path).await?;

    tokio::io::copy(&mut input_data, &mut output_data).await?;
    output_data
        .write_all(format!("\nMetadata: views = {}", views).as_bytes())
        .await?;

    Ok(())
}
