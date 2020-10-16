use std::{sync::Arc, sync::Mutex};

use crate::WasmFn;
use anyhow::{Context, Result};
use wasmer::{Array, ChainableNamedResolver, Cranelift, Instance, Module, Store, WasmPtr, JIT};

pub struct Job<T> {
    pub args: LaunchArgs,
    pub ret_parser: fn(Vec<u8>) -> T,
}

macro_rules! wasm_call{
    ($instance:ident, $func_name:expr, $ty:ty) => (wasm_call!($instance, $func_name, $ty,));
    ($instance:ident, $func_name:expr, $ty:ty, $($arg:expr),*) => ({
        let func = $instance
            .exports
            .get_native_function::<$ty, _>(&$func_name)
            .with_context(|| format!("importing `{}`", &$func_name))?;
        let out = func.call($($arg),*)
            .with_context(|| format!("running `{}`", &$func_name))?;
        out
    })
}

struct Callable<'a> {
    get_in: wasmer::NativeFunc<'a, u32, WasmPtr<u8, Array>>,
    main: wasmer::NativeFunc<'a, (u32, u32), u64>,
}

impl<'a> Callable<'a> {
    fn new(instance: &'a wasmer::Instance, get_in_str: &str, main_str: &str) -> Result<Self> {
        let get_in = instance
            .exports
            .get_native_function(get_in_str)
            .with_context(|| format!("importing `{}`", get_in_str))?;
        let main = instance
            .exports
            .get_native_function(main_str)
            .with_context(|| format!("importing `{}`", main_str))?;
        Ok(Callable { get_in, main })
    }

    fn call(
        &self,
        wasm_memory: &wasmer::Memory,
        bin_arg: Vec<u8>,
        instance_count: u32,
    ) -> Result<Vec<u8>> {
        let param_len = bin_arg.len() as u32;
        let in_buffer_ptr = self.get_in.call(param_len)?;
        let memory_writer = unsafe { in_buffer_ptr.deref_mut(&wasm_memory, 0, param_len).unwrap() };
        for (from, to) in bin_arg.iter().zip(memory_writer) {
            to.set(*from);
        }

        let ret = self.main.call(param_len, instance_count)?;
        let out_buffer_ptr: WasmPtr<u8, Array> = WasmPtr::new((ret >> 32) as u32);
        let ret_len = ret as u32 as usize;

        let offset = out_buffer_ptr.offset() as usize;
        Ok(wasm_memory.view()[offset..offset + ret_len]
            .iter()
            .map(std::cell::Cell::get)
            .collect())
    }
}

pub struct LaunchArgs {
    pub fn_name: String,
    pub in_name: String,
    pub bin_arg: Vec<u8>,
    pub instance_count: u32,
}

enum Req {
    Run { id: u64, args: LaunchArgs },
    Stop,
}

enum Res {
    Result { id: u64, res: Vec<u8> },
}

pub struct Runner {
    manager: Arc<Mutex<crate::future::Manager>>,
    req_queue: smol::channel::Sender<Req>,
}

impl Runner {
    pub fn new(wasm_bin: &[u8]) -> Result<Self> {
        let engine = JIT::new(&Cranelift::default()).engine();
        let store = Store::new(&engine);
        let module = Module::new(&store, wasm_bin).context("module compilation")?;
        let (instance, memory) = get_instance(&module).context("module instanciation")?;
        let (req_queue, worker_req) = smol::channel::unbounded();
        let (worker_res, res_queue) = smol::channel::unbounded();
        for _ in 0..4 {
            let instance = instance.clone();
            let memory = memory.clone();
            let worker_req = worker_req.clone();
            let worker_res = worker_res.clone();
            std::thread::spawn(move || {
                let mut fns = std::collections::HashMap::new();
                loop {
                    match smol::block_on(worker_req.recv()) {
                        Ok(Req::Run { id, args }) => {
                            let func = match fns.entry(args.fn_name.to_string()) {
                                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                                std::collections::hash_map::Entry::Vacant(e) => e.insert(
                                    match Callable::new(&instance, &args.in_name, &args.fn_name) {
                                        Ok(r) => r,
                                        Err(e) => panic!("Callable error: {:?}", e),
                                    },
                                ),
                            };
                            let res = match func.call(&memory, args.bin_arg, args.instance_count) {
                                Ok(res) => res,
                                Err(e) => {
                                    panic!("Execution error: {:?}", e);
                                }
                            };
                            smol::block_on(worker_res.send(Res::Result { id, res })).unwrap();
                        }
                        Ok(Req::Stop) | Err(smol::channel::RecvError) => break,
                    }
                }
            });
        }
        let manager = Arc::new(Mutex::new(crate::future::Manager::new()));
        let manager2 = manager.clone();
        smol::spawn(async move {
            while let Ok(Res::Result { id, res }) = res_queue.recv().await {
                let mut manager = manager2.lock().unwrap();
                manager.wake(id, res);
            }
        })
        .detach();
        Ok(Self { manager, req_queue })
    }

    async fn run(&self, args: LaunchArgs) -> Vec<u8> {
        let id = crate::future::next_id();
        self.req_queue.send(Req::Run { id, args }).await.unwrap();
        crate::future::RunFuture::new(id, self.manager.clone()).await
    }

    pub async fn run_one<A, B>(&self, f: &WasmFn<A, B>, arg: A) -> B
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
            .await;
        B::deserialize_bin(&bin_ret).unwrap()
    }

    pub async fn map<A, B>(&self, f: &WasmFn<A, B>, args: &[A]) -> Vec<B>
    where
        A: nanoserde::SerBin,
        B: nanoserde::DeBin,
    {
        let chunk_size = 2;
        let mut futures = vec![];
        for partition in args.chunks(chunk_size) {
            let mut bin_args = vec![];
            for arg in partition {
                arg.ser_bin(&mut bin_args);
            }
            let future = self.run(LaunchArgs {
                fn_name: f.entry.to_string(),
                in_name: f.get_in.to_string(),
                bin_arg: bin_args,
                instance_count: partition.len() as u32,
            });
            futures.push(future);
        }
        let mut outs = Vec::with_capacity(args.len());
        for future in futures {
            let bin_ret = future.await;

            let mut offset = 0;
            while offset < bin_ret.len() {
                outs.push(B::de_bin(&mut offset, &bin_ret).unwrap());
            }
        }
        assert_eq!(outs.len(), args.len());
        outs
    }

    pub async fn map_reduce<A, B>(&self, f: &WasmFn<Vec<A>, B>, args: &[A]) -> Vec<B>
    where
        A: nanoserde::SerBin,
        B: nanoserde::DeBin,
    {
        let chunk_size = 2;
        let mut futures = vec![];
        for partition in args.chunks(chunk_size) {
            let mut bin_args = vec![];
            for arg in partition {
                arg.ser_bin(&mut bin_args);
            }
            let future = self.run(LaunchArgs {
                fn_name: f.entry.to_string(),
                in_name: f.get_in.to_string(),
                bin_arg: bin_args,
                instance_count: partition.len() as u32,
            });
            futures.push(future);
        }
        let mut outs = vec![];
        for future in futures {
            let bin_ret = future.await;

            let mut offset = 0;
            let out = B::de_bin(&mut offset, &bin_ret).unwrap();
            assert_eq!(offset, bin_ret.len());

            outs.push(out);
        }
        outs
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        for _ in 0..4 {
            let _ = self.req_queue.send(Req::Stop);
        }
    }
}

fn get_instance(module: &wasmer::Module) -> Result<(wasmer::Instance, wasmer::Memory)> {
    use std::cell::RefCell;
    use std::rc::Rc;

    let mem = Rc::new(RefCell::new(None));
    fn exit(memory: &mut Rc<RefCell<Option<wasmer::Memory>>>, str_ptr: u32, str_len: u32) {
        use wasmer::*;

        let str_ptr: WasmPtr<u8, Array> = WasmPtr::new(str_ptr);
        let memory = memory.borrow();
        let error_msg = str_ptr
            .get_utf8_string(memory.as_ref().unwrap(), str_len)
            .unwrap()
            .to_string();
        RuntimeError::raise(Box::new(RuntimeError::from_trap(wasmer_vm::Trap::User(
            anyhow::Error::msg(error_msg).into(),
        ))));
    }
    let mut wasi = wasmer_wasi::WasiState::new("distilled-cmd")
        .env("RUST_BACKTRACE", "1")
        .preopen(|p| p.directory("/etc").read(true))?
        .finalize()?;
    let import_object = wasi.import_object(&module)?.chain_front(wasmer::imports! {
        "env" => {
            "cust_exit" => wasmer::Function::new_native_with_env(module.store(), mem.clone(), exit)
        }
    });
    let instance = Instance::new(&module, &import_object).context("module instanciation")?;
    let wasm_memory = instance
        .exports
        .get_memory("memory")
        .expect("wasm memory")
        .clone();
    wasi.set_memory(wasm_memory.clone());
    mem.replace(Some(wasm_memory.clone()));

    let () = wasm_call!(instance, "_start", ());

    Ok((instance, wasm_memory))
}
