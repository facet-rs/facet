//! `exec` — the process-running effect as a REGISTERED primitive.
//!
//! **Why this is a primitive and not a machine op.** It was `Op::Exec` until
//! now, the last capability-carrying effect resident in the scheduler (issue
//! 2597). The scheduler kept exactly what `machine.primitive.registered`
//! permits — keying, parking, admission, receipts — but it also owned the
//! process boundary, the workspace capture, the termination mapping, and a
//! per-tool record-name match selecting the output protocol, which is exactly
//! what `machine.capability.no-argv-dialect` bans from machine code. All of
//! that is domain knowledge, and it lives HERE now: `vixen-primitives` is
//! allowed to know what a `ProgressiveSh` is; `vix-core` is not.
//!
//! The demand identity is unchanged in shape: "this plan under this
//! capability". The request's capability field is a capability-ROLE argument,
//! so its identity is the demand preimage's argument and its value (the
//! `$program` executable name) is redeemed only here, host-side; the
//! materialized argv is the normalized plan the closure hashes
//! (`machine.primitive.capability-role`, `machine.primitive.exec-plan-normalized`).
//!
//! The process boundary itself stays behind the [`ExecBackend`] service
//! (`machine.primitive.effect-backend-service`): this primitive spawns
//! nothing — it asks the installed backend to, and a snapshot without one
//! fails loudly. The host-trusting default backend supports no `Hermetic`
//! claim, which is why the effect's capability witness is recorded
//! `Unverifiable` (`machine.primitive.memo-policy`).
//!
//! r[impl machine.primitive.capability-role]
//! r[impl machine.primitive.effect-backend-service]
//! r[impl machine.primitive.exec-outcome]

use std::sync::Arc;

use vix::compiler::{byte_stream_type, capability_type, exec_outcome_type};
use vix::runtime::{
    ExecEvent, ExecInvocation, ProcessTermination, ReadProjection, archive_directory,
    canonical_resident_tree, exec_primitive_id, exec_request_type, tree_from_resident,
};
use vix::schema::SchemaPattern;
use vix::vir::Type;

use crate::rt::{
    ArgRole, EffectCtx, PrimitiveCompletion, PrimitiveDescriptor, PrimitiveField,
    PrimitiveFieldValue, PrimitiveMachineError, PrimitiveMemoPolicy, PrimitivePublication,
    PrimitiveValue, PrimitiveValueBody, ProgressivePublication, RawEffectTicket, RawPrimitive,
    Receipt, RequestShape, ValueId,
};

/// The registered exec primitive. Hand-written on the raw rail rather than the
/// typed [`crate::rt::Primitive`] layer for one load-bearing reason: the
/// request's capability field keeps the *program's own capability type*
/// (`Echo`, `Sh`, `ProgressiveSh`, …), so the request schema varies per
/// capability and cannot decode into one fixed Facet type. The descriptor's
/// request pattern is a schema VARIABLE for the same reason.
pub struct ExecPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for ExecPrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: exec_primitive_id(),
                // The capability field's type varies per capability, so the
                // request schema is a pattern variable; admissibility is the
                // lowering's concern (only `exec` syntax builds this request).
                request_schema: SchemaPattern::Var {
                    name: "ExecRequest".to_owned(),
                },
                response_schema: SchemaPattern::exact(&exec_outcome_type().schema_ref()),
                failure_schema: SchemaPattern::Var {
                    name: "ProcessFailure".to_owned(),
                },
                // A host-trusting backend performs ambient reads it cannot
                // witness: the memo is observation-backed, never `Hermetic`
                // (`machine.primitive.memo-policy`).
                memo_policy: PrimitiveMemoPolicy::Observed,
                protocol_version: 1,
                // The concrete capability is a request value referenced by
                // identity; its admissible types vary per program, so none are
                // enumerated here.
                capability_schemas: Vec::new(),
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for ExecPrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    /// The declared shape the scheduler derives the effect demand preimage
    /// from: `capability` is the capability-role argument (its identity keys
    /// the demand), `argv` is the normalized plan (it enters the closure). The
    /// `expected` types are representative — `exec` is keyword syntax lowered
    /// by the core compiler, so no generic call-lowering consults them; only
    /// the ROLES are load-bearing (`machine.primitive.capability-role`).
    fn request_shape(&self) -> Option<RequestShape> {
        let capability = capability_type("Sh");
        Some(RequestShape {
            args: vec![
                ArgRole::Capability {
                    expected: capability.clone(),
                },
                ArgRole::Value {
                    expected: Type::Array(Box::new(Type::String)),
                },
            ],
            request_ty: exec_request_type(&capability),
            result: exec_outcome_type(),
            primitive: exec_primitive_id(),
        })
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx, _app: &Ctx) -> RawEffectTicket {
        let witnessed = match ctx.read(&request, ReadProjection::Whole) {
            Ok(witnessed) => witnessed,
            Err(error) => return complete_with_error(&ctx, error),
        };
        let parsed = match parse_request(&ctx, &witnessed.value) {
            Ok(parsed) => parsed,
            Err(error) => return complete_with_error(&ctx, error),
        };
        let backend = match ctx.exec_backend() {
            Ok(backend) => backend,
            Err(error) => return complete_with_error(&ctx, error),
        };
        let (ticket, completer) = ctx.ticket(|| {});
        // One worker thread owns the whole exchange: events are buffered in the
        // channel until the workspace handle is in hand, so a process that
        // terminates before `begin` returns loses nothing.
        std::thread::spawn(move || {
            let (event_tx, event_rx) = std::sync::mpsc::channel();
            let events: vix::runtime::ExecEventSender = Arc::new(move |event| {
                let _ = event_tx.send(event);
            });
            let workspace = match backend.begin(parsed.invocation, events) {
                Ok(workspace) => workspace,
                Err(detail) => {
                    let _ = completer.complete(publication_or_fallback(
                        &ctx,
                        PrimitiveCompletion::MachineError(PrimitiveMachineError::Unavailable {
                            detail,
                        }),
                    ));
                    return;
                }
            };
            let completion = loop {
                match event_rx.recv() {
                    // A protocol-announced immutable product: stage the
                    // snapshot and publish its readiness while the process
                    // runs (machine.primitive.progressive-response).
                    Ok(ExecEvent::Product(Ok(product))) => {
                        match stage_and_publish(&ctx, &product.path, product.bytes) {
                            Ok(()) => {}
                            Err(error) => break PrimitiveCompletion::MachineError(error),
                        }
                    }
                    Ok(ExecEvent::Product(Err(detail))) => {
                        break PrimitiveCompletion::MachineError(
                            PrimitiveMachineError::Unavailable { detail },
                        );
                    }
                    Ok(ExecEvent::Terminated(Ok(output))) => {
                        break terminated(&ctx, workspace.path(), &output);
                    }
                    Ok(ExecEvent::Terminated(Err(detail))) => {
                        break PrimitiveCompletion::MachineError(
                            PrimitiveMachineError::Unavailable { detail },
                        );
                    }
                    // Every sender dropped without a termination: a boundary
                    // violation by the backend, surfaced loudly.
                    Err(_) => {
                        break PrimitiveCompletion::MachineError(
                            PrimitiveMachineError::Unavailable {
                                detail: "exec backend dropped its event channel before \
                                         termination"
                                    .to_owned(),
                            },
                        );
                    }
                }
            };
            let _ = completer.complete(publication_or_fallback(&ctx, completion));
            drop(workspace);
        });
        ticket
    }
}

struct ParsedRequest {
    invocation: ExecInvocation,
}

/// Read the request record apart: the capability's `$program` value and output
/// protocol, and the materialized argv. The protocol comes from the
/// capability's TYPED content — the record type the program named — which is
/// exactly the per-tool knowledge `machine.capability.no-argv-dialect` bans
/// from the machine and this crate is allowed to hold.
fn parse_request(
    ctx: &EffectCtx,
    request: &PrimitiveValue,
) -> Result<ParsedRequest, PrimitiveMachineError> {
    let invalid = |detail: &str| PrimitiveMachineError::AuthorityViolation {
        detail: format!("malformed exec request: {detail}"),
    };
    let PrimitiveValueBody::Product(fields) = &request.body else {
        return Err(invalid("request was not a record"));
    };
    let [capability_field, argv_field] = fields.as_slice() else {
        return Err(invalid("request does not have exactly two fields"));
    };
    let capability = child_value(capability_field).ok_or_else(|| invalid("capability field"))?;
    let capability_ty = ctx.type_for_schema(&capability.schema)?;
    let Type::Record(capability_record) = &capability_ty else {
        return Err(invalid("capability was not a record value"));
    };
    // The output protocol and the command grammar are contracts of the
    // capability package (`machine.primitive.command-package`), read from the
    // registered package data — never a per-tool match arm in this function.
    let package = crate::capability_package::capability_package(&capability_record.name)
        .ok_or_else(|| invalid("capability names no registered package"))?;
    let protocol = package.protocol;
    let PrimitiveValueBody::Product(capability_fields) = &capability.body else {
        return Err(invalid("capability had no fields"));
    };
    if capability_fields.len() != capability_record.fields.len() {
        return Err(invalid("capability fields disagree with its declared type"));
    }
    let program_index = capability_record
        .fields
        .iter()
        .position(|field| field.name == vix::compiler::CAPABILITY_PROGRAM_FIELD)
        .ok_or_else(|| invalid("capability has no program field"))?;
    let program_bytes = match &capability_fields[program_index].value {
        PrimitiveFieldValue::Inline(bytes) => bytes.clone(),
        PrimitiveFieldValue::Child(child) => child.resident_bytes().to_vec(),
    };
    let program = String::from_utf8(program_bytes)
        .map_err(|_| invalid("capability program was not UTF-8"))?;
    let argv_value = child_value(argv_field).ok_or_else(|| invalid("argv field"))?;
    let PrimitiveValueBody::Sequence { elements, .. } = &argv_value.body else {
        return Err(invalid("argv was not a sequence"));
    };
    let argv = elements
        .iter()
        .map(|element| {
            String::from_utf8(element.resident_bytes().to_vec())
                .map_err(|_| invalid("argv element was not UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // The package's grammar carves declared environment roles out of the
    // materialized plan (env-shaped packages spell them as leading `NAME=VALUE`
    // elements). The demand preimage already hashed the full normalized plan;
    // this split is host-side value redemption, like the program name itself.
    let (env_remove, env, argv) = package.split_invocation(argv);
    Ok(ParsedRequest {
        invocation: ExecInvocation {
            program,
            argv,
            env_remove,
            env,
            protocol,
        },
    })
}

fn child_value(field: &PrimitiveField) -> Option<&PrimitiveValue> {
    match &field.value {
        PrimitiveFieldValue::Child(child) => Some(child),
        PrimitiveFieldValue::Inline(_) => None,
    }
}

/// Stage one immutable product snapshot and publish its readiness projection.
/// The publication is recorded in the effect transaction FIRST (the completion
/// witnesses it) and forwarded live second — the scheduler can serve a parked
/// projection demand before this process exits.
///
/// r[impl machine.primitive.progressive-response]
fn stage_and_publish(
    ctx: &EffectCtx,
    path: &str,
    bytes: Vec<u8>,
) -> Result<(), PrimitiveMachineError> {
    let value = ctx.intern(&Type::String.schema_ref(), &bytes)?;
    ctx.publish_progress(ProgressivePublication {
        projection: ReadProjection::TreePath {
            path: path.to_owned(),
        },
        value,
    });
    Ok(())
}

/// Map one raw termination through the capability package's termination
/// grammar. Today's grammar is the trivial one — exit zero yields the outcome,
/// any other termination is a typed `ProcessFailure` carrying the raw
/// termination data — and it runs HERE, effect-side: the scheduler never sees
/// a status integer (`machine.primitive.exit-status-is-not-a-value`).
fn terminated(
    ctx: &EffectCtx,
    workspace: &std::path::Path,
    output: &std::process::Output,
) -> PrimitiveCompletion {
    if !output.status.success() {
        let termination = match output.status.code() {
            Some(code) => ProcessTermination::Exited {
                code: i64::from(code),
            },
            None => {
                #[cfg(unix)]
                let signal = {
                    use std::os::unix::process::ExitStatusExt as _;
                    i64::from(output.status.signal().unwrap_or_default())
                };
                #[cfg(not(unix))]
                let signal = 0;
                ProcessTermination::Signaled { signal }
            }
        };
        return PrimitiveCompletion::ProcessFailed {
            termination,
            diagnostic: output.stderr.clone(),
        };
    }
    match successful_outcome(ctx, workspace, output) {
        Ok(identity) => PrimitiveCompletion::Ok(identity),
        Err(error) => PrimitiveCompletion::MachineError(error),
    }
}

/// Capture the workspace as the canonical response tree, publish every file's
/// readiness (the completion is the fallback authority for projections the
/// protocol never announced — the exit-time counterpart of the retired
/// workspace read-back), and stage the `ExecOutcome` value.
fn successful_outcome(
    ctx: &EffectCtx,
    workspace: &std::path::Path,
    output: &std::process::Output,
) -> Result<ValueId, PrimitiveMachineError> {
    let unavailable = |detail: String| PrimitiveMachineError::Unavailable { detail };
    let archived = archive_directory(workspace).map_err(unavailable)?;
    let canonical = canonical_resident_tree(&archived)
        .map_err(|error| unavailable(format!("exec capture does not describe a tree: {error}")))?;
    let tree = tree_from_resident(&canonical)
        .map_err(|error| unavailable(format!("exec capture did not decode: {error}")))?;
    for (path, entry) in tree.walk() {
        if !matches!(entry, vix::runtime::TreeEntry::File { .. }) {
            continue;
        }
        let bytes = tree
            .file_bytes(&path)
            .ok_or_else(|| unavailable(format!("exec capture lost `{path}`")))?
            .to_vec();
        stage_and_publish(ctx, &path, bytes)?;
    }
    ctx.intern_value(outcome_value(&canonical, &output.stdout, &output.stderr))
}

/// Build the `ExecOutcome` value — `{ tree, stdout, stderr }`, stdout/stderr
/// as lossy UTF-8 line maps settled at exit — framed exactly as the retired
/// machine-op path framed it, so the outcome's identity survives the move.
/// The settled byte-codata `ExecOutcome<A>` shape
/// (`machine.primitive.exec-outcome`) remains an honest delta.
fn outcome_value(canonical_tree: &[u8], stdout: &[u8], stderr: &[u8]) -> PrimitiveValue {
    let outcome_ty = exec_outcome_type();
    let stream_ty = byte_stream_type();
    let Type::Record(outcome_record) = &outcome_ty else {
        unreachable!("the exec outcome is a record type");
    };
    let tree_ty = outcome_record.fields[0].ty.clone();
    let Type::Record(stream_record) = &stream_ty else {
        unreachable!("a byte stream is a record type");
    };
    let lines_ty = stream_record.fields[0].ty.clone();
    let int_schema = Type::Int.schema_ref();
    let string_schema = Type::String.schema_ref();

    let stream_value = |bytes: &[u8]| -> PrimitiveValue {
        let text = String::from_utf8_lossy(bytes);
        let rows = text
            .lines()
            .enumerate()
            .map(|(index, line)| {
                (
                    PrimitiveValue::bytes(
                        int_schema.clone(),
                        (index as i64).to_le_bytes().to_vec(),
                    ),
                    PrimitiveValue::bytes(string_schema.clone(), line.as_bytes().to_vec()),
                )
            })
            .collect::<Vec<_>>();
        let map = PrimitiveValue {
            schema: lines_ty.schema_ref(),
            body: PrimitiveValueBody::OrderedMap(rows),
        };
        let full = PrimitiveValue::bytes(string_schema.clone(), text.as_bytes().to_vec());
        PrimitiveValue {
            schema: stream_ty.schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                PrimitiveField {
                    schema: lines_ty.schema_ref(),
                    value: PrimitiveFieldValue::Child(Box::new(map)),
                },
                PrimitiveField {
                    schema: string_schema.clone(),
                    value: PrimitiveFieldValue::Child(Box::new(full)),
                },
            ]),
        }
    };

    let tree_value = PrimitiveValue::bytes(tree_ty.schema_ref(), canonical_tree.to_vec());
    PrimitiveValue {
        schema: outcome_ty.schema_ref(),
        body: PrimitiveValueBody::Product(vec![
            PrimitiveField {
                schema: tree_ty.schema_ref(),
                value: PrimitiveFieldValue::Child(Box::new(tree_value)),
            },
            PrimitiveField {
                schema: stream_ty.schema_ref(),
                value: PrimitiveFieldValue::Child(Box::new(stream_value(stdout))),
            },
            PrimitiveField {
                schema: stream_ty.schema_ref(),
                value: PrimitiveFieldValue::Child(Box::new(stream_value(stderr))),
            },
        ]),
    }
}

/// Build the publication from a completion, mirroring the `finish`-error
/// fallback the other primitives use: a failed `finish` still publishes a
/// machine error, never panics.
fn publication_or_fallback(
    ctx: &EffectCtx,
    completion: PrimitiveCompletion,
) -> PrimitivePublication {
    ctx.finish(completion)
        .unwrap_or_else(|error| PrimitivePublication {
            completion: PrimitiveCompletion::MachineError(error),
            receipt: Receipt {
                demand: ctx.demand(),
                reads: Vec::new(),
            },
            journal: Vec::new(),
            progressive: Vec::new(),
        })
}

fn complete_with_error(ctx: &EffectCtx, error: PrimitiveMachineError) -> RawEffectTicket {
    let (ticket, completer) = ctx.ticket(|| {});
    let publication = publication_or_fallback(ctx, PrimitiveCompletion::MachineError(error));
    let _ = completer.complete(publication);
    ticket
}
