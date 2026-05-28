#[derive(Debug)]
struct Point {
    x: u32,
    y: f64,
}

#[derive(Debug)]
#[repr(C)]
struct PointC {
    x: u32,
    y: f64,
}

fn main() {
    let p = Point { x: 42, y: 3.18 };
    let pc = PointC { x: 42, y: 3.18 };
    println!("Point: {:?}", p);
    println!("PointC: {:?}", pc);
}
