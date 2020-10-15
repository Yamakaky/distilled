pub mod iter;

use anyhow::{Context, Result};
use iter::WasmFn;
use std::sync::Arc;
use wasmer::{Array, Cranelift, Instance, Module, Store, WasmPtr, JIT};

pub struct Job<T> {
    pub args: LaunchArgs,
    pub ret_parser: fn(Vec<u8>) -> T,
}

pub struct LaunchArgs {
    pub fn_name: String,
    pub in_name: String,
    pub bin_arg: Vec<u8>,
    pub instance_count: u32,
}

enum Req {
    Run {
        id: u64,
        module: Arc<wasmer::Module>,
        args: LaunchArgs,
    },
    Stop,
}

enum Res {
    Result { id: u64, res: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct Runner {
    store: wasmer::Store,
    wasm: Arc<wasmer::Module>,
    req_queue: crossbeam_channel::Sender<Req>,
    res_queue: crossbeam_channel::Receiver<Res>,
}

impl Runner {
    pub fn new(wasm_bin: &[u8]) -> Self {
        let store = Store::new(&JIT::new(&Cranelift::default()).engine());
        let (req_queue, worker_req) = crossbeam_channel::unbounded();
        let (worker_res, res_queue) = crossbeam_channel::unbounded();
        for _ in 0..4 {
            let worker_req = worker_req.clone();
            let worker_res = worker_res.clone();
            std::thread::spawn(move || loop {
                match worker_req.recv() {
                    Ok(Req::Run { id, module, args }) => {
                        let res = match Runner::job(module, args) {
                            Ok(res) => res,
                            Err(e) => {
                                eprintln!("Execution error: {:?}", e);
                                panic!();
                            }
                        };
                        let _ = worker_res.send(Res::Result { id, res });
                    }
                    Ok(Req::Stop) | Err(crossbeam_channel::RecvError) => break,
                }
            });
        }
        let wasm = Arc::new(
            Module::new(&store, wasm_bin)
                .context("module compilation")
                .unwrap(),
        );
        Self {
            store,
            wasm,
            req_queue,
            res_queue,
        }
    }

    fn run(&self, args: LaunchArgs) -> Result<Vec<u8>> {
        let rid = 1;
        self.req_queue.send(Req::Run {
            id: rid,
            module: self.wasm.clone(),
            args,
        })?;
        let Res::Result { id, res } = self.res_queue.recv()?;
        assert_eq!(id, rid);
        Ok(res)
    }

    fn job(module: Arc<wasmer::Module>, args: LaunchArgs) -> Result<Vec<u8>> {
        let mut wasi = wasmer_wasi::WasiState::new("distilled-cmd")
            .env("RUST_BACKTRACE", "1")
            .preopen(|p| p.directory("/etc").read(true))?
            .finalize()?;
        let import_object = wasi.import_object(&module)?;
        let instance = Instance::new(&module, &import_object).context("module instanciation")?;
        let wasm_memory = instance.exports.get_memory("memory").expect("wasm memory");
        wasi.set_memory(wasm_memory.clone());

        let start = instance.exports.get_function("_start")?;
        start.call(&[]).context("execute _start")?;

        let get_in_buffer = instance
            .exports
            .get_native_function::<u32, WasmPtr<u8, Array>>(&args.in_name)
            .expect("get_wasm_memory_buffer_pointer");
        let func = instance
            .exports
            .get_native_function::<(u32, u32), u64>(&args.fn_name)
            .expect("add function in Wasm module");

        let in_buffer_ptr = get_in_buffer.call(args.bin_arg.len() as u32).unwrap();
        let param_len = args.bin_arg.len() as u32;
        let memory_writer = unsafe { in_buffer_ptr.deref_mut(wasm_memory, 0, param_len).unwrap() };
        for (from, to) in args.bin_arg.iter().zip(memory_writer) {
            to.set(*from);
        }

        let ret_slice = func
            .call(param_len, args.instance_count)
            .context("execute operation")?;
        let ret_ptr = (ret_slice >> 32) as usize;
        let ret_len = ret_slice as u32 as usize;

        Ok(wasm_memory.view()[ret_ptr..ret_ptr + ret_len]
            .iter()
            .map(std::cell::Cell::get)
            .collect())
    }

    pub fn run_one<A, B>(&self, f: &WasmFn<A, B>, arg: A) -> B
    where
        A: nanoserde::SerBin,
        B: nanoserde::DeBin,
    {
        let bin_arg = arg.serialize_bin();
        let bin_ret = self
            .run(LaunchArgs {
                fn_name: f.entry.to_string(),
                in_name: f.get_in.to_string(),
                bin_arg,
                instance_count: 1,
            })
            .unwrap();
        B::deserialize_bin(&bin_ret).unwrap()
    }

    pub fn map<A, B>(&self, f: &WasmFn<A, B>, args: &[A]) -> Vec<B>
    where
        A: nanoserde::SerBin,
        B: nanoserde::DeBin,
    {
        let mut bin_args = vec![];
        for arg in args {
            arg.ser_bin(&mut bin_args);
        }
        let bin_ret = self
            .run(LaunchArgs {
                fn_name: f.entry.to_string(),
                in_name: f.get_in.to_string(),
                bin_arg: bin_args,
                instance_count: args.len() as u32,
            })
            .unwrap();
        let mut offset = 0;
        (0..args.len())
            .map(|_| B::de_bin(&mut offset, &bin_ret).unwrap())
            .collect()
    }

    pub fn map_reduce<A, B>(&self, f: &WasmFn<Vec<A>, B>, args: &[A]) -> B
    where
        A: nanoserde::SerBin,
        B: nanoserde::DeBin,
    {
        let mut bin_args = vec![];
        for arg in args {
            arg.ser_bin(&mut bin_args);
        }
        let bin_ret = self
            .run(LaunchArgs {
                fn_name: f.entry.to_string(),
                in_name: f.get_in.to_string(),
                bin_arg: bin_args,
                instance_count: args.len() as u32,
            })
            .unwrap();
        let mut offset = 0;
        let out = B::de_bin(&mut offset, &bin_ret).unwrap();
        assert_eq!(offset, bin_ret.len());
        out
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        for _ in 0..4 {
            self.req_queue.send(Req::Stop).unwrap();
        }
    }
}
