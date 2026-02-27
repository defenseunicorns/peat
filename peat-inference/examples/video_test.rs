//! Test video file input with GStreamer
//!
//! Usage: cargo run --example video_test -- /path/to/video.mp4
//!
//! This example demonstrates:
//! - Loading a video file via GStreamer
//! - Extracting frames as RGB data
//! - Basic frame statistics

use peat_inference::inference::{VideoConfig, VideoInput, VideoSource};
use std::env;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("peat_inference=info".parse()?),
        )
        .init();

    // Get video file path from args
    let args: Vec<String> = env::args().collect();
    let video_path = if args.len() > 1 {
        &args[1]
    } else {
        eprintln!("Usage: {} <video_file>", args[0]);
        eprintln!();
        eprintln!("Example: {} /path/to/test.mp4", args[0]);
        eprintln!();
        eprintln!("Note: You can download a test video with:");
        eprintln!("  wget -O test.mp4 'https://www.pexels.com/download/video/855564/'");
        std::process::exit(1);
    };

    println!("Video Pipeline Test");
    println!("==================");
    println!("Video file: {}", video_path);
    println!();

    // Create video source config
    let config = VideoConfig::video_file(video_path, false)
        .with_platform_id("test-platform")
        .with_position(33.7749, -84.3958, 0.0); // Atlanta

    // Create video source
    let mut source = VideoSource::new(config)?;

    println!("Starting video pipeline...");
    let start = Instant::now();
    source.start().await?;

    let mut frame_count = 0u64;
    let mut total_bytes = 0usize;

    println!();
    println!("Processing frames (press Ctrl+C to stop):");
    println!();

    // Process frames
    while let Some(frame) = source.next_frame().await? {
        frame_count += 1;
        total_bytes += frame.data.len();

        // Print progress every 30 frames
        if frame_count % 30 == 0 || frame_count == 1 {
            let elapsed = start.elapsed().as_secs_f64();
            let fps = frame_count as f64 / elapsed;
            println!(
                "Frame {}: {}x{}, {} bytes RGB, {:.1} FPS avg",
                frame.frame_id,
                frame.width,
                frame.height,
                frame.data.len(),
                fps
            );
        }

        // Limit to 300 frames for testing
        if frame_count >= 300 {
            println!();
            println!("Reached 300 frames, stopping...");
            break;
        }
    }

    source.stop().await?;

    let elapsed = start.elapsed();
    println!();
    println!("Summary");
    println!("-------");
    println!("Total frames: {}", frame_count);
    println!("Total data: {:.2} MB", total_bytes as f64 / 1_000_000.0);
    println!("Duration: {:.2} s", elapsed.as_secs_f64());
    println!(
        "Average FPS: {:.1}",
        frame_count as f64 / elapsed.as_secs_f64()
    );

    Ok(())
}
