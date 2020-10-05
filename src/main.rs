use anyhow::{Context, Result};
use types::*;
use wasmer::{imports, Array, Instance, Module, NativeFunc, Store, WasmPtr};
use wasmer_compiler_cranelift::Cranelift;
use wasmer_engine_jit::JIT;

fn main() -> Result<()> {
    let store = Store::new(&JIT::new(&Cranelift::default()).engine());
    let wasm_bytes = include_bytes!("../target/wasm32-unknown-unknown/debug/wasm.wasm");
    let module = Module::new(&store, &wasm_bytes[..]).expect("create module");
    let import_object = imports! {};
    let instance = Instance::new(&module, &import_object).expect("instantiate module");
    let wasm_memory = instance.exports.get_memory("memory").expect("wasm memory");

    let get_in_buffer: NativeFunc<(), WasmPtr<u8, Array>> = instance
        .exports
        .get_native_function("get_in_buffer")
        .expect("get_wasm_memory_buffer_pointer");
    let in_buffer_ptr = get_in_buffer.call().unwrap();
    let param: String = Param { a: 1, b: 2 }.serialize_json();
    let param_len = param.len() as u32;
    let memory_writer = unsafe { in_buffer_ptr.deref_mut(wasm_memory, 0, param_len).unwrap() };
    for (from, to) in param.bytes().zip(memory_writer) {
        to.set(from);
    }

    let add = instance
        .exports
        .get_native_function::<u32, u32>("add")
        .expect("add function in Wasm module");
    let ret_len = add.call(param_len)?;

    let get_out_buffer = instance
        .exports
        .get_native_function::<(), WasmPtr<u8, Array>>("get_out_buffer")
        .expect("get_wasm_memory_buffer_pointer");
    let out_buffer_ptr = get_out_buffer.call().unwrap();
    let ret_str = out_buffer_ptr
        .get_utf8_string(&wasm_memory, ret_len)
        .context("bad str value")?;
    let result = {
        let mut state = types::nanoserde::DeJsonState::default();
        let mut chars = ret_str.chars();
        state.next(&mut chars);
        state.next_tok(&mut chars).context("return value deser")?;
        Ret::de_json(&mut state, &mut chars).context("return value deser")?
    };
    println!("Result: {:?}", result);

    Ok(())
}
