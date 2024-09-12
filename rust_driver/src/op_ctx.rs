use std::{
    fmt::Debug,
    sync::{Arc, OnceLock},
    thread::{self, Thread},
};

use parking_lot::Mutex;

use crate::Error;

/// The status of operations.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
/// The status of operations.
pub enum CtxStatus {
    /// The operation is invalid.
    Invalid,
    /// The operation is running.
    Running,
    /// The operation is stopped.
    Failed(&'static str),
    /// The operation is finished.
    Finished,
}

/// The operation context.
///
/// The operation context is track to manage the status of operations.
/// When calling the `read`,`write` or some `control` command, you get an operation context.
///
/// You can wait for the operation to finish by calling the `wait` method and get the result by calling the `get_result` method.
#[derive(Debug, Clone)]
pub struct OpCtx<Payload>(Arc<OpCtxWrapper<Payload>>);

#[allow(clippy::type_complexity)] //FIXME: refactor later
struct OpCtxWrapper<Payload> {
    inner: Mutex<OpCtxInner>,
    payload: OnceLock<Payload>,
    handler: Mutex<Option<Box<dyn Fn(bool) + Sync + Send>>>,
}

impl<Payload> Debug for OpCtxWrapper<Payload> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpCtxWrapper")
            .field("inner", &self.inner)
            .finish()
    }
}

#[derive(Debug)]
struct OpCtxInner {
    thread: Option<Thread>,
    status: CtxStatus,
}

/// The control command operation context.
#[allow(clippy::module_name_repetitions)]
pub type CtrlOpCtx = OpCtx<bool>; // `is_sucess`

impl<Payload> OpCtx<Payload> {
    /// Create a new operation context with the status of `Running`.
    #[must_use]
    pub fn new_running() -> Self {
        let inner = OpCtxInner {
            thread: None,
            status: CtxStatus::Running,
        };
        let wrapper = OpCtxWrapper {
            inner: Mutex::new(inner),
            payload: OnceLock::new(),
            handler: Mutex::new(None),
        };
        Self(Arc::new(wrapper))
    }

    /// # Errors
    /// Returns an error if the operation context is poisoned.
    pub fn wait(&self) -> Result<(), Error> {
        let mut guard = self.0.inner.lock();
        if matches!(guard.status, CtxStatus::Running) {
            guard.thread = Some(thread::current());
            drop(guard);
            thread::park();
        }
        Ok(())
    }

    // TODO: use enum rather than str
    pub(crate) fn set_error(&self, cause: &'static str) {
        // set only once
        let mut guard = self.0.inner.lock();
        guard.status = CtxStatus::Failed(cause);
        if let Some(thread) = guard.thread.take() {
            thread.unpark();
        }
    }
    pub(crate) fn set_result(&self, result: Payload) -> Result<(), Error> {
        self.0
            .payload
            .set(result)
            .map_err(|_| Error::SetCtxResultFailed)?;
        // set only once
        let mut guard = self.0.inner.lock();
        guard.status = CtxStatus::Finished;
        if let Some(thread) = guard.thread.take() {
            thread.unpark();
        }
        Ok(())
    }

    /// Get the result of the operation.
    ///
    /// Returns `None` if the operation is not finished.
    #[must_use]
    pub fn get_result(&self) -> Option<&Payload> {
        self.0.payload.get()
    }

    /// # Errors
    /// Returns an error if the operation context is poisoned.
    pub fn wait_result(&self) -> Result<Option<&Payload>, Error> {
        self.wait()?;
        if let CtxStatus::Failed(reason) = self.0.inner.lock().status{
            return Err(Error::Device(reason.into()))
        }
        Ok(self.0.payload.get())
    }

    /// Get the status of the operation.
    pub fn status(&self) -> CtxStatus {
        self.0.inner.lock().status
    }

    pub(crate) fn set_handler(&self, handler: Box<dyn Fn(bool) + Send + Sync>){
        let mut guard = self.0
            .handler
            .lock();
        *guard = Some(handler);
    }

    pub(crate) fn take_handler(&self) -> Option<Box<(dyn Fn(bool) + Send + Sync)>> {
        let mut guard = self.0
            .handler
            .lock();
        guard.take()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_op_ctx() {
        let ctx = super::OpCtx::new_running();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            ctx_clone.set_result(true).unwrap();
        });
        let _ = ctx.wait();
        assert_eq!(ctx.get_result(), Some(true).as_ref());

        let ctx = super::OpCtx::new_running();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            ctx_clone.set_result(false).unwrap();
        });
        let _ = ctx.wait_result();
        assert_eq!(ctx.get_result(), Some(false).as_ref());
    }
}
