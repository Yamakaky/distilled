use std::task::*;
use std::{collections::HashMap, pin::Pin};
use std::{future::Future, sync::Arc, sync::Mutex};

pub struct Manager {
    wakers: HashMap<u64, Waker>,
    values: HashMap<u64, Vec<u8>>,
}

impl Manager {
    pub fn new() -> Manager {
        Manager {
            wakers: HashMap::new(),
            values: HashMap::new(),
        }
    }
    fn set_waker(&mut self, id: u64, waker: Waker) {
        self.wakers.insert(id, waker);
    }

    pub fn wake(&mut self, id: u64, value: Vec<u8>) {
        let none = self.values.insert(id, value);
        assert!(none.is_none());
        self.wakers.remove(&id).unwrap().wake_by_ref();
    }

    fn value(&mut self, id: u64) -> Option<Vec<u8>> {
        self.values.remove(&id)
    }
}

pub struct RunFuture {
    manager: Arc<Mutex<Manager>>,
    id: u64,
}

impl RunFuture {
    pub fn new(id: u64, manager: Arc<Mutex<Manager>>) -> RunFuture {
        RunFuture { manager, id }
    }
}

impl Future for RunFuture {
    type Output = Vec<u8>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut manager = self.manager.lock().unwrap();
        if let Some(res) = manager.value(self.id) {
            Poll::Ready(res)
        } else {
            manager.set_waker(self.id, cx.waker().clone());
            Poll::Pending
        }
    }
}

pub fn next_id() -> u64 {
    static mut LAST_ID: u64 = 0;
    unsafe {
        LAST_ID += 1;
        LAST_ID
    }
}