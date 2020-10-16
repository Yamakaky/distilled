#[cfg(target_arch = "wasm32")]
fn as_u16(x: u8) -> u16 {
    x as u16
}

#[cfg(target_arch = "wasm32")]
fn double(x: u16) -> u16 {
    x * 2
}

#[cfg(target_arch = "wasm32")]
fn as_u32(x: u16) -> u32 {
    x as u32
}

fn sum(acc: u32, val: u32) -> u32 {
    acc + val
}

fn concat(mut acc: String, val: String) -> String {
    acc.push_str(&val);
    acc
}

distilled::pipeline!(map_reduce = u8 | as_u16 | as_u32 |> sum: u32);
distilled::pipeline!(concat_str = String |> concat: String);
distilled::pipeline_map!(map = u8 | as_u16 | double | as_u32: u32);

distilled::setup_runtime!();

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    smol::block_on(async {
        //let wasm_bytes = include_bytes!("../../target/wasm32-unknown-unknown/debug/wasm.wasm");
        let wasm_bytes = include_bytes!("../../target/wasm32-wasi/debug/wasm.wasm");
        let runner = distilled::Runner::new(wasm_bytes)?;

        dbg!(runner.map_reduce(&map_reduce, 0, &[1, 2, 3, 5]).await?);
        dbg!(
            runner
                .map_reduce(
                    &concat_str,
                    "".to_string(),
                    &["a".to_string(), "b".to_string(), "c".to_string()]
                )
                .await?
        );
        dbg!(
            runner
                .map(&map, &vec![1, 2, 3, 5, 1, 2, 3, 5, 1, 2, 3, 5, 1, 2, 3, 5],)
                .await?
        );

        Ok(())
    })
}
