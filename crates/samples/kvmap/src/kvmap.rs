use std::{cell::Cell, sync::Arc};

use bytes::{Buf, Bytes};
use mssf_com::{
    FabricRuntime::IFabricStateProvider,
    FabricTypes::{FABRIC_REPLICATOR_ADDRESS, FABRIC_REPLICATOR_SETTINGS},
};
use mssf_core::{
    runtime::{
        executor::BoxedCancelToken,
        stateful::{PrimaryReplicator, StatefulServiceFactory, StatefulServiceReplica},
        stateful_proxy::{PrimaryReplicatorProxy, StatefulServicePartition},
    },
    types::{Epoch, OpenMode, ReplicaRole},
    Interface, WString, GUID,
};
use mssf_ext::{
    data::OperationDataBuf,
    state_provider::StateProviderBridge,
    state_replicator::StateReplicatorProxy,
    traits::{
        LocalOperationStream, Operation, OperationData, OperationDataStream, StateProvider,
        StateReplicator,
    },
};
use mssf_util::tokio::{TokioCancelToken, TokioExecutor};
use serde::{Deserialize, Serialize};
use tokio::{select, sync::Mutex};
use tonic::transport::Server;
use tracing::{error, info};

use crate::{
    app::KvApp, data::VecOperationDataStream, rpc::kvmap_service_server::KvmapServiceServer,
    ProcCtx,
};

pub struct Factory {
    ctx: ProcCtx,
}

impl Factory {
    pub fn create(ctx: ProcCtx) -> Self {
        Self { ctx }
    }
}

impl StatefulServiceFactory for Factory {
    fn create_replica(
        &self,
        servicetypename: &mssf_core::WString,
        servicename: &mssf_core::WString,
        initializationdata: &[u8],
        partitionid: &GUID,
        replicaid: i64,
    ) -> mssf_core::Result<impl StatefulServiceReplica> {
        info!("create_replica: service_type {servicetypename}, service_name {servicename}, init data len {}, partition id {:?}, replicaid {replicaid}",
          initializationdata.len(),
          partitionid
        );
        Ok(Replica::create(self.ctx.clone()))
    }
}

#[derive(Clone)]
pub struct ReplicaState {
    // for fast in mem lookup
    lsn: Arc<std::sync::Mutex<Cell<i64>>>,
    app: Arc<KvApp>,
}

impl ReplicaState {
    fn create() -> Self {
        Self {
            lsn: Arc::new(std::sync::Mutex::new(Cell::new(0))),
            app: Arc::new(KvApp::create()),
        }
    }

    // add 1 and return the result
    // async fn increment(&self) -> i64 {
    //     let mut lsn = self.lsn.lock().await;
    //     *lsn.get_mut() += 1;
    //     lsn.get()
    // }

    // set the value and return the prev
    fn apply(&self, lsn: i64) -> i64 {
        let mut lk = self.lsn.lock().unwrap();
        let prev = lk.get();
        *lk.get_mut() = lsn;
        prev
    }

    // // async fn get(&self) -> i64 {
    // //     let lsn = self.lsn.lock().await;
    // //     lsn.get()
    // // }

    fn get_sync(&self) -> i64 {
        let lk = self.lsn.lock().unwrap();
        lk.get()
    }

    // from primary
    fn get_copy_state(
        &self,
        up_to_sequence_number: i64,
        ctx_stream: impl OperationDataStream,
    ) -> impl OperationDataStream {
        CopyStateStream {
            state: self.app.clone(),
            up_to_sequence_number,
            ctx_stream,
        }
    }
}

// determines what to copy to secondary
struct CopyStateStream<T: OperationDataStream> {
    state: Arc<KvApp>,
    up_to_sequence_number: i64,
    ctx_stream: T,
}

// payload of copy state
#[derive(Serialize, Deserialize)]
struct CopyStatePayload {
    sn: i64,
    data: String,
}

impl<T: OperationDataStream> OperationDataStream for CopyStateStream<T> {
    async fn get_next(
        &self,
        cancellation_token: BoxedCancelToken,
    ) -> mssf_core::Result<Option<impl OperationData>> {
        // if ctx stream is end we end as well.
        let ctx = self.ctx_stream.get_next(cancellation_token).await?;
        if ctx.is_none() {
            return Ok(None);
        }
        let mut ctx_data = ctx.unwrap();
        let cb = ctx_data.copy_to_bytes(ctx_data.remaining());
        let s = String::from_utf8_lossy(&cb).into_owned();
        let peer_lsn = s.parse::<i64>().unwrap();
        // get current lsn
        let (lsn, data) = self.state.get_data().await.unwrap();
        #[allow(clippy::comparison_chain)]
        if lsn == peer_lsn {
            // peer is already upto date
            Ok(None)
        } else if lsn > peer_lsn {
            if self.up_to_sequence_number < peer_lsn {
                // peer has already caught up than the sync point
                return Ok(None);
            } else {
                // send data and lsn
                let p = CopyStatePayload { sn: lsn, data };
                return Ok(Some(OperationDataBuf::new(Bytes::from(
                    serde_json::to_string(&p).unwrap(),
                ))));
            }
        } else {
            panic!("peer is more advanced than primary {lsn}, {peer_lsn}");
        }
    }
}

pub struct Replica {
    ctx: ProcCtx,
    state_replicator: Mutex<Cell<Option<StateReplicatorProxy>>>,
    // cancel background work
    background_cancel: Mutex<Cell<Option<BoxedCancelToken>>>,
    // cancel rpc server
    rpc_cancel: Mutex<Cell<Option<BoxedCancelToken>>>,
    state: ReplicaState,
    role: Mutex<Cell<ReplicaRole>>,
}

impl Replica {
    fn create(ctx: ProcCtx) -> Self {
        Self {
            ctx,
            state_replicator: Mutex::new(Cell::new(None)),
            background_cancel: Mutex::new(Cell::new(None)),
            rpc_cancel: Mutex::new(Cell::new(None)),
            state: ReplicaState::create(),
            role: Mutex::new(Cell::new(ReplicaRole::Unknown)),
        }
    }

    fn start_rpc(
        rt: TokioExecutor,
        sr: StateReplicatorProxy,
        app: Arc<KvApp>,
        svc_addr: String,
        token: BoxedCancelToken,
    ) {
        // start rpc server
        rt.get_ref().spawn(async move {
            info!("start grpc server on addr: {}", svc_addr);
            let svc = crate::rpc::KvMapRpc::new(app, sr);
            let addr = svc_addr.parse().unwrap();
            Server::builder()
                .add_service(KvmapServiceServer::new(svc))
                .serve_with_shutdown(addr, async {
                    token.wait().await;
                    println!("Graceful shutdown tonic complete")
                })
                .await
                .unwrap();
        });
    }
}

impl StatefulServiceReplica for Replica {
    async fn open(
        &self,
        openmode: OpenMode,
        partition: &StatefulServicePartition,
        _: BoxedCancelToken,
    ) -> mssf_core::Result<impl PrimaryReplicator> {
        let com = partition.get_com();

        let stateprovider = KvStateProvider::create(self.ctx.rt.clone(), self.state.clone());
        let stateprovider_bridge: IFabricStateProvider =
            StateProviderBridge::new(stateprovider, self.ctx.rt.clone()).into();

        let mut rplctr;
        let state_rplctr;
        {
            let addr = WString::from(format!("{}:{}", "localhost", self.ctx.replication_port));
            let settings = FABRIC_REPLICATOR_SETTINGS {
                Flags: FABRIC_REPLICATOR_ADDRESS.0 as u32,
                ReplicatorAddress: mssf_core::PCWSTR(addr.as_ptr()),
                ..Default::default()
            };

            // return addr for replicator
            rplctr = None;
            let res = unsafe {
                com.CreateReplicator(
                    &stateprovider_bridge,
                    &settings,
                    std::ptr::addr_of_mut!(rplctr),
                )
            };
            if res.is_err() {
                error!("CreateReplicator failed: {:?}", res);
                return Err(res.err().unwrap().into());
            }
            state_rplctr = res.unwrap();
        }
        // save the state replicator in self
        let sr = StateReplicatorProxy::new(state_rplctr.cast().unwrap());
        {
            let lk = self.state_replicator.lock().await;
            let prev = lk.replace(Some(sr.clone()));
            assert!(prev.is_none());
        }
        self.state
            .app
            .open(&self.ctx.workdir)
            .await
            .inspect_err(|e| error!("Fail to open app db {e}"))?;

        // init db if it first time
        if openmode as u8 == OpenMode::New as u8 {
            self.state
                .app
                .set_data(0, "InitData".to_string())
                .await
                .inspect_err(|e| {
                    error!("fail to init data {}", e);
                })?;
        }

        // initiate the cancel token for closing.
        // let new_token = BoxedCancelToken::new();
        // let token = self.background_cancel.lock().await;
        // let prev = token.replace(Some(new_token));
        // assert!(prev.is_none());

        // initiate the rpc server token
        let rpc_token = TokioCancelToken::new_boxed();
        let new_svc_token = rpc_token.clone();
        {
            let token = self.rpc_cancel.lock().await;
            let prev = token.replace(Some(rpc_token));
            assert!(prev.is_none());
        }
        // start rpc server
        let app = self.state.app.clone();
        let svc_addr = format!("[::1]:{}", self.ctx.rpc_port);
        Self::start_rpc(self.ctx.rt.clone(), sr, app, svc_addr, new_svc_token);
        // return the replicator.
        let rplctr = rplctr.unwrap().cast().unwrap();
        let proxy = PrimaryReplicatorProxy::new(rplctr);
        Ok(proxy)
    }

    async fn change_role(
        &self,
        newrole: ReplicaRole,
        _: BoxedCancelToken,
    ) -> mssf_core::Result<WString> {
        // get the state replicator opened.
        let sr = self
            .state_replicator
            .lock()
            .await
            .get_mut()
            .clone()
            .unwrap();
        let rpc_port = self.ctx.rpc_port;
        let svc_addr = format!("[::1]:{rpc_port}",);
        // include scheme in the svc addr returned to SF.
        let addr_res = WString::from(format!("http://{svc_addr}",));
        // clean up pending stuff
        let curr_role = self.role.lock().await.get_mut().clone();
        if curr_role == newrole {
            // nothing has changed.
            return Ok(addr_res);
        }

        // init or re-init background token
        let token = TokioCancelToken::new_boxed();
        if !matches!(curr_role, ReplicaRole::None | ReplicaRole::Unknown) {
            // has background stuff running, cancel it
            let mut lk = self.background_cancel.lock().await;
            // cancel the prev token and init a new one
            let prev = lk.get_mut().replace(token.clone());
            prev.unwrap().cancel();
        };

        let state = self.state.clone();
        match newrole {
            ReplicaRole::ActiveSecondary => {
                // start rpc server on secondary
                // Self::start_rpc(self.ctx.rt.clone(), sr, app, svc_addr, new_svc_token);
                // Handle replicate stream from primary
                self.ctx.rt.get_ref().spawn(async move {
                    let rplct_stream = sr.get_replication_stream().unwrap();
                    loop {
                        let opt = select! {
                            _ = token.wait() => { None }
                            res = rplct_stream.get_operation(TokioCancelToken::new_boxed()) => {
                                res.unwrap() // get op should be ok
                            }
                        };

                        if opt.is_none() {
                            info!("replication stream end or cancelled.");
                            break;
                        }
                        // apply the data with lsn
                        let op = opt.unwrap();
                        let lsn = op.get_metadate().sequence_number;
                        let b = op.get_data().unwrap();
                        let s = String::from_utf8_lossy(b.chunk()).into_owned();
                        state.apply(lsn);
                        state.app.set_data(lsn, s.clone()).await.unwrap();
                        op.acknowledge().unwrap();
                        info!("rplct_stream: data: {}, lsn: {}", s, lsn);
                    }
                });
            }
            ReplicaRole::IdleSecondary => {
                self.ctx.rt.get_ref().spawn(async move {
                    // handle copying. i.e. catch up from primary. The stream is from copy_state from primary
                    let copy_stream = sr.get_copy_stream().unwrap();
                    while let Some(c) = copy_stream
                        .get_operation(TokioCancelToken::new_boxed())
                        .await
                        .unwrap()
                    {
                        let b = c.get_data().unwrap();
                        let s = String::from_utf8_lossy(b.chunk()).into_owned();
                        let p: CopyStatePayload = serde_json::from_str(&s).unwrap();
                        state.apply(p.sn);
                        state.app.set_data(p.sn, p.data.clone()).await.unwrap();
                        // ack the data is applied
                        c.acknowledge().unwrap();
                        info!(
                            "KvStateProvider::change_role: copy_stream: data: {} sn: {}",
                            p.data, p.sn
                        );
                    }
                    info!("KvStateProvider: Completed copy stream catchup on idle secondary.")
                });
            }
            ReplicaRole::None => {
                // delete stuff on disk?
            }
            ReplicaRole::Primary => {
                // start rpc server on primary
                // Self::start_rpc(self.ctx.rt.clone(), sr, app, svc_addr, new_svc_token);
                // start replicate?
                // self.ctx.rt.spawn(async move {
                //     let mut i = 0;
                //     loop {
                //         select! {
                //             _ = token.cancelled() => {
                //                 info!("replicate loop cancelled.");
                //                 break ;
                //             }
                //             _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {}
                //         };
                //         let data = format!("replicate-data-{i}");
                //         let sn = state.app.set_data_client(&sr2, data.clone()).await.unwrap();
                //         state.apply(sn);
                //         info!("replicated lsn {sn}, data: {data}");
                //         i += 1;
                //     }
                // })
            }
            ReplicaRole::Unknown => panic!("Unknonw role"),
        }
        Ok(addr_res)
    }

    async fn close(&self, _: BoxedCancelToken) -> mssf_core::Result<()> {
        // cancel background
        {
            let token = self.background_cancel.lock().await.take();
            if let Some(t) = token {
                t.cancel();
            }
        }
        // cancel rpc
        {
            let token = self.rpc_cancel.lock().await.take();
            if let Some(t) = token {
                t.cancel();
            }
        }
        Ok(())
    }

    fn abort(&self) {
        // cancel but sync. This is in SF thread pool.
        let token = self.background_cancel.blocking_lock().take();
        if let Some(t) = token {
            t.cancel();
        }
        // cancel rpc
        {
            let token = self.rpc_cancel.blocking_lock().take();
            if let Some(t) = token {
                t.cancel();
            }
        }
    }
}

pub struct KvStateProvider {
    _rt: TokioExecutor,
    state: ReplicaState,
}

impl KvStateProvider {
    fn create(rt: TokioExecutor, state: ReplicaState) -> Self {
        Self { _rt: rt, state }
    }
}

impl StateProvider for KvStateProvider {
    async fn update_epoch(
        &self,
        epoch: &Epoch,
        previousepochlastsequencenumber: i64,
        _: BoxedCancelToken,
    ) -> mssf_core::Result<()> {
        info!(
            "update_epoch: Epoch: {:?}, previous epoch lsn: {}",
            epoch, previousepochlastsequencenumber
        );
        Ok(())
    }

    fn get_last_committed_sequence_number(&self) -> mssf_core::Result<i64> {
        let lsn = self.state.get_sync();
        info!("get_last_committed_sequence_number: {lsn}");
        Ok(lsn)
    }
    async fn on_data_loss(&self, _: BoxedCancelToken) -> mssf_core::Result<bool> {
        info!("on_data_loss");
        Ok(false)
    }

    // invoked on secondary.
    fn get_copy_context(&self) -> mssf_core::Result<impl OperationDataStream> {
        info!("KvStateProvider::get_copy_state: get_copy_context");
        // just return the current lsn from secondary to primary
        let lsn = self.state.get_sync();
        Ok(VecOperationDataStream::new(vec![OperationDataBuf::new(
            Bytes::from(lsn.to_string()),
        )]))
    }

    // Invoked on primary. Data stream from above get_copy_context from secondary is presented here.
    fn get_copy_state(
        &self,
        upto_sequence_number: i64,
        copy_context_stream: impl OperationDataStream,
    ) -> mssf_core::Result<impl OperationDataStream> {
        info!(
            "KvStateProvider::get_copy_state: lsn upto {upto_sequence_number}, current lsn {}",
            self.state.get_sync()
        );
        Ok(self
            .state
            .get_copy_state(upto_sequence_number, copy_context_stream))
    }
}
