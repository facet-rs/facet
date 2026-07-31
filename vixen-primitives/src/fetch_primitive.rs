use sha2::{Digest as _, Sha256, Sha512};

use vix::vir::{ExternKind, Type};

use crate::rt::{
    BlobHandle, DigestAlgorithm, EffectCtx, EffectTicket, PinnedBlobRef, PinnedFetchRequest,
    Primitive, PrimitiveDecl, PrimitiveMachineError, ReadProjection, ResponseValue, UpstreamDigest,
    ValueId,
};
// Only the test-only hand parsers (the `decode_primitive_value` oracle) walk the
// wire `PrimitiveValue` structurally now; production `begin` decodes instead.
#[cfg(test)]
use crate::rt::{
    BlobId, Digest, OriginHint, PrimitiveField, PrimitiveFieldValue, PrimitiveValue,
    PrimitiveValueBody, RegistryHandle,
};
#[cfg(test)]
use vix::schema::SchemaRef;

impl ResponseValue for BlobHandle {
    fn into_value(self) -> ValueId {
        self.0
    }
}

pub struct PinnedFetchPrimitive;

impl<Ctx> Primitive<Ctx> for PinnedFetchPrimitive {
    type Request = PinnedFetchRequest;
    type Response = BlobHandle;
    type Deps = ();

    const DECL: PrimitiveDecl = crate::rt::FETCH_DECL;

    fn begin(
        &self,
        req: PinnedFetchRequest,
        ctx: EffectCtx,
        _deps: (),
    ) -> EffectTicket<BlobHandle> {
        let (ticket, completer) = EffectTicket::<BlobHandle>::pair(&ctx, || {});
        std::thread::spawn(move || {
            let _ = match serve(req.pin, &ctx) {
                Ok(value) => completer.complete_ok(&ctx, BlobHandle(value)),
                Err(error) => completer.complete_err(&ctx, error),
            };
        });
        ticket
    }
}

fn serve(pin: PinnedBlobRef, ctx: &EffectCtx) -> Result<ValueId, PrimitiveMachineError> {
    let target = pin.value.id();

    if let Ok(stored) = ctx.read(&target, ReadProjection::Whole) {
        verify_upstream(&stored.bytes, pin.upstream.as_ref())?;
        return Ok(stored.identity);
    }

    let mut last_error = None;
    if let Some(candidate) = ctx.persisted_candidate(&target)? {
        if candidate.claimed != target {
            last_error = Some(PrimitiveMachineError::CorruptCandidate {
                source: candidate.claimed,
            });
        } else {
            let observed = blob_identity(&candidate.bytes);
            if observed != target {
                last_error = Some(PrimitiveMachineError::CorruptCandidate { source: observed });
            } else {
                verify_upstream(&candidate.bytes, pin.upstream.as_ref())?;
                return admit(&candidate.bytes, &target, ctx);
            }
        }
    }

    for origin in &pin.origins {
        // The pin's upstream digest rides the witness as provenance: a record
        // of what the transfer is checked against (verify_upstream below),
        // recorded beside the vix identity. If the check then fails, the
        // demand fails and the receipt never publishes.
        match ctx.origin_candidate(
            &origin.capability.0,
            &origin.coordinate,
            &target,
            pin.upstream.clone(),
        ) {
            Ok(bytes) => {
                verify_upstream(&bytes, pin.upstream.as_ref())?;
                let admitted = admit(&bytes, &target, ctx)?;
                ctx.persist_value(&admitted, &bytes)?;
                return Ok(admitted);
            }
            // The origin taxonomy, enacted (machine.primitive.origin-verbs):
            // a MISS falls through to the next origin — and the ctx has
            // already witnessed it as a Missing observation, so a
            // multi-origin fallthrough has every attempt in the receipt; a
            // REFUSAL (unroutable coordinate) routes on unwitnessed, nobody
            // looked; anything else — corruption above all — stops the
            // demand instead of shopping for an origin that lies better.
            Err(
                error @ (PrimitiveMachineError::ProjectionMissing { .. }
                | PrimitiveMachineError::OriginUnroutable { .. }),
            ) => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(
        last_error.unwrap_or_else(|| PrimitiveMachineError::Unavailable {
            detail: "pinned Blob has no admissible serving source".to_owned(),
        }),
    )
}

fn admit(
    bytes: &[u8],
    expected: &ValueId,
    ctx: &EffectCtx,
) -> Result<ValueId, PrimitiveMachineError> {
    let admitted = ctx.intern(&Type::Extern(ExternKind::Blob).schema_ref(), bytes)?;
    if &admitted != expected {
        return Err(PrimitiveMachineError::CorruptCandidate { source: admitted });
    }
    Ok(admitted)
}

fn blob_identity(bytes: &[u8]) -> ValueId {
    crate::rt::FramedNode::leaf(Type::Extern(ExternKind::Blob).schema_ref(), bytes.to_vec())
        .identity()
}

/// Compute `algorithm` over `bytes`.
///
/// The match is exhaustive on purpose: adding a [`DigestAlgorithm`] must not
/// compile until this machine can actually compute it, because a pin whose
/// algorithm the verifier silently skips is worse than no pin at all.
fn compute_digest(algorithm: DigestAlgorithm, bytes: &[u8]) -> Vec<u8> {
    match algorithm {
        DigestAlgorithm::Blake3 => blake3::hash(bytes).as_bytes().to_vec(),
        DigestAlgorithm::Sha256 => Sha256::digest(bytes).to_vec(),
        DigestAlgorithm::Sha512 => Sha512::digest(bytes).to_vec(),
    }
}

/// Check the served bytes against the pin the recipe wrote.
///
/// r[impl vixen.pins.0-1-verifies-on-arrival]
fn verify_upstream(
    bytes: &[u8],
    expected: Option<&UpstreamDigest>,
) -> Result<(), PrimitiveMachineError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let observed = compute_digest(expected.algorithm, bytes);
    if observed != expected.bytes {
        // The pin is named in the diagnostic: "contradicts its digest" sends you
        // looking at the wrong thing when what actually happened is that a
        // mirror served different bytes than the lockfile promised.
        return Err(PrimitiveMachineError::PolicyRejected {
            detail: format!(
                "Blob body contradicts its pin: expected {}, served {}:{}",
                expected.render(),
                expected.algorithm.tag(),
                hex::encode(&observed)
            ),
        });
    }
    Ok(())
}

// `#[cfg(test)]`: since `begin` decodes via `decode_primitive_value`, these hand
// parsers survive only as the oracle for `primitive_value_decode`'s tests, which
// assert that decoder agrees with them on the same wire `PrimitiveValue` for the
// real request types. Test-only, so they don't dead-code-warn in a normal build.
#[cfg(test)]
pub(crate) fn parse_request(
    request: PrimitiveValue,
    request_id: ValueId,
) -> Result<PinnedBlobRef, PrimitiveMachineError> {
    let PrimitiveValueBody::Product(request_fields) = request.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [pin] = request_fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let pin = child(pin)?;
    let PrimitiveValueBody::Product(fields) = &pin.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [value, origins, upstream] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let value = parse_blob_id(child(value)?, &request_id)?;
    let origins = parse_origins(child(origins)?)?;
    let upstream = parse_upstream(child(upstream)?)?;
    Ok(PinnedBlobRef {
        value,
        origins,
        upstream,
    })
}

#[cfg(test)]
pub(crate) fn parse_blob_id(
    value: &PrimitiveValue,
    request: &ValueId,
) -> Result<BlobId, PrimitiveMachineError> {
    let PrimitiveValueBody::Product(fields) = &value.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    };
    let [schema, content] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    };
    let schema = SchemaRef::from_canonical_bytes(bytes(child(schema)?)?).map_err(|_| {
        PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        }
    })?;
    let content = core::str::from_utf8(bytes(child(content)?)?)
        .ok()
        .and_then(|text| hex::decode(text).ok())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        })?;
    BlobId::new(ValueId {
        schema,
        content: Digest(content),
    })
}

#[cfg(test)]
pub(crate) fn parse_origins(
    value: &PrimitiveValue,
) -> Result<Vec<OriginHint>, PrimitiveMachineError> {
    let PrimitiveValueBody::Sequence { elements, .. } = &value.body else {
        return Err(invalid_value());
    };
    elements
        .iter()
        .map(|origin| {
            let PrimitiveValueBody::Product(fields) = &origin.body else {
                return Err(invalid_value());
            };
            let [capability, coordinate] = fields.as_slice() else {
                return Err(invalid_value());
            };
            let capability = RegistryHandle(child(capability)?.identity());
            let coordinate = core::str::from_utf8(bytes(child(coordinate)?)?)
                .map_err(|_| invalid_value())?
                .to_owned();
            Ok(OriginHint {
                capability,
                coordinate,
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn parse_upstream(
    value: &PrimitiveValue,
) -> Result<Option<UpstreamDigest>, PrimitiveMachineError> {
    let PrimitiveValueBody::Variant { tag, fields } = &value.body else {
        return Err(invalid_value());
    };
    match (*tag, fields.as_slice()) {
        (0, [digest]) => {
            let digest = core::str::from_utf8(bytes(child(digest)?)?)
                .ok()
                .and_then(|text| UpstreamDigest::parse(text).ok())
                .ok_or_else(invalid_value)?;
            Ok(Some(digest))
        }
        (1, []) => Ok(None),
        _ => Err(invalid_value()),
    }
}

#[cfg(test)]
fn child(field: &PrimitiveField) -> Result<&PrimitiveValue, PrimitiveMachineError> {
    let PrimitiveFieldValue::Child(value) = &field.value else {
        return Err(invalid_value());
    };
    Ok(value)
}

#[cfg(test)]
fn bytes(value: &PrimitiveValue) -> Result<&[u8], PrimitiveMachineError> {
    let PrimitiveValueBody::Bytes(bytes) = &value.body else {
        return Err(invalid_value());
    };
    Ok(bytes)
}

#[cfg(test)]
fn invalid_value() -> PrimitiveMachineError {
    PrimitiveMachineError::AuthorityViolation {
        detail: "pinned fetch request disagrees with its declared schema".to_owned(),
    }
}

#[cfg(test)]
mod origin_fallthrough {
    //! The fetch loop enacting the origin taxonomy over a scripted adapter
    //! set: misses fall through and every attempt lands in the receipt;
    //! corruption stops.

    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use vix::schema::SchemaPattern;
    use vix::vir::{ExternKind, Type};

    use super::PinnedFetchPrimitive;
    use crate::rt::{
        BlobId, DemandKey, DemandPreimage, DigestAlgorithm, EffectCtx, FramedNode, OriginAdapter,
        OriginAdapterDecl, OriginAdapterSet, OriginHint, OriginReadError, PinnedBlobRef,
        PinnedFetchRequest, PrimitiveCompletion, PrimitiveMachineError, PrimitivePublication,
        ReadObservation, ReadProjection, RecipeId, RegistryHandle, StagedEffectAuthority,
        UpstreamDigest, ValueId,
    };
    struct ScriptedOrigin {
        serving: BTreeMap<String, Vec<u8>>,
        asked: Mutex<Vec<String>>,
    }

    impl OriginAdapter for ScriptedOrigin {
        fn read(
            &self,
            _capability: &ValueId,
            coordinate: &str,
        ) -> Result<Vec<u8>, OriginReadError> {
            self.asked
                .lock()
                .expect("scripted origin mutex poisoned")
                .push(coordinate.to_owned());
            self.serving
                .get(coordinate)
                .cloned()
                .ok_or_else(|| OriginReadError::Miss {
                    detail: format!("{coordinate} is not scripted"),
                })
        }
    }

    fn scripted_set(
        serving: impl IntoIterator<Item = (&'static str, Vec<u8>)>,
    ) -> (OriginAdapterSet, Arc<ScriptedOrigin>) {
        let adapter = Arc::new(ScriptedOrigin {
            serving: serving
                .into_iter()
                .map(|(coordinate, bytes)| (coordinate.to_owned(), bytes))
                .collect(),
            asked: Mutex::new(Vec::new()),
        });
        let mut set = OriginAdapterSet::default();
        set.install(
            OriginAdapterDecl {
                name: "scripted".to_owned(),
                schemes: vec!["stub".to_owned()],
                capability: SchemaPattern::exact(
                    &Type::Extern(ExternKind::Registry).schema_ref(),
                ),
                tree_namespace: None,
            },
            adapter.clone(),
        )
        .expect("the scripted declaration overlaps nothing");
        (set, adapter)
    }

    fn registry_capability() -> ValueId {
        FramedNode::leaf(
            Type::Extern(ExternKind::Registry).schema_ref(),
            b"scripted-registry".to_vec(),
        )
        .identity()
    }

    fn run_fetch(
        origins: OriginAdapterSet,
        coordinates: &[&str],
        payload: &[u8],
        upstream: Option<UpstreamDigest>,
    ) -> (PrimitivePublication, ValueId, ValueId) {
        let target =
            FramedNode::leaf(Type::Extern(ExternKind::Blob).schema_ref(), payload.to_vec())
                .identity();
        let capability = registry_capability();
        let request = PinnedFetchRequest {
            pin: PinnedBlobRef {
                value: BlobId::new(target.clone()).expect("a Blob-schema pin"),
                origins: coordinates
                    .iter()
                    .map(|coordinate| OriginHint {
                        capability: RegistryHandle(capability.clone()),
                        coordinate: (*coordinate).to_owned(),
                    })
                    .collect(),
                upstream,
            },
        };
        let demand = DemandKey::from_preimage(&DemandPreimage {
            closure: RecipeId::from_canonical_vir(b"fetch-fallthrough-test"),
            arguments: Vec::new(),
        });
        let authority =
            Arc::new(StagedEffectAuthority::new(std::iter::empty()).with_origins(origins));
        let ctx = EffectCtx::new(demand, authority);
        let ticket =
            crate::typed_primitive::Primitive::<()>::begin(&PinnedFetchPrimitive, request, ctx, ())
                .into_raw();
        let (sender, receiver) = std::sync::mpsc::channel();
        let _subscription = ticket.join(move |publication| {
            let _ = sender.send(publication);
        });
        let publication = receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("the fetch completes");
        (publication, target, capability)
    }

    /// A multi-origin fetch that falls through has EVERY attempt in the
    /// receipt: one Missing-observed witness per tried coordinate, then the
    /// Value witness of the origin that served — not just the winner.
    ///
    /// r[verify machine.primitive.witness-reverification]
    #[test]
    fn a_multi_origin_fallthrough_witnesses_every_tried_coordinate() {
        let (origins, adapter) = scripted_set([("stub://serves", b"payload".to_vec())]);
        let (publication, target, capability) = run_fetch(
            origins,
            &["stub://miss-a", "stub://miss-b", "stub://serves"],
            b"payload",
        
            None,
        );

        assert_eq!(
            publication.completion,
            PrimitiveCompletion::Ok(target.clone())
        );
        assert_eq!(
            *adapter.asked.lock().expect("scripted origin mutex poisoned"),
            ["stub://miss-a", "stub://miss-b", "stub://serves"],
            "each origin is tried in declaration order"
        );
        let reads = &publication.receipt.reads;
        assert_eq!(reads.len(), 3, "every attempt is in the receipt: {reads:#?}");
        for (read, coordinate) in reads.iter().zip(["stub://miss-a", "stub://miss-b"]) {
            assert_eq!(read.source, capability);
            assert_eq!(
                read.projection,
                ReadProjection::Origin {
                    coordinate: coordinate.to_owned(),
                }
            );
            assert_eq!(read.observation, ReadObservation::Missing);
        }
        assert_eq!(
            reads[2].projection,
            ReadProjection::Origin {
                coordinate: "stub://serves".to_owned(),
            }
        );
        assert_eq!(reads[2].observation, ReadObservation::Value(target));
    }

    /// Corruption stops: an origin serving bytes that contradict the pin is
    /// not "not found here, try the next" — the loop returns the corruption
    /// instead of shopping for an origin that lies better, and later origins
    /// are never contacted.
    ///
    /// r[verify machine.primitive.origin-verbs]
    #[test]
    fn a_corrupt_candidate_stops_the_fallthrough_loop() {
        let (origins, adapter) = scripted_set([
            ("stub://corrupt", b"not the pinned bytes".to_vec()),
            ("stub://never-reached", b"payload".to_vec()),
        ]);
        let (publication, _target, _capability) = run_fetch(
            origins,
            &["stub://corrupt", "stub://never-reached"],
            b"payload",
        
            None,
        );

        assert!(
            matches!(
                publication.completion,
                PrimitiveCompletion::MachineError(PrimitiveMachineError::CorruptCandidate { .. })
            ),
            "corruption surfaces typed: {:#?}",
            publication.completion
        );
        assert_eq!(
            *adapter.asked.lock().expect("scripted origin mutex poisoned"),
            ["stub://corrupt"],
            "the loop stops at the corruption; later origins are never asked"
        );
    }

    /// The verified upstream digest enters the receipt beside the vix
    /// identity: the origin transfer's Value witness carries it as
    /// provenance, and a tried-and-missed coordinate carries none (nothing
    /// arrived to check).
    ///
    /// r[verify machine.primitive.fetch-integrity-vs-identity]
    /// r[verify machine.primitive.witness-reverification]
    #[test]
    fn the_verified_upstream_digest_rides_the_receipt_as_provenance() {
        use sha2::{Digest as _, Sha256};

        let payload = b"payload";
        let upstream = UpstreamDigest {
            algorithm: DigestAlgorithm::Sha256,
            bytes: Sha256::digest(payload).to_vec(),
        };
        let (origins, _adapter) = scripted_set([("stub://serves", payload.to_vec())]);
        let (publication, target, _capability) = run_fetch(
            origins,
            &["stub://miss", "stub://serves"],
            payload,
            Some(upstream.clone()),
        );

        assert_eq!(publication.completion, PrimitiveCompletion::Ok(target));
        let reads = &publication.receipt.reads;
        assert_eq!(reads.len(), 2, "{reads:#?}");
        assert_eq!(reads[0].observation, ReadObservation::Missing);
        assert_eq!(
            reads[0].provenance, None,
            "a miss has no provenance: nothing arrived to check"
        );
        assert!(matches!(reads[1].observation, ReadObservation::Value(_)));
        assert_eq!(
            reads[1].provenance,
            Some(upstream),
            "both digests are in the receipt: the vix identity as the \
             observation, the verified upstream digest as provenance"
        );
    }
}

#[cfg(test)]
mod schema_snapshot {
    //! Wire-identity gate for the type-schema-unification PR.
    //!
    //! `PrimitiveDescriptor.request_schema = SchemaPattern::exact(&…type().schema_ref())`,
    //! so dispatch matching and wire identity depend on the *exact* schema bytes
    //! of each request/hint/ref type. These snapshots are the canonical
    //! `SchemaRef::Display` (see `schema.rs`) of the five types captured against
    //! the hand-written constructors; deriving them from the `Facet` shapes must
    //! keep every one byte-identical.

    use crate::rt::{BlobId, OriginHint, PinnedBlobRef, PinnedFetchRequest};
    use vix::vir::Type;

    // Captured against the hand-written `*_type()` constructors before they were
    // deleted; the derived `Type::from_facet` types must match byte-for-byte.
    const BLOB_ID: &str = "e5904d597f3968d0";
    const ORIGIN_HINT: &str = "ecd3ac5ba264e915";
    const PINNED_BLOB_REF: &str = "a077429ce555df22";
    const PINNED_FETCH_REQUEST: &str = "053ce66d21abed59";

    #[test]
    fn request_schemas_are_byte_identical() {
        assert_eq!(
            Type::from_facet::<BlobId>().schema_ref().to_string(),
            BLOB_ID
        );
        assert_eq!(
            Type::from_facet::<OriginHint>().schema_ref().to_string(),
            ORIGIN_HINT
        );
        assert_eq!(
            Type::from_facet::<PinnedBlobRef>().schema_ref().to_string(),
            PINNED_BLOB_REF
        );
        assert_eq!(
            Type::from_facet::<PinnedFetchRequest>()
                .schema_ref()
                .to_string(),
            PINNED_FETCH_REQUEST
        );
    }
}
