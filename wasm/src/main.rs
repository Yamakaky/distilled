#![cfg_attr(target_arch = "wasm32", no_main)]

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[distilled_derive::distilled]
pub fn double(val: u32) -> u32 {
    val * val
}

#[distilled_derive::distilled]
pub fn reduce(a: u32, b: u32) -> u32 {
    a + b
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    use distilled::iter::{DistIterator, SliceExt};

    let wasm_bytes = include_bytes!("../../target/wasm32-unknown-unknown/debug/wasm.wasm");
    let mut runner = distilled::Runner::new(wasm_bytes);

    let out = vec![1, 2, 3, 5]
        .dist_iter()
        .map(double())
        .reduce(reduce())
        .run(&mut runner);
    dbg!(out);

    Ok(())
}
