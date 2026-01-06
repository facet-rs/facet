use rapace_schema::{FieldDetail, TypeDetail};
use rapace_service_macros::service;

#[derive(facet::Facet)]
struct Point {
    x: u32,
    y: u32,
}

#[service]
#[allow(dead_code)]
trait Geometry {
    async fn compute(&self, point: Point, data: Vec<u8>, maybe: Option<u32>) -> (u32, Point);
}

#[test]
fn service_detail_uses_facet_reflection() {
    let svc = geometry_service_detail();
    assert_eq!(svc.name, "Geometry");
    assert_eq!(svc.methods.len(), 1);

    let method = &svc.methods[0];
    assert_eq!(method.method_name, "compute");
    assert_eq!(method.args.len(), 3);

    assert_eq!(
        method.args[0].type_info,
        TypeDetail::Struct {
            fields: vec![
                FieldDetail {
                    name: "x".to_string(),
                    type_info: TypeDetail::U32,
                },
                FieldDetail {
                    name: "y".to_string(),
                    type_info: TypeDetail::U32,
                },
            ],
        }
    );
    assert_eq!(method.args[1].type_info, TypeDetail::Bytes);
    assert_eq!(
        method.args[2].type_info,
        TypeDetail::Option(Box::new(TypeDetail::U32))
    );

    assert_eq!(
        method.return_type,
        TypeDetail::Tuple(vec![
            TypeDetail::U32,
            TypeDetail::Struct {
                fields: vec![
                    FieldDetail {
                        name: "x".to_string(),
                        type_info: TypeDetail::U32,
                    },
                    FieldDetail {
                        name: "y".to_string(),
                        type_info: TypeDetail::U32,
                    },
                ],
            },
        ])
    );
}
