#![cfg_attr(target_arch = "wasm32", no_main)]

#[distilled_derive::distilled]
pub fn proc_add(items: Vec<u8>) -> u8 {
    items.iter().sum::<u8>()
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    let runner = distilled::Runner::new();

    let wasm_bytes = include_bytes!("../../target/wasm32-unknown-unknown/debug/wasm.wasm");
    let job = proc_add(vec![1, 2, 3, 5]);

    let ret = runner.run(
        wasm_bytes.to_vec(),
        job.fn_name,
        job.in_name,
        job.out_name,
        job.bin_arg,
    )?;

    let result = (job.ret_parser)(ret);
    println!("Result: {:?}", result);

    Ok(())
}
