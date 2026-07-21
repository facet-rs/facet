use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::vir::{ExternKind, Type};

use super::{FramedNode, PrimitiveMachineError, ValueBodyCandidate, ValueId, ValuePersistence};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// The canonical FV-D1 body codec. It admits only raw `Blob` bodies and
/// verifies them through the single framed identity writer on both encode and
/// decode.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlobBodyCodec;

impl BlobBodyCodec {
    pub fn verify(self, claimed: &ValueId, bytes: &[u8]) -> Result<(), PrimitiveMachineError> {
        if claimed.schema != Type::Extern(ExternKind::Blob).schema_ref() {
            return Err(PrimitiveMachineError::PolicyRejected {
                detail: "canonical Blob persistence rejects non-Blob values".to_owned(),
            });
        }
        let observed = FramedNode::leaf(claimed.schema.clone(), bytes.to_vec()).identity();
        if &observed != claimed {
            return Err(PrimitiveMachineError::CorruptCandidate { source: observed });
        }
        Ok(())
    }
}

/// Filesystem-backed immutable Blob bodies. A body is addressed by its Vix
/// content identity under a schema-specific namespace; the path is only an
/// index, and every read is reverified before the candidate can be admitted.
#[derive(Clone, Debug)]
pub struct CanonicalBlobPersistence {
    root: PathBuf,
    codec: BlobBodyCodec,
}

impl CanonicalBlobPersistence {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            codec: BlobBodyCodec,
        }
    }

    fn body_path(&self, value: &ValueId) -> Result<PathBuf, PrimitiveMachineError> {
        if value.schema != Type::Extern(ExternKind::Blob).schema_ref() {
            return Err(PrimitiveMachineError::PolicyRejected {
                detail: "canonical Blob persistence rejects non-Blob values".to_owned(),
            });
        }
        let digest = value.content.hex();
        Ok(self
            .root
            .join("vix-blob-v2")
            .join(&digest[..2])
            .join(digest))
    }

    fn unavailable(operation: &str, path: &Path, error: std::io::Error) -> PrimitiveMachineError {
        PrimitiveMachineError::Unavailable {
            detail: format!(
                "Blob persistence could not {operation} {}: {error}",
                path.display()
            ),
        }
    }

    fn create_temporary(path: &Path) -> Result<(PathBuf, File), PrimitiveMachineError> {
        let parent = path
            .parent()
            .ok_or_else(|| PrimitiveMachineError::Unavailable {
                detail: format!("Blob persistence path {} has no parent", path.display()),
            })?;
        loop {
            let nonce = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(".vix-blob-{}-{nonce}.tmp", std::process::id()));
            match OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
            {
                Ok(file) => return Ok((temporary, file)),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(Self::unavailable("create", &temporary, error)),
            }
        }
    }
}

impl ValuePersistence for CanonicalBlobPersistence {
    fn get(&self, value: &ValueId) -> Result<Option<ValueBodyCandidate>, PrimitiveMachineError> {
        let path = self.body_path(value)?;
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Self::unavailable("read", &path, error)),
        };
        self.codec.verify(value, &bytes)?;
        Ok(Some(ValueBodyCandidate {
            claimed: value.clone(),
            bytes,
        }))
    }

    fn put(&self, value: &ValueId, bytes: &[u8]) -> Result<(), PrimitiveMachineError> {
        self.codec.verify(value, bytes)?;
        let path = self.body_path(value)?;
        if let Some(existing) = self.get(value)? {
            self.codec.verify(&existing.claimed, &existing.bytes)?;
            return Ok(());
        }
        let parent = path
            .parent()
            .ok_or_else(|| PrimitiveMachineError::Unavailable {
                detail: format!("Blob persistence path {} has no parent", path.display()),
            })?;
        fs::create_dir_all(parent)
            .map_err(|error| Self::unavailable("create directory", parent, error))?;
        let (temporary, mut file) = Self::create_temporary(&path)?;
        file.write_all(bytes)
            .map_err(|error| Self::unavailable("write", &temporary, error))?;
        file.sync_all()
            .map_err(|error| Self::unavailable("sync", &temporary, error))?;
        fs::rename(&temporary, &path)
            .map_err(|error| Self::unavailable("publish", &path, error))?;
        #[cfg(unix)]
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| Self::unavailable("sync directory", parent, error))?;
        Ok(())
    }
}
