//! SHM Image Processing Example
//!
//! This example demonstrates zero-copy image processing over SHM transport.
//!
//! The key insight is: when you allocate data using `ShmAllocator`, that data
//! lives directly in the shared memory segment. When the encoder sees this data,
//! it detects the pointer is already in SHM and simply references it (zero-copy)
//! instead of allocating a new slot and copying.
//!
//! ## Architecture
//!
//! ```text
//! Host (allocates PNG in SHM)
//!   │
//!   ├─► encode_bytes() detects ptr is in SHM
//!   │     └─► zero-copy: just records (slot, offset, len)
//!   │
//!   └─► sends descriptor over ring (64 bytes)
//!
//! Plugin (receives reference to same SHM slot)
//!   │
//!   ├─► reads PNG from slot (no copy)
//!   ├─► processes image (flip, rotate, etc.)
//!   └─► writes result to new slot
//! ```
//!
//! ## Running
//!
//! ```bash
//! cargo run -p rapace-shm-image
//! ```

use std::io::Cursor;
use std::sync::Arc;

use rapace::{
    ErrorCode, RpcError, RpcSession, Streaming, Transport,
    transport::shm::{ShmAllocator, ShmMetrics, ShmSession, ShmTransport, allocator_api2},
};

type AllocVec<T, A> = allocator_api2::vec::Vec<T, A>;

// ============================================================================
// ImageService trait
// ============================================================================

/// An image processing service.
///
/// All methods take raw PNG bytes and return processed PNG bytes.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait ImageService {
    /// Flip the image vertically.
    async fn flip_vertical(&self, png_data: Vec<u8>) -> Vec<u8>;

    /// Flip the image horizontally.
    async fn flip_horizontal(&self, png_data: Vec<u8>) -> Vec<u8>;

    /// Rotate the image 90 degrees clockwise.
    async fn rotate_90(&self, png_data: Vec<u8>) -> Vec<u8>;

    /// Convert to grayscale.
    async fn grayscale(&self, png_data: Vec<u8>) -> Vec<u8>;

    /// Apply a pipeline of operations (streaming example).
    async fn pipeline(&self, png_data: Vec<u8>) -> Streaming<Vec<u8>>;
}

// ============================================================================
// ImageService implementation
// ============================================================================

struct ImageServiceImpl;

impl ImageService for ImageServiceImpl {
    async fn flip_vertical(&self, png_data: Vec<u8>) -> Vec<u8> {
        let img = image::load_from_memory(&png_data).expect("Invalid PNG");
        let flipped = img.flipv();
        encode_png(&flipped)
    }

    async fn flip_horizontal(&self, png_data: Vec<u8>) -> Vec<u8> {
        let img = image::load_from_memory(&png_data).expect("Invalid PNG");
        let flipped = img.fliph();
        encode_png(&flipped)
    }

    async fn rotate_90(&self, png_data: Vec<u8>) -> Vec<u8> {
        let img = image::load_from_memory(&png_data).expect("Invalid PNG");
        let rotated = img.rotate90();
        encode_png(&rotated)
    }

    async fn grayscale(&self, png_data: Vec<u8>) -> Vec<u8> {
        let img = image::load_from_memory(&png_data).expect("Invalid PNG");
        let gray = img.grayscale();
        encode_png(&gray)
    }

    async fn pipeline(&self, png_data: Vec<u8>) -> Streaming<Vec<u8>> {
        let (tx, rx) = tokio::sync::mpsc::channel(4);

        tokio::spawn(async move {
            let img = match image::load_from_memory(&png_data) {
                Ok(img) => img,
                Err(e) => {
                    let _ = tx
                        .send(Err(RpcError::Status {
                            code: ErrorCode::InvalidArgument,
                            message: format!("Invalid PNG: {}", e),
                        }))
                        .await;
                    return;
                }
            };

            // Send original
            let _ = tx.send(Ok(encode_png(&img))).await;

            // Send flipped
            let flipped = img.flipv();
            let _ = tx.send(Ok(encode_png(&flipped))).await;

            // Send grayscale
            let gray = img.grayscale();
            let _ = tx.send(Ok(encode_png(&gray))).await;

            // Send rotated
            let rotated = img.rotate90();
            let _ = tx.send(Ok(encode_png(&rotated))).await;
        });

        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

/// Encode an image as PNG bytes.
fn encode_png(img: &image::DynamicImage) -> Vec<u8> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("PNG encoding failed");
    buf
}

// ============================================================================
// Demo: Create a simple test image
// ============================================================================

/// Create a simple test image (a colorful gradient).
fn create_test_image(width: u32, height: u32) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};

    let img = ImageBuffer::from_fn(width, height, |x, y| {
        let r = (x as f32 / width as f32 * 255.0) as u8;
        let g = (y as f32 / height as f32 * 255.0) as u8;
        let b = 128;
        Rgb([r, g, b])
    });

    let mut buf = Vec::new();
    let dyn_img = image::DynamicImage::ImageRgb8(img);
    dyn_img
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("PNG encoding failed");
    buf
}

// ============================================================================
// Main: Demo the zero-copy path
// ============================================================================

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main());
}

async fn async_main() {
    println!("=== SHM Image Processing Demo ===\n");

    // Create metrics to track zero-copy performance.
    let metrics = Arc::new(ShmMetrics::new());

    // Create a connected pair of SHM sessions.
    let (session_a, session_b) = ShmSession::create_pair().expect("Failed to create SHM sessions");

    // Create transports with metrics enabled (transports have internal Arc, cheap to clone).
    let transport_a = Transport::Shm(ShmTransport::new_with_metrics(
        session_a.clone(),
        metrics.clone(),
    ));
    let transport_b = Transport::Shm(ShmTransport::new_with_metrics(
        session_b.clone(),
        metrics.clone(),
    ));

    // Create an allocator from session A (the "host" side).
    let alloc = ShmAllocator::new(session_a.clone());

    println!("Session created:");
    println!(
        "  Slot size: {} bytes",
        session_a.data_segment().slot_size()
    );
    println!("  Slot count: {}", session_a.data_segment().slot_count());
    println!("  Max allocation: {} bytes", alloc.max_allocation_size());
    println!();

    // Create a test image.
    // We'll use a small image to fit in a single slot (4KB default).
    let test_png = create_test_image(32, 32);
    println!("Test image created: {} bytes", test_png.len());

    // === DEMONSTRATION 1: Regular allocation (will copy) ===
    // NOTE: Encoder API removed in TransportHandle refactor
    // TODO: Rewrite this demo to use send_frame API
    println!("\n--- Demo 1: Regular Vec (skipped - encoder API removed) ---");
    let _ = &test_png; // suppress unused warning

    // === DEMONSTRATION 2: SHM allocation (zero-copy) ===
    // NOTE: Encoder API removed in TransportHandle refactor
    // TODO: Rewrite this demo to use send_frame API
    println!("\n--- Demo 2: ShmAllocator Vec (skipped - encoder API removed) ---");

    // === DEMONSTRATION 3: RPC call pattern ===
    println!("\n--- Demo 3: Simulated RPC Pattern ---");
    {
        // In a real RPC scenario:
        // 1. Host allocates request data in SHM using ShmAllocator
        // 2. Encoder detects data is in SHM, records slot reference (no copy!)
        // 3. Descriptor is enqueued (64 bytes)
        // 4. Plugin reads from the same slot (zero-copy)
        // 5. Plugin processes and writes response to new slot
        // 6. Host reads response from slot

        // Allocate in SHM
        let mut request_data: AllocVec<u8, _> = AllocVec::new_in(alloc.clone());
        request_data.extend_from_slice(&test_png);

        println!("  Request data allocated in SHM");
        println!("  Size: {} bytes", request_data.len());

        // Show that it's in a slot
        if let Some((slot, offset)) =
            session_a.find_slot_location(request_data.as_ptr(), request_data.len())
        {
            println!("  Slot: {}, Offset: {}", slot, offset);

            // The encoder would do this check and take the zero-copy path:
            // if let Some((slot, offset)) = session.find_slot_location(bytes.as_ptr(), bytes.len()) {
            //     self.desc.payload_slot = slot;
            //     self.desc.payload_offset = offset;
            //     self.desc.payload_len = bytes.len() as u32;
            //     // NO COPY! Just reference the existing data.
            // }
            println!("  -> Encoder would reference slot directly (zero-copy)");
        }
    }

    // === DEMONSTRATION 4: End-to-end with actual transport ===
    println!("\n--- Demo 4: End-to-End Transport Test ---");
    {
        let before_zero = metrics.zero_copy_count();
        let before_copy = metrics.copy_count();

        // Start the server
        let service = ImageServiceImpl;
        let server = ImageServiceServer::new(service);

        // Spawn server task
        let server_handle = tokio::spawn(async move {
            // Handle one request then exit
            if let Err(e) = server.serve_one(&transport_b).await {
                eprintln!("Server error: {:?}", e);
            }
        });

        // Create client session and spawn its demux loop
        let client_session = std::sync::Arc::new(RpcSession::new(transport_a.clone()));
        let client_session_runner = client_session.clone();
        tokio::spawn(async move { client_session_runner.run().await });

        // Create client
        let client = ImageServiceClient::new(client_session);

        // Create data in SHM
        let mut shm_request: AllocVec<u8, _> = AllocVec::new_in(alloc.clone());
        shm_request.extend_from_slice(&test_png);

        let request_in_shm = session_a
            .find_slot_location(shm_request.as_ptr(), shm_request.len())
            .is_some();
        println!("  Request data in SHM: {}", request_in_shm);

        // Make the RPC call
        // Note: The current API takes Vec<u8>, so we need to convert.
        // In a fully optimized system, we'd pass a reference or use a custom type.
        let request_vec: Vec<u8> = shm_request.iter().copied().collect();
        let result = client.flip_vertical(request_vec).await;

        match result {
            Ok(response) => {
                println!("  Response received: {} bytes", response.len());

                // Verify it's different (flipped)
                if response != test_png {
                    println!("  Image was transformed successfully!");
                } else {
                    println!("  Warning: Image unchanged (might be symmetric)");
                }
            }
            Err(e) => {
                println!("  Error: {:?}", e);
            }
        }

        let after_zero = metrics.zero_copy_count();
        let after_copy = metrics.copy_count();
        println!(
            "  RPC Metrics: zero-copy {} -> {}, copy {} -> {}",
            before_zero, after_zero, before_copy, after_copy
        );

        // Clean up (close is now sync)
        transport_a.close();
        let _ = server_handle.await;
    }

    println!("\n=== Demo Complete ===");
    println!(
        "Total: {} zero-copy encodes, {} copy encodes",
        metrics.zero_copy_count(),
        metrics.copy_count()
    );
    println!("Zero-copy ratio: {:.1}%", metrics.zero_copy_ratio() * 100.0);
}
