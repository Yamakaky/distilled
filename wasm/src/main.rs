#![cfg_attr(target_arch = "wasm32", no_main)]

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[distilled_derive::distilled]
pub fn upper(s: String) -> String {
    s.to_ascii_uppercase()
}

#[distilled_derive::distilled]
pub fn proc_add(items: Vec<u8>) -> u8 {
    items.iter().sum::<u8>()
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    let runner = distilled::Runner::new();

    let wasm_bytes = include_bytes!("../../target/wasm32-unknown-unknown/debug/wasm.wasm");

    let job = proc_add(vec![1, 2, 3, 5]);
    let ret = runner.run(wasm_bytes.to_vec(), job.args)?;
    let result = (job.ret_parser)(ret);
    println!("Sum: {:?}", result);

    let job = upper("pote".to_string());
    let ret = runner.run(wasm_bytes.to_vec(), job.args)?;
    let result = (job.ret_parser)(ret);
    println!("Upper: {:?}", result);

    Ok(())
}
