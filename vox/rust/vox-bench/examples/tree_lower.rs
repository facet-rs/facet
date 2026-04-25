//! Print Tree's lowered DecodeProgram so we can see CallSelf placement.
use facet::Facet;
use vox_bench::shapes::Tree;
use vox_jit::cal::{BorrowMode, CalibrationRegistry};
use vox_postcard::ir;
use vox_types::SchemaRegistry;

fn main() {
    let plan = vox_postcard::build_identity_plan(<Tree as Facet<'static>>::SHAPE);
    let registry = SchemaRegistry::new();
    let mut cal = CalibrationRegistry::new();
    cal.calibrate_string_for_type();
    cal.get_or_calibrate_by_shape(<Tree as Facet<'static>>::SHAPE);

    let program = ir::lower_with_cal(
        &plan,
        <Tree as Facet<'static>>::SHAPE,
        &registry,
        Some(&cal),
        BorrowMode::Owned,
    )
    .expect("lower");

    println!("Tree program — {} blocks", program.blocks.len());
    println!("top_shape: {:?}", program.top_shape.map(|s| s.to_string()));
    for (i, block) in program.blocks.iter().enumerate() {
        println!("  block {i}:");
        for op in &block.ops {
            println!("    {op:?}");
        }
    }
}
