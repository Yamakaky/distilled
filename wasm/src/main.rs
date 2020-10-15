#[cfg(target_arch = "wasm32")]
fn truc(x: u8) -> u16 {
    x as u16
}

#[cfg(target_arch = "wasm32")]
fn pote(x: u16) -> u32 {
    x as u32
}

#[cfg(target_arch = "wasm32")]
fn red(acc: u32, val: u32) -> u32 {
    acc + val
}

distilled::pipeline!(things = u8 | truc | pote |> red: u32);

distilled::setup_runtime!();

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    use distilled::SliceExt;

    //let wasm_bytes = include_bytes!("../../target/wasm32-unknown-unknown/debug/wasm.wasm");
    let wasm_bytes = include_bytes!("../../target/wasm32-wasi/debug/wasm.wasm");
    let mut runner = distilled::Runner::new(wasm_bytes)?;

    let out = vec![1, 2, 3, 5].map_reduce(things, &mut runner);
    dbg!(out);

    Ok(())
}
