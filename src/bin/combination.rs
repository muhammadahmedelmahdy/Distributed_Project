use std::path::{Path, PathBuf};
use image::{DynamicImage, GenericImageView, ImageBuffer, imageops::FilterType};
use std::sync::{Arc, Mutex};
use std::sync::MutexGuard;
use lazy_static::lazy_static;


#[derive(Clone, Debug)]
struct ImageData {
    id: u32,
    owner: String,
    path: PathBuf,
    image: DynamicImage,
}

lazy_static! {
    static ref IMAGES: Mutex<Vec<ImageData>> = Mutex::new(Vec::new());
}


fn prepare_low_quality_image(image: &DynamicImage) -> DynamicImage {
    // Resize the image to a smaller size, e.g., thumbnail size.
    let preview = image.resize_exact(100, 100, FilterType::Triangle); // Better filtering for downsizing

    // Convert to a new DynamicImage to return, simulating lower quality by reducing data.
    // Note: The 'save' function is not used here because we don't write to file yet.
    DynamicImage::ImageRgba8(preview.to_rgba8())
}

/// Adds an image with metadata to the global vector.
fn add_image(id: u32, owner: &str, image_path: &Path) -> Result<(), image::ImageError> {
    let image = image::open(image_path)?;
    let processed_image = prepare_low_quality_image(&image);
    let image_data = ImageData {
        id,
        owner: owner.to_string(),
        path: image_path.to_path_buf(),
        image: processed_image,
    };

    let mut images = IMAGES.lock().unwrap();
    images.push(image_data);
    Ok(())
}

/// Removes an image from the global vector by its ID.
fn delete_image(image_id: u32) {
    let mut images = IMAGES.lock().unwrap();
    images.retain(|img| img.id != image_id);
}

/// Combines multiple images horizontally.
fn combine_images_horizontally() -> Result<DynamicImage, image::ImageError> {
    let images = IMAGES.lock().unwrap();
    let total_width: u32 = images.iter().map(|img_data| img_data.image.width()).sum();
    let max_height: u32 = images.iter().map(|img_data| img_data.image.height()).max().unwrap_or(0);

    let mut new_img = ImageBuffer::new(total_width, max_height);
    let mut x_offset = 0;

    for img_data in &*images {
        for (x, y, pixel) in img_data.image.pixels() {
            if y < max_height {
                new_img.put_pixel(x + x_offset, y, pixel);
            }
        }
        x_offset += img_data.image.width();
    }

    Ok(DynamicImage::ImageRgba8(new_img))
}


fn main() -> Result<(), image::ImageError> {
    // Example usage
    add_image(1, "client1", Path::new("/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/image1.jpg"))?;
    add_image(2, "client2", Path::new("/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/image2.jpg"))?;
    add_image(3, "client3", Path::new("/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/image3.jpg"))?;

    // Preview images before deleting one
    let final_combined_image = combine_images_horizontally()?;
    final_combined_image.save_with_format("/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/firstpreview.jpg", image::ImageFormat::Jpeg)?;

    // Delete an image by ID
    delete_image(2);

    // Preview remaining images
    let final_combined_image = combine_images_horizontally()?;
    final_combined_image.save_with_format("/Users/ahmedgouda/Desktop/distributedProject/Distributed_Project/final_preview_after_deletion.jpg", image::ImageFormat::Jpeg)?;

    println!("Operations completed successfully");
    Ok(())
}