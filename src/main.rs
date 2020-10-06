use anyhow::{Context, Result};
use types::*;
use wasmer::{imports, Array, Cranelift, Instance, Module, Store, WasmPtr, JIT};

fn main() -> Result<()> {
    let runner = Runner::new();

    let wasm_bytes = include_bytes!("../target/wasm32-unknown-unknown/debug/wasm.wasm");
    let param: String = Param { a: 1, b: 2 }.serialize_json();

    let ret = runner.run(wasm_bytes.to_vec(), "add".into(), param.into_bytes())?;

    let result = {
        let mut state = types::nanoserde::DeJsonState::default();
        let mut chars = std::str::from_utf8(&ret)?.chars();
        state.next(&mut chars);
        state.next_tok(&mut chars).context("return value deser")?;
        Ret::de_json(&mut state, &mut chars)
            .context("return value deser")?
            .ret
    };
    println!("Result: {:?}", result);

    Ok(())
}

enum Req {
    Run {
        id: u64,
        wasm: Vec<u8>,
        func: String,
        params: Vec<u8>,
    },
    Stop,
}

enum Res {
    Result { id: u64, res: Vec<u8> },
}

#[derive(Debug, Clone)]
struct Runner {
    store: wasmer::Store,
    req_queue: crossbeam_channel::Sender<Req>,
    res_queue: crossbeam_channel::Receiver<Res>,
}

impl Runner {
    fn new() -> Self {
        let store = Store::new(&JIT::new(&Cranelift::default()).engine());
        let (req_queue, worker_req) = crossbeam_channel::unbounded();
        let (worker_res, res_queue) = crossbeam_channel::unbounded();
        for _ in 0..4 {
            let worker_req = worker_req.clone();
            let worker_res = worker_res.clone();
            let store = store.clone();
            std::thread::spawn(move || loop {
                match worker_req.recv() {
                    Ok(Req::Run {
                        id,
                        wasm,
                        func,
                        params,
                    }) => {
                        let _ = worker_res.send(Res::Result {
                            id,
                            res: Runner::job(&store, &wasm, &func, &params).unwrap(),
                        });
                    }
                    Ok(Req::Stop) | Err(crossbeam_channel::RecvError) => break,
                }
            });
        }
        Self {
            store,
            req_queue,
            res_queue,
        }
    }

    pub fn run(&self, wasm: Vec<u8>, func: String, params: Vec<u8>) -> Result<Vec<u8>> {
        let rid = 1;
        self.req_queue.send(Req::Run {
            id: rid,
            wasm,
            func,
            params,
        })?;
        let Res::Result { id, res } = self.res_queue.recv()?;
        assert_eq!(id, rid);
        Ok(res)
    }

    fn job(
        store: &Store,
        wasm_bytes: &[u8],
        func_name: &str,
        param_bytes: &[u8],
    ) -> Result<Vec<u8>> {
        let module = Module::new(store, wasm_bytes).context("module compilation")?;
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

impl Drop for Runner {
    fn drop(&mut self) {
        for _ in 0..4 {
            self.req_queue.send(Req::Stop).unwrap();
        }
    }
}
