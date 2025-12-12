use picante::Revision;
use picante::ingredient::InputIngredient;
use picante::key::QueryKindId;
use picante::runtime::{HasRuntime, Runtime, RuntimeEvent};

fn init_tracing() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .try_init();
    });
}

#[derive(Default)]
struct TestDb {
    runtime: Runtime,
}

impl HasRuntime for TestDb {
    fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}

#[tokio::test]
async fn revision_watch_updates_on_input_set() {
    init_tracing();

    let db = TestDb::default();
    let mut revisions = db.runtime().subscribe_revisions();

    assert_eq!(*revisions.borrow(), Revision(0));

    let input: InputIngredient<String, String> = InputIngredient::new(QueryKindId(1), "Text");
    input.set(&db, "a".into(), "hello".into());

    revisions.changed().await.unwrap();
    assert_eq!(*revisions.borrow(), Revision(1));
}

#[tokio::test]
async fn input_set_and_remove_emit_events() {
    init_tracing();

    let db = TestDb::default();
    let mut events = db.runtime().subscribe_events();

    let input: InputIngredient<String, String> = InputIngredient::new(QueryKindId(1), "Text");

    input.set(&db, "a".into(), "hello".into());

    match events.recv().await.unwrap() {
        RuntimeEvent::RevisionBumped { revision } => assert_eq!(revision, Revision(1)),
        other => panic!("expected RevisionBumped, got {other:?}"),
    }

    match events.recv().await.unwrap() {
        RuntimeEvent::InputSet {
            revision,
            kind,
            key,
            ..
        } => {
            assert_eq!(revision, Revision(1));
            assert_eq!(kind, QueryKindId(1));
            assert_eq!(key.decode_facet::<String>().unwrap(), "a");
        }
        other => panic!("expected InputSet, got {other:?}"),
    }

    input.remove(&db, &"a".into());

    match events.recv().await.unwrap() {
        RuntimeEvent::RevisionBumped { revision } => assert_eq!(revision, Revision(2)),
        other => panic!("expected RevisionBumped, got {other:?}"),
    }

    match events.recv().await.unwrap() {
        RuntimeEvent::InputRemoved {
            revision,
            kind,
            key,
            ..
        } => {
            assert_eq!(revision, Revision(2));
            assert_eq!(kind, QueryKindId(1));
            assert_eq!(key.decode_facet::<String>().unwrap(), "a");
        }
        other => panic!("expected InputRemoved, got {other:?}"),
    }
}
