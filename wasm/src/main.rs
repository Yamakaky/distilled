use std::str;
use types::*;

const IN_BUFFER_SIZE: usize = 1024;
static mut IN_BUFFER: &[u8] = &[0; IN_BUFFER_SIZE];
const OUT_BUFFER_SIZE: usize = 1024;
static mut OUT_BUFFER: &mut [u8] = &mut [0; OUT_BUFFER_SIZE];

#[no_mangle]
pub fn get_in_buffer() -> *const u8 {
    unsafe { IN_BUFFER.as_ptr() }
}

#[no_mangle]
pub fn get_out_buffer() -> *const u8 {
    unsafe { OUT_BUFFER.as_ptr() }
}

#[no_mangle]
pub fn add(in_buffer_len: u32) -> u32 {
    let passed_string = unsafe { str::from_utf8(&IN_BUFFER[..in_buffer_len as usize]).unwrap() };
    let args = {
        let mut state = types::nanoserde::DeJsonState::default();
        let mut chars = passed_string.chars();
        state.next(&mut chars);
        state.next_tok(&mut chars).expect("deser2");
        Param::de_json(&mut state, &mut chars).expect("deser")
    };
    let ret: String = types::Ret {
        ret: args.a + args.b,
    }
    .serialize_json();
    unsafe {
        std::ptr::copy_nonoverlapping(ret.as_ptr(), OUT_BUFFER.as_mut_ptr(), ret.len());
    }
    ret.len() as u32
}

fn main() {}
