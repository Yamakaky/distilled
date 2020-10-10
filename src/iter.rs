use std::marker::PhantomData;

use crate::Runner;

pub struct DistIter<Item> {
    values: Vec<Item>,
}

pub struct Map<I, A, B> {
    iter: I,
    f: WasmFn<A, B>,
}

pub struct Reduce<I, A> {
    iter: I,
    f: WasmFn<(A, A), A>,
}

pub trait DistIterator {
    type Item;

    fn map<B>(self, f: WasmFn<Self::Item, B>) -> Map<Self, Self::Item, B>
    where
        Self: Sized,
        Self::Item: nanoserde::SerBin,
        B: nanoserde::DeBin,
    {
        Map { iter: self, f }
    }

    fn reduce(self, f: WasmFn<(Self::Item, Self::Item), Self::Item>) -> Reduce<Self, Self::Item>
    where
        Self: Sized,
        Self::Item: nanoserde::SerBin,
    {
        Reduce { iter: self, f }
    }

    fn collect(self, runner: &mut Runner) -> Vec<Self::Item>
    where
        Self: Sized,
        Self::Item: nanoserde::SerBin;
}

impl<Item> DistIterator for DistIter<Item> {
    type Item = Item;
    fn collect(self, _runner: &mut Runner) -> Vec<Self::Item>
    where
        Self: Sized,
        Self::Item: nanoserde::SerBin,
    {
        self.values
    }
}

impl<B, I> DistIterator for Map<I, I::Item, B>
where
    I: DistIterator,
    I::Item: nanoserde::SerBin,
    B: nanoserde::DeBin,
{
    type Item = B;

    fn collect(self, runner: &mut Runner) -> Vec<B>
    where
        Self: Sized,
        Self::Item: nanoserde::SerBin,
    {
        let f = self.f;
        self.iter
            .collect(runner)
            .into_iter()
            .map(|x| runner.run_one(&f, x))
            .collect()
    }
}

impl<I: DistIterator> Reduce<I, I::Item>
where
    I::Item: nanoserde::DeBin + nanoserde::SerBin + Default,
{
    pub fn run(self, runner: &mut Runner) -> I::Item
    where
        Self: Sized,
    {
        let f = self.f;
        self.iter
            .collect(runner)
            .into_iter()
            .fold(I::Item::default(), |a, b| runner.run_one(&f, (a, b)))
    }
}

#[derive(Clone)]
pub struct WasmFn<A, B> {
    pub entry: String,
    pub get_in: String,
    pub get_out: String,
    pub _phantom: PhantomData<(A, B)>,
}

pub trait SliceExt<Item> {
    fn dist_iter(self) -> DistIter<Item>;
}

impl<Item> SliceExt<Item> for Vec<Item> {
    fn dist_iter(self) -> DistIter<Item> {
        DistIter { values: self }
    }
}
