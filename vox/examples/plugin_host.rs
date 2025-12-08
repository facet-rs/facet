//! Plugin host example for rapace.
//!
//! This example demonstrates hosting multiple plugins using rapace's shared memory IPC.
//! The host:
//! - Creates separate shared memory segments for each plugin
//! - Exposes host services (HostLogger, HostStorage, HostEvents)
//! - Calls into plugins (initialize, process, shutdown)
//! - Handles plugin lifecycle (connected, running, crashed)
//! - Demonstrates nested calls (plugins calling back into host during processing)
//!
//! Architecture:
//! - Each plugin has its own dedicated SHM segment and session
//! - Host services are exposed on control channel (channel 0)
//! - Plugin interface methods use user channels (channel 1+)
//! - Method dispatch uses the registry/dispatch system

use std::collections::HashMap;
use std::ptr::NonNull;
use std::time::Duration;

use rapace::alloc::DataSegment;
use rapace::dispatch::MethodDispatcher;
use rapace::frame::{FrameBuilder, RawDescriptor, DescriptorLimits, ControlMethod};
use rapace::layout::{DescRingHeader, MsgDescHot, SegmentHeader, SlotMeta, MAGIC};
use rapace::registry::{ServiceRegistryBuilder, MethodKind};
use rapace::ring::Ring;
use rapace::session::{PeerA, Session, SessionConfig};
use rapace::shm::{calculate_segment_size, SharedMemory};
use rapace::types::{ChannelId, MethodId, MsgId};

use serde::{Deserialize, Serialize};

// === Configuration ===

const RING_CAPACITY: u32 = 64;
const SLOT_COUNT: u32 = 16;
const SLOT_SIZE: u32 = 4096;

// === Host Service Method IDs ===

mod host_methods {
    /// HostLogger::log(level, message)
    pub const HOST_LOGGER_LOG: u32 = 1000;

    /// HostStorage::get(key) -> Option<value>
    pub const HOST_STORAGE_GET: u32 = 1001;

    /// HostStorage::set(key, value)
    pub const HOST_STORAGE_SET: u32 = 1002;

    /// HostEvents::subscribe() -> stream of events
    pub const HOST_EVENTS_SUBSCRIBE: u32 = 1003;
}

// === Plugin Interface Method IDs ===

mod plugin_methods {
    /// Plugin::initialize(config)
    pub const PLUGIN_INITIALIZE: u32 = 2000;

    /// Plugin::process(request) -> response
    pub const PLUGIN_PROCESS: u32 = 2001;

    /// Plugin::shutdown()
    pub const PLUGIN_SHUTDOWN: u32 = 2002;
}

// === Host Services ===

/// Host service: Logger
/// Allows plugins to log messages through the host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRequest {
    pub level: LogLevel,
    pub message: String,
}

/// Host service: Storage
/// Key-value storage for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRequest {
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResponse {
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRequest {
    pub key: String,
    pub value: Vec<u8>,
}

/// Host service: Events
/// Bidirectional stream for host events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostEvent {
    PluginLoaded { plugin_id: String },
    PluginUnloaded { plugin_id: String },
    ConfigUpdated { key: String, value: String },
    Shutdown,
}

// === Plugin Interface ===

/// Plugin configuration passed during initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub plugin_id: String,
    pub config_data: HashMap<String, String>,
}

/// Request sent to Plugin::process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub request_id: u64,
    pub operation: String,
    pub data: Vec<u8>,
}

/// Response from Plugin::process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResponse {
    pub request_id: u64,
    pub success: bool,
    pub result: Vec<u8>,
}

// === Plugin Session ===

/// Represents a single plugin session
struct PluginSession {
    plugin_id: String,
    shm: SharedMemory,
    session: Session<PeerA>,
    state: PluginState,
    next_msg_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginState {
    Connected,
    Initializing,
    Running,
    ShuttingDown,
    Crashed,
}

impl PluginSession {
    /// Create a new plugin session
    fn new(plugin_id: String) -> Result<Self, Box<dyn std::error::Error>> {
        println!("[Host] Creating session for plugin: {}", plugin_id);

        // Calculate segment size
        let segment_size = calculate_segment_size(RING_CAPACITY, SLOT_COUNT, SLOT_SIZE);

        // Create shared memory segment
        let shm_name = format!("plugin-{}", plugin_id);
        let mut shm = SharedMemory::create(&shm_name, segment_size)?;

        // Initialize segment layout
        unsafe {
            initialize_segment(&mut shm);
        }

        // Create session as PeerA (host)
        let session = unsafe {
            create_peer_a_session(&shm)
        };

        Ok(PluginSession {
            plugin_id,
            shm,
            session,
            state: PluginState::Connected,
            next_msg_id: 1,
        })
    }

    /// Send initialization request to plugin
    fn initialize(&mut self, config: PluginConfig) -> Result<(), Box<dyn std::error::Error>> {
        println!("[Host] Initializing plugin: {}", self.plugin_id);
        self.state = PluginState::Initializing;

        // Serialize config
        let payload = postcard::to_allocvec(&config)?;

        // Send initialize request on control channel
        self.send_control_message(
            MethodId::new(plugin_methods::PLUGIN_INITIALIZE),
            &payload,
        )?;

        Ok(())
    }

    /// Send process request to plugin
    fn process(&mut self, request: ProcessRequest) -> Result<(), Box<dyn std::error::Error>> {
        println!("[Host] Sending process request to plugin: {}", self.plugin_id);

        // Serialize request
        let payload = postcard::to_allocvec(&request)?;

        // Send on channel 1 (user data channel)
        self.send_data_message(
            ChannelId::new(1).unwrap(),
            MethodId::new(plugin_methods::PLUGIN_PROCESS),
            &payload,
        )?;

        Ok(())
    }

    /// Send shutdown request to plugin
    fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("[Host] Shutting down plugin: {}", self.plugin_id);
        self.state = PluginState::ShuttingDown;

        // Send empty shutdown request
        self.send_control_message(
            MethodId::new(plugin_methods::PLUGIN_SHUTDOWN),
            &[],
        )?;

        Ok(())
    }

    /// Send a control channel message
    fn send_control_message(&mut self, method: MethodId, payload: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let msg_id = self.next_msg_id();

        // Build frame
        let frame = if payload.len() <= 24 {
            FrameBuilder::control(ControlMethod::OpenChannel, msg_id)
                .inline_payload(payload)
                .map_err(|_| "Payload too large for inline")?
                .build()
        } else {
            // Allocate slot for larger payloads
            let mut slot = self.session.outbound_segment().alloc()
                .map_err(|e| format!("Allocation error: {:?}", e))?;

            let buf = slot.as_mut_bytes();
            buf[..payload.len()].copy_from_slice(payload);

            let byte_len = rapace::types::ByteLen::new(payload.len() as u32, SLOT_SIZE)
                .ok_or("Payload too large for slot")?;
            let committed = slot.commit(byte_len);

            FrameBuilder::control(ControlMethod::OpenChannel, msg_id)
                .slot_payload(committed)
                .build()
        };

        // Send frame
        self.session.outbound_producer().try_enqueue(frame)
            .map_err(|_| "Ring full")?;

        Ok(())
    }

    /// Send a data channel message
    fn send_data_message(&mut self, channel: ChannelId, method: MethodId, payload: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let msg_id = self.next_msg_id();

        // Build frame
        let frame = if payload.len() <= 24 {
            FrameBuilder::data(channel, method, msg_id)
                .inline_payload(payload)
                .map_err(|_| "Payload too large for inline")?
                .build()
        } else {
            // Allocate slot for larger payloads
            let mut slot = self.session.outbound_segment().alloc()
                .map_err(|e| format!("Allocation error: {:?}", e))?;

            let buf = slot.as_mut_bytes();
            buf[..payload.len()].copy_from_slice(payload);

            let byte_len = rapace::types::ByteLen::new(payload.len() as u32, SLOT_SIZE)
                .ok_or("Payload too large for slot")?;
            let committed = slot.commit(byte_len);

            FrameBuilder::data(channel, method, msg_id)
                .slot_payload(committed)
                .build()
        };

        // Send frame
        self.session.outbound_producer().try_enqueue(frame)
            .map_err(|_| "Ring full")?;

        Ok(())
    }

    /// Process incoming messages from plugin
    fn poll(&mut self, storage: &mut HashMap<String, Vec<u8>>) -> Result<(), Box<dyn std::error::Error>> {
        // Update heartbeat
        self.session.heartbeat();

        // Check if plugin is alive
        if !self.session.is_peer_alive() {
            if self.state != PluginState::Crashed {
                println!("[Host] Plugin {} appears to have crashed", self.plugin_id);
                self.state = PluginState::Crashed;
            }
            return Ok(());
        }

        // Process incoming messages
        loop {
            let desc = match self.session.inbound_consumer().try_dequeue() {
                Some(d) => d,
                None => break,
            };

            // Validate descriptor
            let raw_desc = RawDescriptor::new(desc);
            let limits = DescriptorLimits::default();

            let valid_desc = match raw_desc.validate(self.session.inbound_segment(), &limits) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("[Host] Invalid descriptor from plugin {}: {:?}", self.plugin_id, e);
                    continue;
                }
            };

            // Handle the message
            self.handle_message(valid_desc, storage)?;
        }

        Ok(())
    }

    /// Handle an incoming message from plugin
    fn handle_message(
        &mut self,
        desc: rapace::frame::ValidDescriptor,
        storage: &mut HashMap<String, Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let method_id = desc.method_id().get();
        let channel_id = desc.channel_id();

        // Read payload
        let payload_data = if desc.is_inline() {
            desc.inline_payload().to_vec()
        } else {
            let (slot_idx, _gen, offset, len) = desc.slot_info().unwrap();
            self.session.inbound_segment()
                .slot_data(slot_idx, offset, len)
                .to_vec()
        };

        // Free the inbound payload immediately (drop it before we need to borrow self mutably)
        {
            let _inbound = desc.into_inbound_payload(self.session.inbound_segment());
            // _inbound drops here, freeing the slot
        }

        // Dispatch based on method ID
        match method_id {
            // Host service: Logger
            host_methods::HOST_LOGGER_LOG => {
                let req: LogRequest = postcard::from_bytes(&payload_data)?;
                println!("[Host/Logger] [{}] [{:?}] {}",
                    self.plugin_id, req.level, req.message);
            }

            // Host service: Storage::get
            host_methods::HOST_STORAGE_GET => {
                let req: GetRequest = postcard::from_bytes(&payload_data)?;
                let value = storage.get(&req.key).cloned();

                println!("[Host/Storage] Plugin {} GET {}: {:?}",
                    self.plugin_id, req.key, value.as_ref().map(|v| v.len()));

                // Send response
                let response = GetResponse { value };
                let response_payload = postcard::to_allocvec(&response)?;

                if let Some(ch) = channel_id {
                    self.send_data_message(
                        ch,
                        MethodId::new(host_methods::HOST_STORAGE_GET),
                        &response_payload,
                    )?;
                }
            }

            // Host service: Storage::set
            host_methods::HOST_STORAGE_SET => {
                let req: SetRequest = postcard::from_bytes(&payload_data)?;
                println!("[Host/Storage] Plugin {} SET {}: {} bytes",
                    self.plugin_id, req.key, req.value.len());

                storage.insert(req.key, req.value);
            }

            // Plugin response: process
            plugin_methods::PLUGIN_PROCESS => {
                let response: ProcessResponse = postcard::from_bytes(&payload_data)?;
                println!("[Host] Plugin {} process response: req_id={}, success={}, {} bytes",
                    self.plugin_id, response.request_id, response.success, response.result.len());

                if self.state == PluginState::Initializing {
                    self.state = PluginState::Running;
                    println!("[Host] Plugin {} is now running", self.plugin_id);
                }
            }

            _ => {
                println!("[Host] Unknown method {} from plugin {}", method_id, self.plugin_id);
            }
        }

        Ok(())
    }

    fn next_msg_id(&mut self) -> MsgId {
        let id = self.next_msg_id;
        self.next_msg_id += 1;
        MsgId::new(id)
    }
}

// === Plugin Host ===

/// Main plugin host
struct PluginHost {
    plugins: HashMap<String, PluginSession>,
    storage: HashMap<String, Vec<u8>>,
    dispatcher: MethodDispatcher,
}

impl PluginHost {
    fn new() -> Self {
        let mut dispatcher = MethodDispatcher::new();

        // Register host service handlers
        // (In a real implementation, these would be proper Handler implementations)

        PluginHost {
            plugins: HashMap::new(),
            storage: HashMap::new(),
            dispatcher,
        }
    }

    /// Load a plugin
    fn load_plugin(&mut self, plugin_id: String, config: HashMap<String, String>) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n=== Loading plugin: {} ===", plugin_id);

        // Create plugin session
        let mut session = PluginSession::new(plugin_id.clone())?;

        // Initialize plugin
        let plugin_config = PluginConfig {
            plugin_id: plugin_id.clone(),
            config_data: config,
        };
        session.initialize(plugin_config)?;

        self.plugins.insert(plugin_id, session);
        Ok(())
    }

    /// Unload a plugin
    fn unload_plugin(&mut self, plugin_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n=== Unloading plugin: {} ===", plugin_id);

        if let Some(mut session) = self.plugins.remove(plugin_id) {
            session.shutdown()?;
        }

        Ok(())
    }

    /// Poll all plugins for messages
    fn poll_plugins(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (_id, session) in self.plugins.iter_mut() {
            session.poll(&mut self.storage)?;
        }
        Ok(())
    }

    /// Send process request to a plugin
    fn send_to_plugin(&mut self, plugin_id: &str, request: ProcessRequest) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(session) = self.plugins.get_mut(plugin_id) {
            session.process(request)?;
        } else {
            eprintln!("[Host] Plugin not found: {}", plugin_id);
        }
        Ok(())
    }

    /// Get plugin state
    fn plugin_state(&self, plugin_id: &str) -> Option<PluginState> {
        self.plugins.get(plugin_id).map(|s| s.state)
    }

    /// Build service registry for introspection
    fn build_registry(&self) -> Vec<u8> {
        let mut builder = ServiceRegistryBuilder::new();

        // Host services
        let host_service = builder.add_service("rapace.HostServices", 1, 0).unwrap();
        host_service.add_method("Logger.log", host_methods::HOST_LOGGER_LOG, MethodKind::Unary).unwrap();
        host_service.add_method("Storage.get", host_methods::HOST_STORAGE_GET, MethodKind::Unary).unwrap();
        host_service.add_method("Storage.set", host_methods::HOST_STORAGE_SET, MethodKind::Unary).unwrap();
        host_service.add_method("Events.subscribe", host_methods::HOST_EVENTS_SUBSCRIBE, MethodKind::ServerStreaming).unwrap();

        // Plugin interface
        let plugin_service = builder.add_service("rapace.Plugin", 1, 0).unwrap();
        plugin_service.add_method("initialize", plugin_methods::PLUGIN_INITIALIZE, MethodKind::Unary).unwrap();
        plugin_service.add_method("process", plugin_methods::PLUGIN_PROCESS, MethodKind::Unary).unwrap();
        plugin_service.add_method("shutdown", plugin_methods::PLUGIN_SHUTDOWN, MethodKind::Unary).unwrap();

        builder.build()
    }
}

// === Main ===

fn main() {
    println!("=== Rapace Plugin Host Example ===\n");

    // Create host
    let mut host = PluginHost::new();

    // Build and display service registry
    let registry_data = host.build_registry();
    println!("Service registry: {} bytes\n", registry_data.len());

    // Load plugins
    let mut config1 = HashMap::new();
    config1.insert("log_level".to_string(), "debug".to_string());
    config1.insert("cache_size".to_string(), "1024".to_string());

    let mut config2 = HashMap::new();
    config2.insert("log_level".to_string(), "info".to_string());
    config2.insert("worker_threads".to_string(), "4".to_string());

    // Load plugin 1
    if let Err(e) = host.load_plugin("analytics".to_string(), config1) {
        eprintln!("Failed to load analytics plugin: {}", e);
        return;
    }

    // Load plugin 2
    if let Err(e) = host.load_plugin("transform".to_string(), config2) {
        eprintln!("Failed to load transform plugin: {}", e);
        return;
    }

    // Give plugins time to initialize
    std::thread::sleep(Duration::from_millis(100));

    // Poll for initialization responses
    for _ in 0..10 {
        if let Err(e) = host.poll_plugins() {
            eprintln!("Error polling plugins: {}", e);
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Demonstrate nested calls: send process request that triggers host service calls
    println!("\n=== Sending work to plugins ===\n");

    // Send to analytics plugin
    let request1 = ProcessRequest {
        request_id: 1,
        operation: "analyze".to_string(),
        data: b"data sample 1".to_vec(),
    };

    if let Err(e) = host.send_to_plugin("analytics", request1) {
        eprintln!("Failed to send to analytics: {}", e);
    }

    // Send to transform plugin
    let request2 = ProcessRequest {
        request_id: 2,
        operation: "transform".to_string(),
        data: b"input data for transformation".to_vec(),
    };

    if let Err(e) = host.send_to_plugin("transform", request2) {
        eprintln!("Failed to send to transform: {}", e);
    }

    // Event loop: poll plugins for responses and nested calls
    println!("\n=== Event loop (processing plugin messages) ===\n");

    for iteration in 0..20 {
        if let Err(e) = host.poll_plugins() {
            eprintln!("Error polling plugins: {}", e);
        }

        // Check plugin states
        if iteration % 5 == 0 {
            println!("[Host] Plugin states:");
            for (id, session) in &host.plugins {
                println!("  - {}: {:?}", id, session.state);
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    // Unload plugins
    println!("\n=== Shutting down ===\n");

    if let Err(e) = host.unload_plugin("analytics") {
        eprintln!("Failed to unload analytics: {}", e);
    }

    if let Err(e) = host.unload_plugin("transform") {
        eprintln!("Failed to unload transform: {}", e);
    }

    // Final poll for shutdown acknowledgments
    for _ in 0..5 {
        if let Err(e) = host.poll_plugins() {
            eprintln!("Error polling plugins: {}", e);
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    println!("\n=== Plugin host terminated ===");
}

// === Segment Initialization Helpers ===

/// Initialize the shared memory segment layout
unsafe fn initialize_segment(shm: &mut SharedMemory) {
    let base = shm.as_ptr();
    let mut offset = 0usize;

    let align = |off: usize| (off + 63) & !63;

    // 1. Segment header
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

    // 2. A→B ring (host sends to plugin)
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

    // 3. B→A ring (plugin sends to host)
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

    // 4. Slot metadata
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

/// Create a PeerA session (host side)
unsafe fn create_peer_a_session(shm: &SharedMemory) -> Session<PeerA> {
    let base = shm.as_ptr();
    let mut offset = 0usize;

    let align = |off: usize| (off + 63) & !63;

    // Get pointers to all components
    let header = base as *const SegmentHeader;
    offset += std::mem::size_of::<SegmentHeader>();
    offset = align(offset);

    // A→B ring (our outbound)
    let a_to_b_ring_header = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    // B→A ring (our inbound)
    let b_to_a_ring_header = base.add(offset) as *mut DescRingHeader;
    offset += std::mem::size_of::<DescRingHeader>();
    offset += std::mem::size_of::<MsgDescHot>() * RING_CAPACITY as usize;
    offset = align(offset);

    // Slot metadata
    let slot_meta_base = base.add(offset) as *mut SlotMeta;
    offset += std::mem::size_of::<SlotMeta>() * SLOT_COUNT as usize;
    offset = align(offset);

    // Slot data
    let slot_data_base = base.add(offset);

    // Create rings
    let a_to_b_ring = Box::leak(Box::new(Ring::from_raw(
        NonNull::new(a_to_b_ring_header).unwrap(),
        RING_CAPACITY,
    )));
    let b_to_a_ring = Box::leak(Box::new(Ring::from_raw(
        NonNull::new(b_to_a_ring_header).unwrap(),
        RING_CAPACITY,
    )));

    // Split rings
    let (outbound_producer, _outbound_consumer) = a_to_b_ring.split();
    let (_inbound_producer, inbound_consumer) = b_to_a_ring.split();

    // Create data segments
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

    // Create session
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
