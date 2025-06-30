#![expect(clippy::approx_constant)]

use divan::{Bencher, black_box};
use facet::Facet;
use facet_reflect::{Partial, ReflectError};

#[derive(Facet)]
struct SimpleStruct {
    a: u32,
    b: i32,
    c: f32,
}

#[divan::bench(name = "Populate small struct by field names")]
fn populate_simple_struct(bencher: Bencher) {
    fn populate_by_field_name<'shape>(
        partial: &mut Partial<'_, 'shape>,
    ) -> Result<(), ReflectError<'shape>> {
        partial.set_field("a", 42u32)?;
        partial.set_field("b", -42i32)?;
        partial.set_field("c", 3.14f32)?;
        Ok(())
    }

    bencher.bench(|| {
        let mut partial = Partial::alloc::<SimpleStruct>().unwrap();
        populate_by_field_name(black_box(partial.inner_mut())).unwrap();
        // Don't materialize it - we just want to measure the population time.
    })
}

#[divan::bench(name = "Populate small struct by field index")]
fn populate_simple_struct_by_field_index(bencher: Bencher) {
    fn populate_by_field_index<'shape>(
        partial: &mut Partial<'_, 'shape>,
    ) -> Result<(), ReflectError<'shape>> {
        partial.set_nth_field(0, 42u32)?;
        partial.set_nth_field(1, -42i32)?;
        partial.set_nth_field(2, 3.14f32)?;
        Ok(())
    }

    bencher.bench(|| {
        let mut partial = Partial::alloc::<SimpleStruct>().unwrap();
        populate_by_field_index(black_box(partial.inner_mut())).unwrap();
        // Don't materialize it - we just want to measure the population time.
    })
}

fn main() {
    divan::main();
}
