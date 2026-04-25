//! Lower the borrowed mirror types and report which sub-shapes lower to
//! `SlowPath` (i.e. fall back to the reflective deserializer, which currently
//! fails for `&'a str` because the slow-path entry hardcodes `BORROW=false`).
//!
//! Run:
//!     cargo run --release --example lower_borrowed
use facet::Facet;
use vox_bench::borrowed;
use vox_jit::cal::{BorrowMode, CalibrationRegistry};
use vox_postcard::ir;
use vox_types::SchemaRegistry;

fn dump_slow_paths(label: &str, shape: &'static facet::Shape) {
    let plan = vox_postcard::build_identity_plan(shape);
    let registry = SchemaRegistry::new();
    let mut cal = CalibrationRegistry::new();
    cal.calibrate_string_for_type();

    fn register_tree(shape: &'static facet::Shape, cal: &mut CalibrationRegistry) {
        match shape.def {
            facet::Def::List(_) => {
                cal.get_or_calibrate_by_shape(shape);
            }
            _ => {}
        }
        match shape.ty {
            facet::Type::User(facet::UserType::Struct(st)) => {
                for f in st.fields {
                    register_tree(f.shape(), cal);
                }
            }
            facet::Type::User(facet::UserType::Enum(et)) => {
                for v in et.variants {
                    for f in v.data.fields {
                        register_tree(f.shape(), cal);
                    }
                }
            }
            _ => {}
        }
        match shape.def {
            facet::Def::Option(o) => register_tree(o.t, cal),
            facet::Def::Result(r) => {
                register_tree(r.t, cal);
                register_tree(r.e, cal);
            }
            facet::Def::List(l) => register_tree(l.t, cal),
            facet::Def::Pointer(p) => {
                if let Some(inner) = p.pointee() {
                    register_tree(inner, cal);
                }
            }
            facet::Def::Array(a) => register_tree(a.t, cal),
            _ => {}
        }
    }
    register_tree(shape, &mut cal);

    let program = ir::lower_with_cal(&plan, shape, &registry, Some(&cal), BorrowMode::Borrowed)
        .expect("lower");

    println!("\n{label}: {} blocks", program.blocks.len());
    let mut total_slow = 0;
    for (block_id, block) in program.blocks.iter().enumerate() {
        for op in &block.ops {
            if let ir::DecodeOp::SlowPath { shape, dst_offset, .. } = op {
                total_slow += 1;
                println!(
                    "  SlowPath in block {block_id}: shape={shape}, dst_offset={dst_offset}"
                );
            }
        }
    }
    if total_slow == 0 {
        println!("  (no SlowPath ops)");
    } else {
        println!("  total SlowPath ops: {total_slow}");
    }
}

fn main() {
    dump_slow_paths(
        "(GnarlyPayload,) [owned]",
        <(spec_proto::GnarlyPayload,) as Facet<'static>>::SHAPE,
    );
    dump_slow_paths(
        "(GnarlyPayload,) [borrowed]",
        <(borrowed::GnarlyPayload<'static>,) as Facet<'static>>::SHAPE,
    );
}
