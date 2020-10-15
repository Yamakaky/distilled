#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

//#[distilled_derive::distilled]
//fn double(val: u32) -> u32 {
//    val * val
//}
//
//#[distilled_derive::distilled]
//pub fn reduce(a: u32, b: u32) -> u32 {
//    a + b
//}

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

struct Raw<X> {
    slice: &'static [u8],
    idx: usize,
    instance_count: u32,
    _phantom: std::marker::PhantomData<X>,
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

#[cfg(target_arch = "wasm32")]
mod wastd {
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

macro_rules! pipeline {
    ($name:ident, $in_ty:ty, ($($map:ident),*), $reduce:ident, $out_ty:ty) => (
        #[cfg(not(target_arch = "wasm32"))]
        #[allow(non_upper_case_globals)]
        const $name: ::distilled::iter::WasmFn<Vec<$in_ty>, $out_ty> = ::distilled::iter::WasmFn {
            entry: stringify!($name),
            get_in: "get_in",
            _phantom: ::std::marker::PhantomData,
        };

        #[cfg(target_arch = "wasm32")]
        #[no_mangle]
        pub unsafe fn $name(in_buffer_len: u32, instance_count: u32) -> u64 {
            use ::nanoserde::{ SerBin};
            fn inner(vals: impl Iterator<Item=$in_ty>) -> $out_ty {
                vals.fold(0, |acc, val| $reduce(acc, call_chain!(val, $($map),*)))
            }

            let ret = inner(Raw{
                slice: &wastd::IN_BUFFER[..in_buffer_len as usize],
                idx:0,
                instance_count,
                _phantom:std::marker::PhantomData
            });
            wastd::OUT_BUFFER.clear();
            ret.ser_bin(&mut wastd::OUT_BUFFER);
            ((wastd::OUT_BUFFER.as_ptr() as u64) << 32 | wastd::OUT_BUFFER.len() as u64)
        }
    )
}

#[cfg(target_arch = "wasm32")]
macro_rules! call_chain {
    ($param:tt, $first:ident, $($then:ident),+) => ({
        let x = $first($param);
        call_chain!(x, $($then),*)
    });
    ($param:tt, $first:ident) => ($first($param));
}

pipeline!(things, u8, (truc, pote), red, u32);

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    use distilled::iter::SliceExt;

    //let wasm_bytes = include_bytes!("../../target/wasm32-unknown-unknown/debug/wasm.wasm");
    let wasm_bytes = include_bytes!("../../target/wasm32-wasi/debug/wasm.wasm");
    let mut runner = distilled::Runner::new(wasm_bytes);

    let out = vec![1, 2, 3, 5].map_reduce(things, &mut runner);
    dbg!(out);

    Ok(())
}
