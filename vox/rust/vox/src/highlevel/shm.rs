use std::sync::Arc;

use super::VoxListener;

/// A [`VoxListener`] that accepts SHM bootstrap connections on a Unix control socket.
pub struct ShmListener {
    control_listener: tokio::net::UnixListener,
    hub: Arc<vox_shm::host::HostHub>,
    segment: Arc<vox_shm::Segment>,
    _lock: vox_stream::LocalListenerLock,
}

impl ShmListener {
    /// Create an SHM listener with default segment configuration.
    pub fn bind(control_socket_path: impl Into<String>) -> std::io::Result<Self> {
        Self::bind_with_config(control_socket_path, ShmListenerConfig::default())
    }

    /// Create an SHM listener with custom segment configuration.
    pub fn bind_with_config(
        control_socket_path: impl Into<String>,
        config: ShmListenerConfig,
    ) -> std::io::Result<Self> {
        let control_socket_path = control_socket_path.into();

        let lock = match vox_stream::try_local_lock(&control_socket_path)? {
            vox_stream::LocalLockOutcome::Acquired(lock) => lock,
            vox_stream::LocalLockOutcome::Held => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!("another process is serving SHM on {control_socket_path}"),
                ));
            }
        };

        let _ = std::fs::remove_file(&control_socket_path);

        let segment_path = std::path::PathBuf::from(format!("{control_socket_path}.segment"));
        let segment = Arc::new(vox_shm::Segment::create(
            &segment_path,
            vox_shm::SegmentConfig {
                max_guests: config.max_guests,
                bipbuf_capacity: config.bipbuf_capacity,
                max_payload_size: config.max_payload_size,
                inline_threshold: 256,
                heartbeat_interval: 0,
                size_classes: &config.size_classes(),
            },
            vox_shm::FileCleanup::Auto,
        )?);

        let hub = Arc::new(vox_shm::host::HostHub::new(Arc::clone(&segment)));
        let control_listener = tokio::net::UnixListener::bind(&control_socket_path)?;

        Ok(Self {
            control_listener,
            hub,
            segment,
            _lock: lock,
        })
    }
}

/// Configuration for [`ShmListener`].
pub struct ShmListenerConfig {
    pub max_guests: u8,
    pub bipbuf_capacity: u32,
    pub max_payload_size: u32,
}

impl Default for ShmListenerConfig {
    fn default() -> Self {
        Self {
            max_guests: 8,
            bipbuf_capacity: 256 * 1024,
            max_payload_size: 1024 * 1024,
        }
    }
}

impl ShmListenerConfig {
    fn size_classes(&self) -> Vec<vox_shm::SizeClassConfig> {
        vec![
            vox_shm::SizeClassConfig {
                slot_size: 1024,
                slot_count: 16,
            },
            vox_shm::SizeClassConfig {
                slot_size: 16384,
                slot_count: 8,
            },
            vox_shm::SizeClassConfig {
                slot_size: 65536,
                slot_count: 4,
            },
        ]
    }
}

impl VoxListener for ShmListener {
    type Link = vox_shm::ShmLink;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        use std::os::unix::io::AsRawFd;

        let (stream, _addr) = self.control_listener.accept().await?;

        let hub = Arc::clone(&self.hub);
        let segment = Arc::clone(&self.segment);

        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; vox_shm::bootstrap::BOOTSTRAP_REQUEST_LEN];
            use std::io::Read;
            let mut std_stream = stream.into_std()?;
            std_stream.read_exact(&mut buf)?;
            let _request = vox_shm::bootstrap::decode_request(&buf)
                .map_err(|e| std::io::Error::other(format!("bad bootstrap request: {e}")))?;

            let segment_path = segment
                .path()
                .to_str()
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "segment path not UTF-8")
                })?
                .as_bytes();
            let prepared = hub
                .prepare_bootstrap_success(segment_path)
                .map_err(|e| std::io::Error::other(format!("prepare bootstrap: {e}")))?;

            prepared
                .send_success_unix(std_stream.as_raw_fd(), &segment)
                .map_err(|e| std::io::Error::other(format!("send bootstrap: {e}")))?;

            prepared.host_peer.into_link()
        })
        .await
        .map_err(|e| std::io::Error::other(format!("bootstrap task panicked: {e}")))?
    }
}
