use facet::Facet;
use facet_reflect::Peek;
use facet_testhelpers::test;
use std::rc::Rc;
use std::sync::Arc;

#[test]
fn test_peek_arc() {
    let source = Arc::new(42);
    let peek_value = Peek::new(&source);

    let peek_smart_pointer = peek_value.into_smart_pointer()?;

    let def = peek_smart_pointer.def();

    assert_eq!(def.pointee(), Some(i32::SHAPE));
}

#[test]
fn test_peek_arc_with_string() {
    let source = Arc::new("Hello, world!".to_string());
    let peek_value = Peek::new(&source);

    let peek_smart_pointer = peek_value.into_smart_pointer()?;

    let def = peek_smart_pointer.def();

    assert_eq!(def.pointee(), Some(String::SHAPE));
}

#[test]
fn test_peek_arc_in_struct() {
    #[derive(Facet)]
    struct TestStruct {
        data: Arc<String>,
    }

    let source = TestStruct {
        data: Arc::new("Hello, world!".to_string()),
    };

    let peek_value = Peek::new(&source);
    let peek_struct = peek_value.into_struct()?;
    let peek_data = peek_struct.field_by_name("data").unwrap();

    let peek_smart_pointer = peek_data.into_smart_pointer()?;

    let def = peek_smart_pointer.def();
    assert!(def.flags.contains(facet_core::SmartPointerFlags::ATOMIC));

    assert_eq!(def.pointee(), Some(String::SHAPE));
}

#[test]
fn test_peek_arc_in_vec() {
    let source = vec![Arc::new(1), Arc::new(2), Arc::new(3)];
    let peek_value = Peek::new(&source);
    let peek_list = peek_value.into_list()?;

    assert_eq!(peek_list.len(), 3);

    for item in peek_list.iter() {
        let peek_smart_pointer = item.into_smart_pointer()?;

        let def = peek_smart_pointer.def();
        assert_eq!(def.pointee(), Some(i32::SHAPE));
        assert!(def.flags.contains(facet_core::SmartPointerFlags::ATOMIC));
    }
}

#[test]
fn test_smart_pointer_flags() {
    let source = Arc::new(42);
    let peek_value = Peek::new(&source);
    let peek_smart_pointer = peek_value.into_smart_pointer()?;

    let def = peek_smart_pointer.def();
    assert!(def.flags.contains(facet_core::SmartPointerFlags::ATOMIC));
    assert!(!def.flags.contains(facet_core::SmartPointerFlags::WEAK));
    assert!(!def.flags.contains(facet_core::SmartPointerFlags::LOCK));

    if let Some(known_type) = def.known {
        assert_eq!(known_type, facet_core::KnownSmartPointer::Arc);
    }
}

#[test]
fn test_peek_rc() {
    let source = Rc::new(42);
    let peek_value = Peek::new(&source);

    let peek_smart_pointer = peek_value.into_smart_pointer()?;

    let def = peek_smart_pointer.def();

    assert_eq!(def.pointee(), Some(i32::SHAPE));
}

#[test]
fn test_peek_rc_with_string() {
    let source = Rc::new("Hello, world!".to_string());
    let peek_value = Peek::new(&source);

    let peek_smart_pointer = peek_value.into_smart_pointer()?;

    let def = peek_smart_pointer.def();

    assert_eq!(def.pointee(), Some(String::SHAPE));
}
