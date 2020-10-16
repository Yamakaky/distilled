use std::marker::PhantomData;

//TODO separate reduce and map
#[derive(Clone)]
pub struct WasmFn<A, B> {
    pub entry: &'static str,
    pub get_in: &'static str,
    pub reduce: Option<fn(B, B) -> B>,
    pub _phantom: PhantomData<(A, B)>,
}
