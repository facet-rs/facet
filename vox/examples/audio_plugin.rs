//! Audio processing plugin example for rapace.
//!
//! This example demonstrates a realistic audio plugin architecture that:
//! - Acts as a VST-style audio effect processor (gain + simple EQ)
//! - Implements bidirectional streaming for audio processing
//! - Exposes plugin services (process_buffer, set_parameter, get_parameter)
//! - Makes callbacks to host services (logging, preset storage)
//! - Demonstrates nested calls (reading parameters from host storage during processing)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        HOST PROCESS                          │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │ Host Services (provided by host, called by plugin)   │   │
//! │  │  - HostLogger::log()                                 │   │
//! │  │  - HostStorage::get_preset() / set_preset()          │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                           ▲                                  │
//! │                           │                                  │
//! │  ┌────────────────────────┼──────────────────────────────┐  │
//! │  │    Shared Memory       │                              │  │
//! │  │                        │                              │  │
//! │  │    A→B Ring ───────────┼───────────────────────────►  │  │
//! │  │    (host sends)        │     (plugin receives)        │  │
//! │  │                        │                              │  │
//! │  │    B→A Ring ◄──────────┼──────────────────────────────│  │
//! │  │    (plugin sends)      │     (host receives)          │  │
//! │  └────────────────────────┼──────────────────────────────┘  │
//! │                           │                                  │
//! │                           ▼                                  │
//! └───────────────────────────┼──────────────────────────────────┘
//!                             │
//! ┌───────────────────────────┼──────────────────────────────────┐
//! │                    PLUGIN PROCESS                            │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │ Plugin Services (provided by plugin, called by host) │   │
//! │  │  - AudioProcessor::process_buffer()                  │   │
//! │  │  - AudioProcessor::set_parameter()                   │   │
//! │  │  - AudioProcessor::get_parameter()                   │   │
//! │  │  - AudioProcessor::stream_audio()                    │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                                                              │
//! │  Plugin Implementation: GainEQPlugin                         │
//! │  - Processes audio samples                                   │
//! │  - Manages parameters (gain, low/high shelf)                 │
//! │  - Stores/loads presets via host storage                     │
//! │  - Logs processing statistics via host logger                │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Method IDs
//!
//! Plugin services (1000-1999):
//! - 1000: initialize - Initialize plugin, return metadata
//! - 1001: process_buffer - Process audio samples
//! - 1002: set_parameter - Set a parameter value
//! - 1003: get_parameter - Get a parameter value
//! - 1004: stream_audio - Bidirectional audio streaming
//! - 1005: shutdown - Clean shutdown
//!
//! Host services (2000-2999):
//! - 2000: log - Log a message from plugin
//! - 2001: get_preset - Retrieve stored preset
//! - 2002: set_preset - Store preset data
//!
//! ## Running this example
//!
//! This is a conceptual example showing plugin architecture patterns.
//! In a real implementation, you would:
//! - Run the host in one process
//! - Run the plugin in another process
//! - Pass the shared memory FD via Unix domain sockets
//! - Handle bidirectional streaming with proper backpressure

use std::ptr::NonNull;
use std::time::Duration;

use rapace::alloc::DataSegment;
use rapace::frame::FrameBuilder;
use rapace::layout::{DescRingHeader, MsgDescHot, SegmentHeader, SlotMeta, MAGIC};
use rapace::ring::Ring;
use rapace::session::{PeerB, Session, SessionConfig};
use rapace::shm::{calculate_segment_size, SharedMemory};
use rapace::types::{ByteLen, ChannelId, MethodId, MsgId};

use serde::{Deserialize, Serialize};

// ============================================================================
// Configuration
// ============================================================================

const RING_CAPACITY: u32 = 128;
const SLOT_COUNT: u32 = 32;
const SLOT_SIZE: u32 = 8192; // 8KB slots for audio buffers

// Plugin service method IDs
const METHOD_INITIALIZE: u32 = 1000;
const METHOD_PROCESS_BUFFER: u32 = 1001;
const METHOD_SET_PARAMETER: u32 = 1002;
const METHOD_GET_PARAMETER: u32 = 1003;
const METHOD_STREAM_AUDIO: u32 = 1004;
const METHOD_SHUTDOWN: u32 = 1005;

// Host service method IDs (callbacks)
const HOST_METHOD_LOG: u32 = 2000;
const HOST_METHOD_GET_PRESET: u32 = 2001;
const HOST_METHOD_SET_PRESET: u32 = 2002;

const PLUGIN_CHANNEL: u32 = 1;
const HOST_CALLBACK_CHANNEL: u32 = 2;

// ============================================================================
// Data Types - Plugin Services
// ============================================================================

/// Plugin metadata returned during initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginInfo {
    name: String,
    version: String,
    author: String,
    description: String,
    parameters: Vec<ParameterInfo>,
}

/// Information about a plugin parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParameterInfo {
    name: String,
    label: String,
    min_value: f32,
    max_value: f32,
    default_value: f32,
    unit: String,
}

/// Audio buffer for processing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AudioBuffer {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u8,
}

/// Request to process an audio buffer
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessBufferRequest {
    buffer: AudioBuffer,
}

/// Response from processing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessBufferResponse {
    buffer: AudioBuffer,
    samples_processed: u64,
    peak_level: f32,
}

/// Request to set a parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetParameterRequest {
    name: String,
    value: f32,
}

/// Response from setting parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetParameterResponse {
    success: bool,
    actual_value: f32,
}

/// Request to get a parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetParameterRequest {
    name: String,
}

/// Response with parameter value
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetParameterResponse {
    value: f32,
}

/// Audio stream chunk for bidirectional streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AudioStreamChunk {
    samples: Vec<f32>,
    timestamp: u64,
    is_final: bool,
}

// ============================================================================
// Data Types - Host Services (Callbacks)
// ============================================================================

/// Log message sent to host
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogMessage {
    level: LogLevel,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// Request to get a preset from host storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetPresetRequest {
    name: String,
}

/// Response with preset data
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetPresetResponse {
    found: bool,
    data: Vec<u8>,
}

/// Request to store a preset
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetPresetRequest {
    name: String,
    data: Vec<u8>,
}

/// Response from storing preset
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetPresetResponse {
    success: bool,
}

// ============================================================================
// Plugin Implementation: GainEQ Processor
// ============================================================================

/// A simple audio effect plugin that provides:
/// - Gain control
/// - Low shelf EQ
/// - High shelf EQ
struct GainEQPlugin {
    // Parameters
    gain: f32,
    low_shelf_gain: f32,
    high_shelf_gain: f32,

    // Statistics
    samples_processed: u64,
    peak_level: f32,

    // Preset management
    current_preset: String,
}

impl GainEQPlugin {
    fn new() -> Self {
        Self {
            gain: 1.0,
            low_shelf_gain: 1.0,
            high_shelf_gain: 1.0,
            samples_processed: 0,
            peak_level: 0.0,
            current_preset: "default".to_string(),
        }
    }

    fn get_info(&self) -> PluginInfo {
        PluginInfo {
            name: "GainEQ".to_string(),
            version: "1.0.0".to_string(),
            author: "Rapace Audio".to_string(),
            description: "Simple gain and EQ processor demonstrating plugin architecture".to_string(),
            parameters: vec![
                ParameterInfo {
                    name: "gain".to_string(),
                    label: "Gain".to_string(),
                    min_value: 0.0,
                    max_value: 2.0,
                    default_value: 1.0,
                    unit: "linear".to_string(),
                },
                ParameterInfo {
                    name: "low_shelf_gain".to_string(),
                    label: "Low Shelf".to_string(),
                    min_value: 0.0,
                    max_value: 2.0,
                    default_value: 1.0,
                    unit: "linear".to_string(),
                },
                ParameterInfo {
                    name: "high_shelf_gain".to_string(),
                    label: "High Shelf".to_string(),
                    min_value: 0.0,
                    max_value: 2.0,
                    default_value: 1.0,
                    unit: "linear".to_string(),
                },
            ],
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> f32 {
        let clamped = value.clamp(0.0, 2.0);
        match name {
            "gain" => self.gain = clamped,
            "low_shelf_gain" => self.low_shelf_gain = clamped,
            "high_shelf_gain" => self.high_shelf_gain = clamped,
            _ => return f32::NAN,
        }
        clamped
    }

    fn get_parameter(&self, name: &str) -> f32 {
        match name {
            "gain" => self.gain,
            "low_shelf_gain" => self.low_shelf_gain,
            "high_shelf_gain" => self.high_shelf_gain,
            _ => f32::NAN,
        }
    }

    fn process_buffer(&mut self, mut buffer: AudioBuffer) -> AudioBuffer {
        // Simple audio processing: apply gain and basic EQ simulation
        let mut peak = 0.0f32;

        for sample in buffer.samples.iter_mut() {
            // Apply gain
            *sample *= self.gain;

            // Simplified EQ (not real filtering, just for demonstration)
            // In a real plugin, you'd use proper biquad filters
            let low_component = sample.clamp(-0.3, 0.3) * self.low_shelf_gain;
            let high_component = (*sample - low_component) * self.high_shelf_gain;
            *sample = low_component + high_component;

            // Track peak
            peak = peak.max(sample.abs());
        }

        self.samples_processed += buffer.samples.len() as u64;
        self.peak_level = peak;

        buffer
    }

    fn get_stats(&self) -> (u64, f32) {
        (self.samples_processed, self.peak_level)
    }

    fn save_preset(&self) -> Vec<u8> {
        // Serialize current state as preset
        let preset = serde_json::json!({
            "gain": self.gain,
            "low_shelf_gain": self.low_shelf_gain,
            "high_shelf_gain": self.high_shelf_gain,
        });
        preset.to_string().into_bytes()
    }

    fn load_preset(&mut self, data: &[u8]) -> bool {
        if let Ok(text) = std::str::from_utf8(data) {
            if let Ok(preset) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(gain) = preset.get("gain").and_then(|v| v.as_f64()) {
                    self.gain = gain as f32;
                }
                if let Some(low) = preset.get("low_shelf_gain").and_then(|v| v.as_f64()) {
                    self.low_shelf_gain = low as f32;
                }
                if let Some(high) = preset.get("high_shelf_gain").and_then(|v| v.as_f64()) {
                    self.high_shelf_gain = high as f32;
                }
                return true;
            }
        }
        false
    }
}

// ============================================================================
// Plugin Service Handler
// ============================================================================

fn main() {
    println!("=== Rapace Audio Plugin Example ===");
    println!("This example demonstrates a realistic audio plugin architecture.\n");

    // In a real implementation, we would receive the FD from the host
    // For this demo, we create our own segment
    let segment_size = calculate_segment_size(RING_CAPACITY, SLOT_COUNT, SLOT_SIZE);
    let mut shm = SharedMemory::create("audio-plugin-demo", segment_size)
        .expect("Failed to create shared memory");

    unsafe {
        initialize_segment(&mut shm);
    }

    let mut session = unsafe { create_plugin_session(&shm) };
    let mut plugin = GainEQPlugin::new();
    let mut msg_id_counter = 1u64;

    println!("Plugin initialized: {}", plugin.get_info().name);
    println!("Waiting for host requests...\n");

    // Simulate plugin lifecycle
    demonstrate_plugin_lifecycle(&mut session, &mut plugin, &mut msg_id_counter);

    println!("\n=== Plugin shutdown complete ===");
}

/// Demonstrates a complete plugin lifecycle with various operations
fn demonstrate_plugin_lifecycle(
    session: &mut Session<PeerB>,
    plugin: &mut GainEQPlugin,
    msg_id_counter: &mut u64,
) {
    // 1. INITIALIZE - Host requests plugin info
    println!(">>> Host: Requesting plugin initialization");
    let info = plugin.get_info();
    println!("<<< Plugin: Returning metadata");
    println!("    Name: {}", info.name);
    println!("    Version: {}", info.version);
    println!("    Parameters: {}", info.parameters.len());
    for param in &info.parameters {
        println!("      - {} ({}-{})", param.label, param.min_value, param.max_value);
    }

    // 2. LOAD PRESET - Plugin calls host to retrieve saved preset
    println!("\n>>> Plugin: Requesting preset 'my-favorite-sound' from host storage");
    send_host_callback(
        session,
        HOST_METHOD_GET_PRESET,
        &GetPresetRequest {
            name: "my-favorite-sound".to_string(),
        },
        msg_id_counter,
    );
    println!("<<< Host: Preset found, returning data");
    let preset_data = plugin.save_preset(); // Simulate loaded data
    if plugin.load_preset(&preset_data) {
        println!("    Plugin: Preset loaded successfully");
        log_to_host(
            session,
            LogLevel::Info,
            "Loaded preset: my-favorite-sound",
            msg_id_counter,
        );
    }

    // 3. SET PARAMETERS - Host adjusts plugin parameters
    println!("\n>>> Host: Setting gain to 1.5");
    let actual = plugin.set_parameter("gain", 1.5);
    println!("<<< Plugin: Parameter set, actual value: {}", actual);
    log_to_host(
        session,
        LogLevel::Debug,
        &format!("Gain changed to {}", actual),
        msg_id_counter,
    );

    println!("\n>>> Host: Setting low_shelf_gain to 1.2");
    let actual = plugin.set_parameter("low_shelf_gain", 1.2);
    println!("<<< Plugin: Parameter set, actual value: {}", actual);

    // 4. PROCESS AUDIO - Host sends audio buffer for processing
    println!("\n>>> Host: Sending audio buffer for processing");
    let input_buffer = AudioBuffer {
        samples: generate_test_audio(512),
        sample_rate: 48000,
        channels: 2,
    };
    println!("    Input: {} samples, {}Hz, {} channels",
             input_buffer.samples.len(),
             input_buffer.sample_rate,
             input_buffer.channels);

    let output_buffer = plugin.process_buffer(input_buffer.clone());
    let (total_samples, peak) = plugin.get_stats();

    println!("<<< Plugin: Buffer processed");
    println!("    Output: {} samples, peak level: {:.3}",
             output_buffer.samples.len(), peak);
    println!("    Total samples processed: {}", total_samples);

    // Log processing stats to host
    log_to_host(
        session,
        LogLevel::Debug,
        &format!("Processed {} samples, peak: {:.3}", output_buffer.samples.len(), peak),
        msg_id_counter,
    );

    // 5. NESTED CALL - During processing, plugin reads parameter from host
    println!("\n>>> Plugin: Reading parameter from host during processing");
    send_host_callback(
        session,
        HOST_METHOD_GET_PRESET,
        &GetPresetRequest {
            name: "current-state".to_string(),
        },
        msg_id_counter,
    );
    println!("<<< Host: Returning current state");

    // 6. STREAM PROCESSING - Demonstrate bidirectional streaming
    println!("\n>>> Host: Starting audio stream");
    stream_audio(session, plugin, msg_id_counter);

    // 7. SAVE PRESET - Plugin saves current state
    println!("\n>>> Plugin: Saving current preset to host");
    let preset_data = plugin.save_preset();
    send_host_callback(
        session,
        HOST_METHOD_SET_PRESET,
        &SetPresetRequest {
            name: "my-custom-preset".to_string(),
            data: preset_data,
        },
        msg_id_counter,
    );
    println!("<<< Host: Preset saved successfully");
    log_to_host(
        session,
        LogLevel::Info,
        "Saved preset: my-custom-preset",
        msg_id_counter,
    );

    // 8. FINAL STATS
    println!("\n=== Final Statistics ===");
    let (total_samples, peak) = plugin.get_stats();
    println!("Total samples processed: {}", total_samples);
    println!("Peak level: {:.3}", peak);
    println!("Current parameters:");
    println!("  gain: {}", plugin.get_parameter("gain"));
    println!("  low_shelf_gain: {}", plugin.get_parameter("low_shelf_gain"));
    println!("  high_shelf_gain: {}", plugin.get_parameter("high_shelf_gain"));
}

/// Simulate bidirectional audio streaming
fn stream_audio(
    session: &mut Session<PeerB>,
    plugin: &mut GainEQPlugin,
    msg_id_counter: &mut u64,
) {
    const STREAM_CHUNKS: usize = 5;
    const CHUNK_SIZE: usize = 128;

    println!("    Starting {} audio chunks...", STREAM_CHUNKS);

    for i in 0..STREAM_CHUNKS {
        let is_final = i == STREAM_CHUNKS - 1;

        // Host sends chunk to plugin
        let input_chunk = AudioStreamChunk {
            samples: generate_test_audio(CHUNK_SIZE),
            timestamp: i as u64 * CHUNK_SIZE as u64,
            is_final,
        };

        println!("    >>> Host: Chunk {}/{} ({} samples)",
                 i + 1, STREAM_CHUNKS, input_chunk.samples.len());

        // Plugin processes chunk
        let mut buffer = AudioBuffer {
            samples: input_chunk.samples.clone(),
            sample_rate: 48000,
            channels: 2,
        };
        buffer = plugin.process_buffer(buffer);

        let _output_chunk = AudioStreamChunk {
            samples: buffer.samples,
            timestamp: input_chunk.timestamp,
            is_final,
        };

        println!("    <<< Plugin: Chunk {}/{} processed, peak: {:.3}",
                 i + 1, STREAM_CHUNKS, plugin.peak_level);

        if is_final {
            println!("    Stream complete");
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    log_to_host(
        session,
        LogLevel::Info,
        &format!("Completed streaming {} chunks", STREAM_CHUNKS),
        msg_id_counter,
    );
}

/// Send a log message to the host
fn log_to_host(
    session: &mut Session<PeerB>,
    level: LogLevel,
    message: &str,
    msg_id_counter: &mut u64,
) {
    let log_msg = LogMessage {
        level,
        message: message.to_string(),
    };
    send_host_callback(session, HOST_METHOD_LOG, &log_msg, msg_id_counter);
}

/// Send a callback to the host service
fn send_host_callback<T: Serialize>(
    session: &mut Session<PeerB>,
    method_id: u32,
    payload: &T,
    msg_id_counter: &mut u64,
) {
    let channel = ChannelId::new(HOST_CALLBACK_CHANNEL).expect("Invalid channel ID");
    let msg_id = MsgId::new(*msg_id_counter);
    *msg_id_counter += 1;

    // Serialize payload
    let data = serde_json::to_vec(payload).expect("Failed to serialize");

    // Send via appropriate mechanism (inline or slot-based)
    if data.len() <= 24 {
        let frame = FrameBuilder::data(channel, MethodId::new(method_id), msg_id)
            .inline_payload(&data)
            .expect("Payload too large for inline")
            .build();

        let _ = session.outbound_producer().try_enqueue(frame);
    } else {
        // Allocate slot and build frame separately to avoid borrow issues
        let frame_opt = if let Ok(mut slot) = session.outbound_segment().alloc() {
            let buf = slot.as_mut_bytes();
            buf[..data.len()].copy_from_slice(&data);

            let byte_len = ByteLen::new(data.len() as u32, SLOT_SIZE)
                .expect("Payload too large");
            let committed = slot.commit(byte_len);

            Some(FrameBuilder::data(channel, MethodId::new(method_id), msg_id)
                .slot_payload(committed)
                .build())
        } else {
            None
        };

        if let Some(frame) = frame_opt {
            let _ = session.outbound_producer().try_enqueue(frame);
        }
    }
}

/// Generate test audio samples (sine wave)
fn generate_test_audio(sample_count: usize) -> Vec<f32> {
    let frequency = 440.0; // A4
    let sample_rate = 48000.0;
    (0..sample_count)
        .map(|i| {
            let t = i as f32 / sample_rate;
            (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5
        })
        .collect()
}

// ============================================================================
// Shared Memory Initialization (same as echo examples)
// ============================================================================

unsafe fn initialize_segment(shm: &mut SharedMemory) {
    let base = shm.as_ptr();
    let mut offset = 0usize;
    let align = |off: usize| (off + 63) & !63;

    // Segment header
    let header = base.add(offset) as *mut SegmentHeader;
    offset += std::mem::size_of::<SegmentHeader>();
    offset = align(offset);

    std::ptr::write(header, SegmentHeader {
        magic: MAGIC,
        version: 1,
        flags: 0,
        peer_a_epoch: std::sync::atomic::AtomicU64::new(0),
        peer_b_epoch: std::sync::atomic::AtomicU64::new(0),
        peer_a_last_seen: std::sync::atomic::AtomicU64::new(0),
        peer_b_last_seen: std::sync::atomic::AtomicU64::new(0),
    });

    // A→B ring
    let a_to_b_ring = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    std::ptr::write(a_to_b_ring, DescRingHeader {
        visible_head: std::sync::atomic::AtomicU64::new(0),
        _pad1: [0; 56],
        tail: std::sync::atomic::AtomicU64::new(0),
        _pad2: [0; 56],
        capacity: RING_CAPACITY,
        _pad3: [0; 60],
    });

    // B→A ring
    let b_to_a_ring = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    std::ptr::write(b_to_a_ring, DescRingHeader {
        visible_head: std::sync::atomic::AtomicU64::new(0),
        _pad1: [0; 56],
        tail: std::sync::atomic::AtomicU64::new(0),
        _pad2: [0; 56],
        capacity: RING_CAPACITY,
        _pad3: [0; 60],
    });

    // Slot metadata
    let slot_meta_base = base.add(offset) as *mut SlotMeta;
    offset += std::mem::size_of::<SlotMeta>() * SLOT_COUNT as usize;
    offset = align(offset);

    for i in 0..SLOT_COUNT {
        std::ptr::write(slot_meta_base.add(i as usize), SlotMeta {
            generation: std::sync::atomic::AtomicU32::new(0),
            state: std::sync::atomic::AtomicU32::new(0),
        });
    }
}

unsafe fn create_plugin_session(shm: &SharedMemory) -> Session<PeerB> {
    let base = shm.as_ptr();
    let mut offset = 0usize;
    let align = |off: usize| (off + 63) & !63;

    let header = base as *const SegmentHeader;
    offset += std::mem::size_of::<SegmentHeader>();
    offset = align(offset);

    let a_to_b_ring_header = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    let b_to_a_ring_header = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    let slot_meta_base = base.add(offset) as *mut SlotMeta;
    offset += std::mem::size_of::<SlotMeta>() * SLOT_COUNT as usize;
    offset = align(offset);

    let slot_data_base = base.add(offset);

    let a_to_b_ring = Box::leak(Box::new(Ring::from_raw(
        NonNull::new(a_to_b_ring_header).unwrap(),
        RING_CAPACITY,
    )));
    let b_to_a_ring = Box::leak(Box::new(Ring::from_raw(
        NonNull::new(b_to_a_ring_header).unwrap(),
        RING_CAPACITY,
    )));

    let (_inbound_producer, inbound_consumer) = a_to_b_ring.split();
    let (outbound_producer, _outbound_consumer) = b_to_a_ring.split();

    let outbound_segment = DataSegment::from_raw(
        NonNull::new(slot_meta_base).unwrap(),
        NonNull::new(slot_data_base).unwrap(),
        SLOT_SIZE,
        SLOT_COUNT,
    );
    outbound_segment.init_free_list();

    let inbound_segment = DataSegment::from_raw(
        NonNull::new(slot_meta_base).unwrap(),
        NonNull::new(slot_data_base).unwrap(),
        SLOT_SIZE,
        SLOT_COUNT,
    );

    let session = Session::new(
        header,
        outbound_producer,
        inbound_consumer,
        outbound_segment,
        inbound_segment,
        SessionConfig::default(),
    );

    session.heartbeat();
    session
}
