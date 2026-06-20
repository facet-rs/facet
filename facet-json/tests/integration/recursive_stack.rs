use std::cell::RefCell;
use std::convert::Infallible;

use facet::Facet;

#[derive(Facet, Debug, PartialEq, Eq)]
struct IdentityNode {
    id: u32,
    #[facet(proxy = StackProbeProxy)]
    probe: StackProbe,
    #[facet(recursive_type)]
    child: Option<Box<IdentityNode>>,
}

#[derive(Facet, Debug, PartialEq, Eq)]
struct StackProbe;

#[derive(Facet)]
#[facet(transparent)]
struct StackProbeProxy(u8);

impl TryFrom<&StackProbe> for StackProbeProxy {
    type Error = Infallible;

    fn try_from(_: &StackProbe) -> Result<Self, Self::Error> {
        record_stack_sample();
        Ok(Self(0))
    }
}

impl TryFrom<StackProbeProxy> for StackProbe {
    type Error = Infallible;

    fn try_from(_: StackProbeProxy) -> Result<Self, Self::Error> {
        record_stack_sample();
        Ok(Self)
    }
}

#[derive(Debug, Default)]
struct StackMeasurement {
    first_remaining: Option<usize>,
    min_remaining: usize,
    samples: usize,
}

impl StackMeasurement {
    fn record(&mut self, remaining: usize) {
        self.first_remaining.get_or_insert(remaining);
        self.min_remaining = if self.samples == 0 {
            remaining
        } else {
            self.min_remaining.min(remaining)
        };
        self.samples += 1;
    }

    fn finish(self) -> StackUsage {
        let first_remaining = self
            .first_remaining
            .expect("recursive JSON stack test did not take any stack samples");
        StackUsage {
            samples: self.samples,
            bytes: first_remaining.saturating_sub(self.min_remaining),
        }
    }
}

#[derive(Debug)]
struct StackUsage {
    samples: usize,
    bytes: usize,
}

thread_local! {
    static STACK_MEASUREMENT: RefCell<Option<StackMeasurement>> = RefCell::new(None);
}

fn record_stack_sample() {
    STACK_MEASUREMENT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(measurement) = slot.as_mut() {
            let remaining = stacker::remaining_stack()
                .expect("stacker::remaining_stack is unavailable on this target");
            measurement.record(remaining);
        }
    });
}

fn measure_stack_usage<T>(f: impl FnOnce() -> T) -> (T, StackUsage) {
    STACK_MEASUREMENT.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "recursive JSON stack measurement was already active"
        );
        *slot.borrow_mut() = Some(StackMeasurement::default());
    });

    let result = f();
    let measurement = STACK_MEASUREMENT.with(|slot| {
        slot.borrow_mut()
            .take()
            .expect("recursive JSON stack measurement disappeared")
    });
    (result, measurement.finish())
}

impl IdentityNode {
    fn chain(depth: u32) -> Self {
        let mut node = Self {
            id: depth,
            probe: StackProbe,
            child: None,
        };

        for id in (0..depth).rev() {
            node = Self {
                id,
                probe: StackProbe,
                child: Some(Box::new(node)),
            };
        }

        node
    }

    fn depth(&self) -> u32 {
        let mut depth = 0;
        let mut current = self;
        while let Some(child) = current.child.as_deref() {
            depth += 1;
            current = child;
        }
        depth
    }
}

fn expected_json(depth: u32) -> String {
    let mut json = format!(r#"{{"id":{depth},"probe":0,"child":null}}"#);
    for id in (0..depth).rev() {
        json = format!(r#"{{"id":{id},"probe":0,"child":{json}}}"#);
    }
    json
}

fn configured_depth() -> u32 {
    std::env::var("FACET_RECURSIVE_STACK_DEPTH")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(23)
}

fn configured_stack_limit(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn assert_stack_usage(operation: &str, depth: u32, usage: StackUsage, max_bytes: usize) {
    let expected_samples = depth as usize + 1;
    assert_eq!(
        usage.samples, expected_samples,
        "{operation} should sample stack usage once per recursive node"
    );
    if depth > 0 {
        assert!(
            usage.bytes > 0,
            "{operation} did not observe recursive stack growth"
        );
    }
    assert!(
        usage.bytes <= max_bytes,
        "{operation} used {} bytes of stack for depth {depth}, above {max_bytes} bytes",
        usage.bytes
    );
}

#[test]
fn recursive_identity_node_json_serializes() {
    let depth = configured_depth();
    let node = IdentityNode::chain(depth);

    let (json, stack_usage) = measure_stack_usage(|| {
        facet_json::to_string(&node).expect("recursive node should serialize")
    });
    assert_eq!(json, expected_json(depth));
    assert_stack_usage(
        "recursive JSON serialization",
        depth,
        stack_usage,
        configured_stack_limit("FACET_RECURSIVE_STACK_MAX_SERIALIZE_BYTES", 512 * 1024),
    );
}

#[test]
fn recursive_identity_node_json_deserializes() {
    let depth = configured_depth();
    let node = IdentityNode::chain(depth);
    let json = expected_json(depth);

    let (parsed, stack_usage) = measure_stack_usage(|| {
        facet_json::from_str::<IdentityNode>(&json).expect("recursive node should deserialize")
    });
    assert_eq!(parsed.depth(), depth);
    assert_eq!(parsed, node);
    assert_stack_usage(
        "recursive JSON deserialization",
        depth,
        stack_usage,
        configured_stack_limit(
            "FACET_RECURSIVE_STACK_MAX_DESERIALIZE_BYTES",
            2 * 1024 * 1024,
        ),
    );
}
