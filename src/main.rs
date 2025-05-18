use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;
use rusqlite::{Connection, Result};
use image::ImageFormat;
use exif::{Reader, In};
use anyhow::Error;

struct ImageMetadata {
    path: String,
    file_name: String,
    file_size: u64,
    dimensions: Option<(u32, u32)>,
    format: Option<ImageFormat>,
    creation_date: Option<String>,
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
            creation_date TEXT
        )",
        [],
    )?;
    Ok(())
}

fn process_image(path: &Path) -> Result<ImageMetadata, Error> {
    let file_name = path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();

    // Get image dimensions and format
    let dimensions = image::image_dimensions(path).ok();
    let format = ImageFormat::from_path(path).ok();

    // Try to read EXIF data
    let file = std::fs::File::open(path)?;
    let creation_date = Reader::new()
        .read_from_container(&mut std::io::BufReader::new(&file))
        .ok()
        .and_then(|exif| {
            exif.get_field(exif::Tag::DateTimeOriginal)
                .map(|field| field.display_value().to_string())
        });

    Ok(ImageMetadata {
        path: path.to_string_lossy().into_owned(),
        file_name,
        file_size,
        dimensions,
        format,
        creation_date,
    })
}

fn scan_directory(dir_path: &Path, conn: &Connection) -> Result<(), Error> {
    for entry in WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let path = entry.path();
            if let Some(extension) = path.extension() {
                if matches!(extension.to_string_lossy().to_lowercase().as_str(), 
                          "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp") {
                    match process_image(path) {
                        Ok(metadata) => {
                            if let Some((width, height)) = metadata.dimensions {
                                conn.execute(
                                    "INSERT INTO images (path, file_name, file_size, width, height, format, creation_date)
                                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                                    [
                                        &metadata.path,
                                        &metadata.file_name,
                                        &metadata.file_size.to_string(),
                                        &width.to_string(),
                                        &height.to_string(),
                                        &metadata.format.map(|f| f.to_string()).unwrap_or_default(),
                                        &metadata.creation_date.unwrap_or_default(),
                                    ],
                                )?;
                            }
                        }
                        Err(e) => eprintln!("Error processing {}: {}", path.display(), e),
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Error> {
    // Create or open SQLite database
    let conn = Connection::open("image_catalog.db")?;

    // Initialize database schema
    init_database(&conn)?;

    // Specify the directory to scan
    let dir_path = PathBuf::from("./images"); // Change this to your image directory

    // Create directory if it doesn't exist
    fs::create_dir_all(&dir_path)?;

    // Scan directory and process images
    scan_directory(&dir_path, &conn)?;

    println!("Image cataloging complete!");
    Ok(())
}
