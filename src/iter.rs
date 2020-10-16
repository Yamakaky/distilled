use std::marker::PhantomData;

#[derive(Clone)]
pub struct WasmFn<A, B> {
    pub entry: &'static str,
    pub get_in: &'static str,
    pub _phantom: PhantomData<(A, B)>,
}
