use facet_poke::{Peek, PeekValue};
use log::trace;
use std::collections::VecDeque;
use std::io::{self, Write};

fn peek_value_to_json<W: Write>(pv: PeekValue, writer: &mut W) -> io::Result<()> {
    if pv.shape().is_type::<()>() {
        write!(writer, "null")?;
    } else if pv.shape().is_type::<bool>() {
        let value = unsafe { pv.data().as_ref::<bool>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<u8>() {
        let value = unsafe { pv.data().as_ref::<u8>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<u16>() {
        let value = unsafe { pv.data().as_ref::<u16>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<u32>() {
        let value = unsafe { pv.data().as_ref::<u32>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<u64>() {
        let value = unsafe { pv.data().as_ref::<u64>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<u128>() {
        let value = unsafe { pv.data().as_ref::<u128>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<i8>() {
        let value = unsafe { pv.data().as_ref::<i8>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<i16>() {
        let value = unsafe { pv.data().as_ref::<i16>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<i32>() {
        let value = unsafe { pv.data().as_ref::<i32>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<i64>() {
        let value = unsafe { pv.data().as_ref::<i64>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<i128>() {
        let value = unsafe { pv.data().as_ref::<i128>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<f32>() {
        let value = unsafe { pv.data().as_ref::<f32>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<f64>() {
        let value = unsafe { pv.data().as_ref::<f64>() };
        write!(writer, "{}", value)?;
    } else if pv.shape().is_type::<String>() {
        let value = unsafe { pv.data().as_ref::<String>() };
        write!(writer, "\"{}\"", value.escape_debug())?;
    } else {
        write!(writer, "\"<unsupported type>\"")?;
    }
    Ok(())
}

/// Serializes any Facet type to JSON
pub fn to_json<W: Write>(peek: Peek<'_>, writer: &mut W, indent: bool) -> io::Result<()> {
    #[derive(Debug)]
    enum StackItem<'mem> {
        Value {
            peek: Peek<'mem>,
            level: usize,
        },
        StructField {
            field_name: String,
            peek: Peek<'mem>,
            level: usize,
            is_first: bool,
        },
        StructEnd {
            level: usize,
            had_fields: bool,
        },
        ListItem {
            peek: Peek<'mem>,
            level: usize,
            is_first: bool,
        },
        ListEnd {
            level: usize,
            had_items: bool,
        },
        MapEntry {
            key: Peek<'mem>,
            value: Peek<'mem>,
            level: usize,
            is_first: bool,
        },
        MapEnd {
            level: usize,
            had_entries: bool,
        },
    }

    let mut stack: VecDeque<StackItem> = VecDeque::new();
    stack.push_back(StackItem::Value { peek, level: 0 });

    while let Some(item) = stack.pop_front() {
        match item {
            StackItem::Value { peek, level } => {
                match peek {
                    Peek::Value(pv) => {
                        peek_value_to_json(pv, writer)?;
                    }
                    Peek::Struct(ps) => {
                        write!(writer, "{{")?;
                        if indent {
                            writeln!(writer)?;
                        }

                        let fields: Vec<_> = ps.fields().collect();
                        stack.push_front(StackItem::StructEnd {
                            level,
                            had_fields: !fields.is_empty(),
                        });

                        // Push fields in reverse order so they'll be processed in the correct order
                        for (i, field) in fields.into_iter().enumerate().rev() {
                            stack.push_front(StackItem::StructField {
                                field_name: field.0.to_string(),
                                peek: field.1,
                                level,
                                is_first: i == 0,
                            });
                        }
                    }
                    Peek::List(pl) => {
                        write!(writer, "[")?;
                        if indent {
                            writeln!(writer)?;
                        }

                        let mut items = Vec::new();
                        let mut index = 0;
                        while let Some(item) = pl.item_at(index) {
                            items.push(item);
                            index += 1;
                        }

                        stack.push_front(StackItem::ListEnd {
                            level,
                            had_items: !items.is_empty(),
                        });

                        // Push items in reverse order
                        for (i, item) in items.into_iter().enumerate().rev() {
                            stack.push_front(StackItem::ListItem {
                                peek: item,
                                level,
                                is_first: i == 0,
                            });
                        }
                    }
                    Peek::Map(pm) => {
                        write!(writer, "{{")?;
                        if indent {
                            writeln!(writer)?;
                        }

                        // Collect entries using the iterator and convert them to the format expected by MapEntry
                        let entries: Vec<(PeekValue<'_>, Peek<'_>)> = pm
                            .iter()
                            .map(|(key, value)| (key.as_value(), value))
                            .collect();

                        stack.push_front(StackItem::MapEnd {
                            level,
                            had_entries: !entries.is_empty(),
                        });

                        // Push entries in reverse order
                        for (i, (key, value)) in entries.into_iter().enumerate().rev() {
                            stack.push_front(StackItem::MapEntry {
                                key: Peek::Value(key),
                                value,
                                level,
                                is_first: i == 0,
                            });
                        }
                    }
                    _ => todo!("unsupported peek type: {:?}", peek),
                }
            }
            StackItem::StructField {
                field_name,
                peek,
                level,
                is_first,
            } => {
                if !is_first {
                    write!(writer, ",")?;
                    if indent {
                        writeln!(writer)?;
                    }
                }

                if indent {
                    write!(writer, "{:indent$}", "", indent = (level + 1) * 2)?;
                }
                write!(writer, "\"{}\":", field_name)?;
                if indent {
                    write!(writer, " ")?;
                }

                stack.push_front(StackItem::Value {
                    peek,
                    level: level + 1,
                });
            }
            StackItem::StructEnd { level, had_fields } => {
                if had_fields && indent {
                    writeln!(writer)?;
                    write!(writer, "{:indent$}", "", indent = level * 2)?;
                }
                write!(writer, "}}")?;
            }
            StackItem::ListItem {
                peek,
                level,
                is_first,
            } => {
                if !is_first {
                    write!(writer, ",")?;
                    if indent {
                        writeln!(writer)?;
                    }
                }

                if indent {
                    write!(writer, "{:indent$}", "", indent = (level + 1) * 2)?;
                }

                stack.push_front(StackItem::Value {
                    peek,
                    level: level + 1,
                });
            }
            StackItem::ListEnd { level, had_items } => {
                if had_items && indent {
                    writeln!(writer)?;
                    write!(writer, "{:indent$}", "", indent = level * 2)?;
                }
                write!(writer, "]")?;
            }
            StackItem::MapEntry {
                key,
                value,
                level,
                is_first,
            } => {
                if !is_first {
                    write!(writer, ",")?;
                    if indent {
                        writeln!(writer)?;
                    }
                }

                if indent {
                    write!(writer, "{:indent$}", "", indent = (level + 1) * 2)?;
                }

                // Process key first (inline with no indentation)
                let mut temp_writer = Vec::new();
                let mut temp_stack = VecDeque::new();
                temp_stack.push_back(StackItem::Value {
                    peek: key,
                    level: 0,
                });

                while let Some(temp_item) = temp_stack.pop_front() {
                    match temp_item {
                        StackItem::Value { peek, level: _ } => match peek {
                            Peek::Value(pv) => {
                                trace!("{:?}", pv.shape());
                                peek_value_to_json(pv, &mut temp_writer)?;
                            }
                            _ => {
                                write!(&mut temp_writer, "\"<complex_key>\"")?;
                            }
                        },
                        _ => {
                            // This should not happen for simple key serialization
                            write!(&mut temp_writer, "\"<invalid_key>\"")?;
                        }
                    }
                }

                let key_string = String::from_utf8(temp_writer).unwrap();
                write!(writer, "{}:", key_string)?;

                if indent {
                    write!(writer, " ")?;
                }

                stack.push_front(StackItem::Value {
                    peek: value,
                    level: level + 1,
                });
            }
            StackItem::MapEnd { level, had_entries } => {
                if had_entries && indent {
                    writeln!(writer)?;
                    write!(writer, "{:indent$}", "", indent = level * 2)?;
                }
                write!(writer, "}}")?;
            }
        }
    }

    Ok(())
}

/// Serializes any Facet type to JSON and returns it as a String
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use facet_json_write::to_json_string;
/// use facet_peek::Peek;
///
/// #[derive(facet::Facet)]
/// struct Foo {
///   bar: String,
///   baz: u32,
/// }
///
/// let foo = Foo {
///   bar: "Hello, World!".to_string(),
///   baz: 42,
/// };
/// let foo = Peek::new(&foo);
///
/// println!("{}", to_json_string(foo, true));
/// ```
pub fn to_json_string(peek: Peek<'_>, indent: bool) -> String {
    let mut buffer = Vec::new();
    to_json(peek, &mut buffer, indent).unwrap();
    String::from_utf8(buffer).unwrap()
}
