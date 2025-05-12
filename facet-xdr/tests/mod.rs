use facet::Facet;
use facet_xdr::{deserialize, to_vec};

const FILE_EXAMPLE_BYTES: [u8; 48] = [
    0x00, 0x00, 0x00, 0x09, 0x73, 0x69, 0x6c, 0x6c, 0x79, 0x70, 0x72, 0x6f, 0x67, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x6c, 0x69, 0x73, 0x70, 0x00, 0x00, 0x00, 0x04,
    0x6a, 0x6f, 0x68, 0x6e, 0x00, 0x00, 0x00, 0x06, 0x28, 0x71, 0x75, 0x69, 0x74, 0x29, 0x00, 0x00,
];

fn file_example() -> File {
    File {
        filename: "sillyprog".to_owned(),
        filetype: FileType::Exec {
            interpretor: "lisp".to_owned(),
        },
        owner: "john".to_owned(),
        data: vec![b'(', b'q', b'u', b'i', b't', b')'],
    }
}

#[allow(unused)]
#[derive(Debug, Facet, PartialEq)]
#[repr(u32)]
enum FileType {
    Text,
    Data { creator: String },
    Exec { interpretor: String },
}

#[derive(Debug, Facet, PartialEq)]
struct File {
    filename: String,
    filetype: FileType,
    owner: String,
    data: Vec<u8>,
}

#[test]
fn test_serialize_file_example() {
    let file_bytes = to_vec(&file_example()).unwrap();
    assert_eq!(&file_bytes[..], FILE_EXAMPLE_BYTES);
}

#[test]
fn test_deserialize_file_example() {
    let file: File = deserialize(&FILE_EXAMPLE_BYTES).unwrap();
    assert_eq!(file, file_example());
}
