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

#[derive(Debug, Clone)]
pub struct Runner {
    req_queue: crossbeam_channel::Sender<Req>,
    res_queue: crossbeam_channel::Receiver<Res>,
}

impl Runner {
    pub fn new(wasm_bin: &[u8]) -> Result<Self> {
        let engine = JIT::new(&Cranelift::default()).engine();
        let store = Store::new(&engine);
        let module = Module::new(&store, wasm_bin).context("module compilation")?;
        let (instance, memory) = get_instance(&module).context("module instanciation")?;
        let (req_queue, worker_req) = crossbeam_channel::unbounded();
        let (worker_res, res_queue) = crossbeam_channel::unbounded();
        for _ in 0..1 {
            let instance = instance.clone();
            let memory = memory.clone();
            let worker_req = worker_req.clone();
            let worker_res = worker_res.clone();
            std::thread::spawn(move || {
                let mut fns = std::collections::HashMap::new();
                loop {
                    match worker_req.recv() {
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
                            let _ = worker_res.send(Res::Result { id, res });
                        }
                        Ok(Req::Stop) | Err(crossbeam_channel::RecvError) => break,
                    }
                }
            });
        }
        Ok(Self {
            req_queue,
            res_queue,
        })
    }

    fn run(&self, args: LaunchArgs) -> Result<Vec<u8>> {
        let rid = 1;
        self.req_queue.send(Req::Run { id: rid, args })?;
        let Res::Result { id, res } = self.res_queue.recv()?;
        assert_eq!(id, rid);
        Ok(res)
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
