use std::task::*;
use std::{collections::HashMap, pin::Pin};
use std::{future::Future, sync::Arc, sync::Mutex};

#[derive(Debug, Default)]
pub struct Manager {
    wakers: HashMap<u64, Waker>,
    values: HashMap<u64, Result<Vec<u8>, crate::ExecutionError>>,
}

impl Manager {
    pub fn new() -> Manager {
        Manager::default()
    }

    pub fn wake(&mut self, id: u64, value: Result<Vec<u8>, crate::ExecutionError>) {
        // If absent, the future was cancelled.
        if self.wakers.contains_key(&id) {
            let none = self.values.insert(id, value);
            assert!(none.is_none());
            self.wakers.remove(&id).expect("unreachable").wake_by_ref();
        }
    }

    fn set_waker(&mut self, id: u64, waker: Waker) {
        self.wakers.insert(id, waker);
    }

    fn value(&mut self, id: u64) -> Option<Result<Vec<u8>, crate::ExecutionError>> {
        self.values.remove(&id)
    }

    fn cancel(&mut self, id: u64) {
        self.values.remove(&id);
        self.wakers.remove(&id);
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
    type Output = Result<Vec<u8>, crate::ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut manager = self.manager.lock().expect("error locking the manager");
        if let Some(res) = manager.value(self.id) {
            Poll::Ready(res)
        } else {
            manager.set_waker(self.id, cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Drop for RunFuture {
    fn drop(&mut self) {
        // TODO: cancel the remote computation
        let mut manager = self.manager.lock().expect("error locking the manager");
        manager.cancel(self.id);
    }
}

pub fn next_id() -> u64 {
    static mut LAST_ID: u64 = 0;
    unsafe {
        LAST_ID += 1;
        LAST_ID
    }
}
