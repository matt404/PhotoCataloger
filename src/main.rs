use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;
use rusqlite::{Connection, Result};
use image::ImageFormat;
use exif::{Reader, In};
use anyhow::Error;
use serde_json::Value;
use reqwest;
use image::GenericImageView;
use std::env;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

struct ImageMetadata {
    path: String,
    file_name: String,
    file_size: u64,
    dimensions: Option<(u32, u32)>,
    format: Option<ImageFormat>,
    creation_date: Option<String>,
    keywords: Option<String>,
    description: Option<String>,
}

fn init_database(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS images (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL,
            file_name TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            width INTEGER,
            height INTEGER,
            format TEXT,
            creation_date TEXT,
            keywords TEXT,
            description TEXT
        )",
        [],
    )?;
    Ok(())
}

async fn get_image_analysis(image_path: &Path) -> Result<(String, String), Error> {
    // Read the image file as base64
    let image_data = fs::read(image_path)?;
    let base64_image = STANDARD.encode(image_data);

    // Create the client
    let client = reqwest::Client::new();

    // Prepare the prompt
    let prompt = format!(
        "Analyze this image and provide: \
        1. A concise description of what you see \
        2. A list of relevant keywords separated by commas"
    );

    // Make request to local Ollama server
    let response = client
        .post("http://localhost:11434/api/generate")
        .header("Content-Type", "application/json")
        .body(serde_json::json!({
            "model": "llava",
            "prompt": prompt,
            "images": [base64_image],
            "stream": false
        }).to_string())
        .send()
        .await?;

    // Get the response text as a String
    let response_text = response.text().await?;

    // Parse the JSON response
    let response_json: Value = serde_json::from_str(&response_text)?;

    // Extract the response field and convert to owned String
    let full_response = response_json["response"]
        .as_str()
        .unwrap_or("No response")
        .to_owned();

    // Split the response into description and keywords
    // Split the response into description and keywords
    let parts: Vec<&str> = full_response.split("\n\n").collect();
    let description = parts.get(0).unwrap_or(&"").to_string();  // Changed to to_string()
    let keywords = parts
        .get(1)
        .unwrap_or(&"")
        .trim_start_matches("Keywords: ")
        .to_string();  // Changed to to_string()
    
    // print keywords and description
    println!("Keywords: {}", keywords);
    println!("Description: {}", description);

    Ok((description, keywords))
}

fn process_image(path: &Path) -> Result<ImageMetadata, Error> {
    let file_name = path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();
    
    // Instead of trying to get format from DynamicImage
    let file = fs::read(path)?;
    let img = image::load_from_memory(&file).ok();
    let dimensions = img.as_ref().map(|img| img.dimensions());
    let format = image::guess_format(&file).ok();

    // Get EXIF data for creation date
    let file = fs::File::open(path)?;
    let creation_date = Reader::new()
        .read_from_container(&mut std::io::BufReader::new(&file))
        .ok()
        .and_then(|exif| {
            exif.get_field(exif::Tag::DateTimeOriginal, In::PRIMARY)
                .map(|field| field.display_value().to_string())
        });

    // Get image analysis from Ollama
    let rt = tokio::runtime::Runtime::new()?;
    let (description, keywords) = rt.block_on(get_image_analysis(path))?;

    Ok(ImageMetadata {
        path: path.to_string_lossy().into_owned(),
        file_name,
        file_size,
        dimensions,
        format,
        creation_date,
        keywords: Some(keywords),
        description: Some(description),
    })
}

fn save_metadata(conn: &Connection, metadata: &ImageMetadata) -> Result<()> {
    conn.execute(
        "INSERT INTO images (
            path, file_name, file_size, width, height, format,
            creation_date, keywords, description
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            metadata.path,
            metadata.file_name,
            metadata.file_size,
            metadata.dimensions.map(|(w, _)| w),
            metadata.dimensions.map(|(_, h)| h),
            metadata.format.map(|f| format!("{:?}", f)),
            metadata.creation_date,
            metadata.keywords,
            metadata.description,
        ],
    )?;
    Ok(())
}

fn main() -> Result<(), Error> {
    // Get the directory to scan from command line argument or use current directory
    let scan_dir = env::args().nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap());

    println!("Scanning directory: {}", scan_dir.display());

    // Initialize SQLite database
    let conn = Connection::open("photo_catalog.db")?;
    init_database(&conn)?;

    // Count for processed images
    let mut processed_count = 0;

    // Walk through the directory
    for entry in WalkDir::new(scan_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension()
                .map(|ext| {
                    let ext = ext.to_string_lossy().to_lowercase();
                    matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "bmp")
                })
                .unwrap_or(false)
        })
    {
        match process_image(entry.path()) {
            Ok(metadata) => {
                println!("Processing: {}", entry.path().display());
                if let Err(e) = save_metadata(&conn, &metadata) {
                    eprintln!("Error saving metadata for {}: {}", entry.path().display(), e);
                } else {
                    processed_count += 1;
                }
            }
            Err(e) => {
                eprintln!("Error processing {}: {}", entry.path().display(), e);
            }
        }
    }

    println!("Successfully processed {} images", processed_count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;
    use mockito::Server;
    use tokio;

    #[test]
    fn test_init_database() -> Result<(), Error> {
        let conn = Connection::open_in_memory()?;
        init_database(&conn)?;

        // Verify table exists and has correct schema
        let mut stmt = conn.prepare("PRAGMA table_info(images)")?;
        let columns: Vec<(i32, String)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?.collect::<Result<Vec<_>, _>>()?;

        // Check for all expected columns
        let expected_columns = vec![
            "id", "path", "file_name", "file_size", "width", "height",
            "format", "creation_date", "keywords", "description"
        ];

        assert_eq!(columns.len(), expected_columns.len());
        for (i, col_name) in expected_columns.iter().enumerate() {
            assert_eq!(columns[i].1, *col_name);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_get_image_analysis() -> Result<(), Error> {
        // Create a mock server
        let mut server = Server::new();

        // Create a mock response
        let mock_response = r#"{
            "model": "llava",
            "response": "A colorful sunset over mountains\n\nKeywords: sunset, mountains, nature, landscape, evening, colorful"
        }"#;

        // Set up the mock endpoint
        let _m = server.mock("POST", "/api/generate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_response)
            .create();

        // Create a temporary test image
        let dir = tempdir()?;
        let test_image_path = dir.path().join("test.jpg");
        let mut test_image = File::create(&test_image_path)?;

        // Write some dummy image data
        test_image.write_all(&[0xFF, 0xD8, 0xFF, 0xE0])?; // JPEG header
        test_image.sync_all()?;

        // Test the analysis function
        let (description, keywords) = get_image_analysis(&test_image_path).await?;

        assert_eq!(description, "A colorful sunset over mountains");
        assert_eq!(
            keywords,
            "sunset, mountains, nature, landscape, evening, colorful"
        );

        Ok(())
    }

    #[test]
    fn test_process_image() -> Result<(), Error> {
        let dir = tempdir()?;
        let test_image_path = dir.path().join("test.jpg");

        // Create a test JPEG image
        let mut test_image = File::create(&test_image_path)?;
        test_image.write_all(&[0xFF, 0xD8, 0xFF, 0xE0])?; // JPEG header
        test_image.sync_all()?;

        let metadata = process_image(&test_image_path)?;

        assert_eq!(metadata.file_name, "test.jpg");
        assert_eq!(metadata.file_size, 4); // Size of our minimal JPEG header
        assert!(metadata.keywords.is_some());
        assert!(metadata.description.is_some());

        Ok(())
    }

    #[test]
    fn test_save_metadata() -> Result<(), Error> {
        let conn = Connection::open_in_memory()?;
        init_database(&conn)?;

        let metadata = ImageMetadata {
            path: String::from("/test/path"),
            file_name: String::from("test.jpg"),
            file_size: 1000,
            dimensions: Some((800, 600)),
            format: Some(ImageFormat::Jpeg),
            creation_date: Some(String::from("2024-01-01 00:00:00")),
            keywords: Some(String::from("test, image, mock")),
            description: Some(String::from("A test image")),
        };

        save_metadata(&conn, &metadata)?;

        // Verify the saved data
        let mut stmt = conn.prepare("SELECT * FROM images WHERE file_name = 'test.jpg'")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?, // path
                row.get::<_, String>(2)?, // file_name
                row.get::<_, i64>(3)?,    // file_size
                row.get::<_, i64>(4)?,    // width
                row.get::<_, i64>(5)?,    // height
                row.get::<_, String>(8)?, // keywords
                row.get::<_, String>(9)?, // description
            ))
        })?.collect::<Result<Vec<_>, _>>()?;

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.0, "/test/path");
        assert_eq!(row.1, "test.jpg");
        assert_eq!(row.2, 1000);
        assert_eq!(row.3, 800);
        assert_eq!(row.4, 600);
        assert_eq!(row.5, "test, image, mock");
        assert_eq!(row.6, "A test image");

        Ok(())
    }

    #[test]
    fn test_integration() -> Result<(), Error> {
        let conn = Connection::open_in_memory()?;
        init_database(&conn)?;

        // Create a temporary directory with a test image
        let dir = tempdir()?;
        let test_image_path = dir.path().join("test.jpg");
        let mut test_image = File::create(&test_image_path)?;
        test_image.write_all(&[0xFF, 0xD8, 0xFF, 0xE0])?;
        test_image.sync_all()?;

        // Process and save the image
        let metadata = process_image(&test_image_path)?;
        save_metadata(&conn, &metadata)?;

        // Verify the image was processed and saved
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM images WHERE file_name = 'test.jpg'",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(count, 1);
        Ok(())
    }
}
