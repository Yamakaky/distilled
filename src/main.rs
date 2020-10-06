use anyhow::{Context, Result};
use types::*;
use wasmer::{imports, Array, Cranelift, Instance, Module, Store, WasmPtr, JIT};

fn main() -> Result<()> {
    let runner = Runner::new();

    let wasm_bytes = include_bytes!("../target/wasm32-unknown-unknown/debug/wasm.wasm");
    let param: String = Param { a: 1, b: 2 }.serialize_json();

    let ret = runner.run(wasm_bytes, "add", param.as_bytes())?;

    let result = {
        let mut state = types::nanoserde::DeJsonState::default();
        let mut chars = std::str::from_utf8(&ret)?.chars();
        state.next(&mut chars);
        state.next_tok(&mut chars).context("return value deser")?;
        Ret::de_json(&mut state, &mut chars).context("return value deser")?.ret
    };
    println!("Result: {:?}", result);

    Ok(())
}

#[derive(Debug, Clone)]
struct Runner {
    store: wasmer::Store,
}

impl Runner {
    fn new() -> Self {
        Self {
            store: Store::new(&JIT::new(&Cranelift::default()).engine()),
        }
    }

    fn run(&self, wasm_bytes: &[u8], func_name: &str, param_bytes: &[u8]) -> Result<Vec<u8>> {
        let module = Module::new(&self.store, wasm_bytes).context("module compilation")?;
        let import_object = imports! {};
        let instance = Instance::new(&module, &import_object).context("module instanciation")?;
        let wasm_memory = instance.exports.get_memory("memory").expect("wasm memory");

        let get_in_buffer = instance
            .exports
            .get_native_function::<(), WasmPtr<u8, Array>>("get_in_buffer")
            .expect("get_wasm_memory_buffer_pointer");
        let func = instance
            .exports
            .get_native_function::<u32, u32>(func_name)
            .expect("add function in Wasm module");
        let get_out_buffer = instance
            .exports
            .get_native_function::<(), WasmPtr<u8, Array>>("get_out_buffer")
            .expect("get_wasm_memory_buffer_pointer");

        let in_buffer_ptr = get_in_buffer.call().unwrap();
        let param_len = param_bytes.len() as u32;
        let memory_writer = unsafe { in_buffer_ptr.deref_mut(wasm_memory, 0, param_len).unwrap() };
        for (from, to) in param_bytes.iter().zip(memory_writer) {
            to.set(*from);
        }

        let ret_len = func.call(param_len)? as usize;

        let out_buffer_ptr = get_out_buffer.call().unwrap();
        let offset = out_buffer_ptr.offset() as usize;
        Ok(wasm_memory.view()[offset..offset + ret_len]
            .iter()
            .map(std::cell::Cell::get)
            .collect())
    }
}
