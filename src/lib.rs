#[cfg(not(target_arch = "wasm32"))]
mod future;
#[cfg(not(target_arch = "wasm32"))]
mod host;
#[cfg(not(target_arch = "wasm32"))]
mod iter;

#[cfg(not(target_arch = "wasm32"))]
mod inner {
    pub use super::future::*;
    pub use super::host::*;
    pub use super::iter::*;
}

#[cfg(target_arch = "wasm32")]
mod guest;

#[cfg(target_arch = "wasm32")]
mod inner {
    pub use super::guest::*;

    pub static mut IN_BUFFER: Vec<u8> = Vec::new();
    pub static mut OUT_BUFFER: Vec<u8> = Vec::new();

    #[no_mangle]
    pub unsafe fn get_in(size: u32) -> *const u8 {
        IN_BUFFER.clear();
        IN_BUFFER.reserve(size as usize);
        IN_BUFFER.set_len(size as usize);
        IN_BUFFER.as_ptr()
    }
}
#[macro_export]
macro_rules! setup_runtime {
    () => {
        #[cfg(target_arch = "wasm32")]
        extern "C" {
            fn cust_exit(str_ptr: u32, str_len: u32);
        }

        #[cfg(target_arch = "wasm32")]
        fn main() {
            std::panic::set_hook(Box::new(|panic_info| {
                use std::fmt::Write;

                let mut out = "WASM code panicked".to_string();
                let payload = panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(|x| x.clone())
                    .or_else(|| {
                        panic_info
                            .payload()
                            .downcast_ref::<&str>()
                            .map(|x| x.to_string())
                    });
                match (payload, panic_info.location()) {
                    (Some(info), Some(location)) => write!(out, ": {}, {}", info, location),
                    (Some(info), None) => write!(out, ": {}", info),
                    (None, Some(location)) => write!(out, " at {}", location),
                    (None, None) => write!(out, " (no info)"),
                }
                .expect("write to string");
                unsafe {
                    cust_exit(out.as_ptr() as u32, out.len() as u32);
                }
            }));
        }

        #[cfg(target_arch = "wasm32")]
        #[global_allocator]
        static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
    };
}

pub use inner::*;

pub struct Raw<X> {
    pub slice: &'static [u8],
    pub idx: usize,
    pub instance_count: u32,
    pub _phantom: std::marker::PhantomData<X>,
}

impl<X: nanoserde::DeBin> Iterator for Raw<X> {
    type Item = X;

    fn next(&mut self) -> Option<Self::Item> {
        if self.instance_count == 0 {
            assert_eq!(self.slice.len(), self.idx);
            None
        } else {
            self.instance_count -= 1;
            Some(nanoserde::DeBin::de_bin(&mut self.idx, &self.slice).unwrap())
        }
    }
}

#[macro_export]
macro_rules! pipeline {
    ($name:ident = $in_ty:ty | $($map:ident)|* |> $reduce:ident: $out_ty:ty) => (
        #[cfg(not(target_arch = "wasm32"))]
        #[allow(non_upper_case_globals)]
        const $name: ::distilled::WasmFn<Vec<$in_ty>, $out_ty> = ::distilled::WasmFn {
            entry: stringify!($name),
            get_in: "get_in",
            _phantom: ::std::marker::PhantomData,
        };

        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub unsafe fn $name(in_buffer_len: u32, instance_count: u32) -> u64 {
            use ::nanoserde::SerBin;
            fn inner(vals: impl Iterator<Item=$in_ty>) -> $out_ty {
                vals.fold(0, |acc, val| $reduce(acc, ::distilled::call_chain!(val, $($map),*)))
            }

            let ret = inner(::distilled::Raw{
                slice: &::distilled::IN_BUFFER[..in_buffer_len as usize],
                idx:0,
                instance_count,
                _phantom: std::marker::PhantomData,
            });
            ::distilled::OUT_BUFFER.clear();
            ret.ser_bin(&mut ::distilled::OUT_BUFFER);
            ((::distilled::OUT_BUFFER.as_ptr() as u64) << 32 | ::distilled::OUT_BUFFER.len() as u64)
        }
    )
}

#[macro_export]
macro_rules! pipeline_map {
    ($name:ident = $in_ty:ty | $($map:ident)|* : $out_ty:ty) => (
        #[cfg(not(target_arch = "wasm32"))]
        #[allow(non_upper_case_globals)]
        const $name: ::distilled::WasmFn<$in_ty, $out_ty> = ::distilled::WasmFn {
            entry: stringify!($name),
            get_in: "get_in",
            _phantom: ::std::marker::PhantomData,
        };

        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub unsafe fn $name(in_buffer_len: u32, instance_count: u32) -> u64 {
            use ::nanoserde::SerBin;

            let ret_iter = ::distilled::Raw{
                slice: &::distilled::IN_BUFFER[..in_buffer_len as usize],
                idx:0,
                instance_count,
                _phantom: std::marker::PhantomData,
            }.map(|val| ::distilled::call_chain!(val, $($map),*));
            ::distilled::OUT_BUFFER.clear();
            for x in ret_iter {
                x.ser_bin(&mut ::distilled::OUT_BUFFER);
            }
            ((::distilled::OUT_BUFFER.as_ptr() as u64) << 32 | ::distilled::OUT_BUFFER.len() as u64)
        }
    )
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! call_chain {
    ($param:tt, $first:ident, $($then:ident),+) => ({
        let x = $first($param);
        ::distilled::call_chain!(x, $($then),*)
    });
    ($param:tt, $first:ident) => ($first($param));
}
