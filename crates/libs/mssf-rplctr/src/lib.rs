// replicator impl

pub mod rpc;
pub mod rplctr;
pub mod rplctr_inner;
pub mod state_rplctr;

// use mssf_core::runtime::stateful::PrimaryReplicator;
// use rplctr::Rplctr;

// pub fn create_rplctr() -> mssf_core::Result<impl PrimaryReplicator> {
//     Ok(Rplctr {})
// }

#[cfg(test)]
mod tests {}
