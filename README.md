# PhotoCataloger

A Rust application that catalogs images in a directory, extracting and storing metadata such as dimensions, format, creation date, and file information in a SQLite database.

## Features

- Recursively scans directories for image files
- Supports common image formats (JPG, JPEG, PNG, GIF, BMP, WebP)
- Extracts image metadata including:
  - File path and name
  - File size
  - Image dimensions
  - Image format
  - Creation date (from EXIF data if available)
- Stores all information in a SQLite database

## Prerequisites

- Rust (edition 2021)
- Cargo (Rust's package manager)
- SQLite

## Installation

1. Clone the repository:
```bash
git clone [repository-url]
cd PhotoCataloger
```

2. Build the project:
```bash
cargo build --release
```

## Usage

1. Create a directory named `images` in the project root (or modify the path in `main.rs`)
2. Place your images in the `images` directory
3. Run the application:
```bash
cargo run --release
```

The program will:
- Create a SQLite database named `image_catalog.db` if it doesn't exist
- Scan the `images` directory recursively
- Process all supported image files
- Store metadata in the database

## Development

### Building
```bash
# Debug build
cargo build

# Release build
cargo build --release
```

### Running Tests
```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

### Dependencies

- walkdir (2.5.0): Directory traversal
- rusqlite (0.35.0): SQLite database operations
- image (0.25.6): Image processing and metadata extraction
- kamadak-exif (0.6.1): EXIF data extraction
- anyhow (1.0.98): Error handling

## Error Handling

The application uses the `anyhow` crate for error handling and will:
- Create the images directory if it doesn't exist
- Skip files it cannot process
- Print error messages for problematic files
- Continue processing remaining files even if some fail

## Notes

- The application creates an `images` directory in the project root if it doesn't exist
- The SQLite database is created as `image_catalog.db` in the project root
- Modify the `dir_path` in `main.rs` to scan a different directory