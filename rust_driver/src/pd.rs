use crate::{types::Qpn, Device, Error, Mr};
use rand::RngCore as _;
use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
};

// TODO: PD will be shared by multi function call. Use reference counter?
/// Protection Domain
#[derive(Debug, Clone, Copy, Default)]
pub struct Pd {
    pub(crate) handle: u32,
}

#[derive(Debug)]
pub(crate) struct PdCtx {
    pub(crate) mr: HashSet<Mr>,
    pub(crate) qp: HashSet<Qpn>,
}

impl Device {
    /// allocate a pd
    ///
    /// # Errors
    ///
    /// Will return `Err` if:
    /// * lock poisoned
    pub fn alloc_pd(&self) -> Result<Pd, Error> {
        let mut pool = self.0.pd.lock();

        let pd = Pd {
            handle: rand::thread_rng().next_u32(),
        };

        let res = pool.insert(
            pd,
            PdCtx {
                mr: HashSet::new(),
                qp: HashSet::new(),
            },
        );

        if res.is_some() {
            return Err(Error::Invalid(format!("PD :{pd:?}")));
        }

        Ok(pd)
    }

    /// deallocate a pd
    ///
    /// # Errors
    ///
    /// Will return `Err` if:
    /// * lock poisoned
    /// * invalid Pd
    /// * mr or qp is in use
    pub fn dealloc_pd(&self, pd: Pd) -> Result<(), Error> {
        let mut pool = self.0.pd.lock();
        let pd_ctx = pool.get(&pd).ok_or(Error::Invalid(format!("PD :{pd:?}")))?;

        if !pd_ctx.mr.is_empty() {
            return Err(Error::PdInUse(format!("mr is not empty:{:?}", pd_ctx.mr)));
        }

        if !pd_ctx.qp.is_empty() {
            return Err(Error::PdInUse(format!("qp is not empty:{:?}", pd_ctx.qp)));
        }

        let _: Option<PdCtx> = pool.remove(&pd);

        Ok(())
    }
}

impl Hash for Pd {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.handle.hash(state);
    }
}

impl PartialEq for Pd {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
    }
}

impl Eq for Pd {}
