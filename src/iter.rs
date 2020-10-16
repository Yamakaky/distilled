use std::marker::PhantomData;

#[derive(Clone)]
pub struct WasmFn<A, B> {
    pub entry: &'static str,
    pub get_in: &'static str,
    pub reduce: Option<fn(B, B) -> B>,
    pub _phantom: PhantomData<(A, B)>,
}
