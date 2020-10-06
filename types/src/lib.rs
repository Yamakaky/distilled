pub use nanoserde::{self, DeJson, SerJson};

#[derive(Copy, Clone, Debug, Default, DeJson, SerJson)]
pub struct Param {
    pub a: u32,
    pub b: u32,
}

#[derive(Copy, Clone, Debug, Default, DeJson, SerJson)]
pub struct Ret {
    pub ret: u32,
}

#[distilled_derive::distilled]
pub fn proc_add(a: u32, b: u32) -> u32 {
    a + b
}
