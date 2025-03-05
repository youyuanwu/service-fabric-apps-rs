// ------------------------------------------------------------
// Copyright 2024 Youyuan Wu
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

use std::{
    cell::Cell,
    ffi::c_void,
    sync::{Arc, Mutex},
};
use tracing::info;

use mssf_core::{
    runtime::{
        executor::{DefaultExecutor, Executor},
        stateful::{PrimaryReplicator, StatefulServiceFactory, StatefulServiceReplica},
        stateful_proxy::{PrimaryReplicatorProxy, StatefulServicePartition},
    },
    sync::CancellationToken,
    types::{OpenMode, ReplicaRole},
    Interface, GUID,
};
use mssf_core::{Error, WString};
use sfrc_c::ReliableCollectionRuntime::{IFabricDataLossHandler, TxnReplicator_Settings};
use sfrc_core::wrap::{get_txn_replicator, TxnReplicaReplicator};
use tokio::sync::oneshot::{self, Sender};
use tonic::transport::Server;

// use crate::utils::DataLossHandler;

pub struct Factory {
    replication_port: u32,
    grpc_port: u32,
    rt: DefaultExecutor,
}

impl Factory {
    pub fn create(replication_port: u32, grpc_port: u32, rt: DefaultExecutor) -> Factory {
        Factory {
            replication_port,
            grpc_port,
            rt,
        }
    }
}

fn get_addr(port: u32, hostname: WString) -> String {
    let mut addr = String::new();
    addr.push_str(&hostname.to_string());
    addr.push(':');
    addr.push_str(&port.to_string());
    addr
}

impl StatefulServiceFactory for Factory {
    fn create_replica(
        &self,
        servicetypename: &WString,
        servicename: &WString,
        initializationdata: &[u8],
        partitionid: &GUID,
        replicaid: i64,
    ) -> Result<impl StatefulServiceReplica, Error> {
        info!(
            "Factory::create_replica type {}, service {}, init data size {}, partid {:?}",
            servicetypename,
            servicename,
            initializationdata.len(),
            partitionid
        );

        let svc = Service::new(self.grpc_port, self.rt.clone());

        let replica = Replica::new(replicaid, svc, self.replication_port, self.grpc_port);
        Ok(replica)
    }
}

pub struct Replica {
    svc: Service,
    id: i64,
    rplc_port: u32,
    grpc_port: u32,
}

impl Replica {
    pub fn new(id: i64, svc: Service, rplc_port: u32, grpc_port: u32) -> Replica {
        Replica {
            svc,
            id,
            rplc_port,
            grpc_port,
        }
    }
}

// The serving of the database.
pub struct Service {
    rt: DefaultExecutor,
    store: Mutex<Cell<Option<Arc<TxnReplicaReplicator>>>>,
    tx: Mutex<Cell<Option<Sender<()>>>>,
    grpc_port: u32,
}

impl Service {
    pub fn new(grpc_port: u32, rt: DefaultExecutor) -> Service {
        Service {
            rt,
            store: Mutex::new(Cell::new(None)),
            tx: Mutex::new(Cell::new(None)),
            grpc_port,
        }
    }

    pub fn set_store(&self, store: TxnReplicaReplicator) {
        let prev = self.store.lock().unwrap().replace(Some(Arc::new(store)));
        assert!(prev.is_none());
    }

    fn get_store(&self) -> Arc<TxnReplicaReplicator> {
        self.store.lock().unwrap().get_mut().clone().unwrap()
    }

    pub fn start_loop(&self) {
        let (tx, rx) = oneshot::channel::<()>();
        self.stop();
        self.tx.lock().unwrap().set(Some(tx));
        let store = self.get_store();
        let port = self.grpc_port;

        self.rt.spawn(async move {
            info!("start grpc server on port: {}", port);
            let svc = rpc::rpc_svc::new(store);
            let addr = format!("[::1]:{}", port).parse().unwrap();

            Server::builder()
                .add_service(
                    crate::rcstore::rpc::rcstore_service_server::RcstoreServiceServer::new(svc),
                )
                .serve_with_shutdown(addr, async {
                    rx.await.ok();
                    println!("Graceful shutdown complete")
                })
                .await
                .unwrap();
        });
    }

    pub fn stop(&self) {
        let mut op = self.tx.lock().unwrap().take();
        if op.is_some() {
            op.take().unwrap().send(()).unwrap()
        }
    }
}

impl StatefulServiceReplica for Replica {
    async fn open(
        &self,
        openmode: OpenMode,
        partition: &StatefulServicePartition,
        _: CancellationToken,
    ) -> mssf_core::Result<impl PrimaryReplicator> {
        // should be primary replicator
        info!("Replica::open {:?}", openmode);

        // TODO: this is to unblock code change. The nullptr approach might not work.
        let dataloss_handler: IFabricDataLossHandler =
            unsafe { IFabricDataLossHandler::from_raw(std::ptr::null_mut()) };

        let addr = get_addr(self.rplc_port, WString::from("localhost"));
        let waddr = WString::from(addr);

        let txn_settings = TxnReplicator_Settings {
            ReplicatorAddress: waddr.as_ptr() as *mut c_void,
            ..Default::default()
        };
        // create reliable collection.
        let ok = get_txn_replicator(
            self.id,
            partition.get_com(),
            &dataloss_handler,
            &txn_settings,
            &WString::new(),
            &WString::new(),
            &WString::new(),
        );
        // Do not clean up nullptr
        std::mem::forget(dataloss_handler);
        if ok.is_err() {
            let e = ok.err().unwrap();
            info!("get_txn_replicator failed with {}", e);
            return Err(e);
        }

        let (p, txnp) = ok.unwrap();

        self.svc.set_store(txnp);

        Ok(PrimaryReplicatorProxy::new(p))
    }
    async fn change_role(
        &self,
        newrole: ReplicaRole,
        _: CancellationToken,
    ) -> mssf_core::Result<WString> {
        info!("Replica::change_role {:?}", newrole);

        if newrole == ReplicaRole::Primary {
            self.svc.start_loop();
        }
        let addr = WString::from(format!("http://localhost:{}", self.grpc_port));
        Ok(addr)
    }
    async fn close(&self, _: CancellationToken) -> mssf_core::Result<()> {
        info!("Replica::close");
        self.svc.stop();
        Ok(())
    }
    fn abort(&self) {
        info!("Replica::abort");
        self.svc.stop();
    }
}

// grpc code
#[allow(non_snake_case)]
pub mod rpc {
    use std::sync::Arc;

    use mssf_core::ErrorCode::E_NOT_FOUND;
    use mssf_core::{Error, WString, PCWSTR};
    use sfrc_c::ReliableCollectionRuntime::{
        StateProvider_Info, StateProvider_Info_V1_Size, StateProvider_Kind_Store,
        Store_LockMode_Exclusive,
    };
    use sfrc_core::wrap::{StateProvider, TxnReplicaReplicator};
    use tonic::async_trait;

    tonic::include_proto!("rcstore_rpc"); // The string specified here must match the proto package name

    pub struct rpc_svc {
        store: Arc<TxnReplicaReplicator>,
    }

    impl rpc_svc {
        pub fn new(store: Arc<TxnReplicaReplicator>) -> rpc_svc {
            rpc_svc { store }
        }

        async fn get_state_provider(&self, url: &WString) -> Result<StateProvider, Error> {
            let txn = self.store.create_transaction().unwrap();
            let waiter;
            {
                let timeout = 3000;

                let store_name = url;
                let lang = WString::default();
                let stateproviderinfo = StateProvider_Info {
                    Size: StateProvider_Info_V1_Size,
                    Kind: StateProvider_Kind_Store,
                    LangMetadata: PCWSTR(lang.as_ptr()),
                };
                // get store
                waiter = self.store.get_or_add_state_provider_async(
                    &txn,
                    store_name,
                    &lang,
                    &stateproviderinfo,
                    timeout,
                );
            }
            let (sp, _existing) = waiter.await.unwrap()?;
            txn.commit_async().await.unwrap()?;
            Ok(sp)
        }

        async fn add_interal(
            &self,
            sp: &StateProvider,
            key: &WString,
            val: String,
        ) -> Result<(), Error> {
            let txnn = self.store.create_transaction()?;
            let waiter = sp.add_async(&txnn, key, val.as_bytes(), 3000);
            waiter.await.unwrap()?;
            txnn.commit_async().await.unwrap()
        }

        async fn get_internal(&self, sp: &StateProvider, key: &WString) -> Result<String, Error> {
            let txn = self.store.create_transaction()?;
            let waiter = sp.conditional_get_async(&txn, key, 3000, Store_LockMode_Exclusive);
            let (found, val, _) = waiter.await.unwrap()?;
            txn.commit_async().await.unwrap()?;

            if found.0 == 0 {
                Err(Error::from(E_NOT_FOUND))
            } else {
                Ok(String::from_utf8_lossy(val.as_slice()).into_owned())
            }
        }

        async fn remove_internal(
            &self,
            sp: &StateProvider,
            key: &WString,
            conditionalversion: i64,
        ) -> Result<bool, Error> {
            let txn = self.store.create_transaction()?;
            let waiter = sp.conditional_remove_async(&txn, key, 3000, conditionalversion);
            let removed = waiter.await.unwrap()?;
            txn.commit_async().await.unwrap()?;
            Ok(removed.0 != 0)
        }

        async fn enumerate_all_internal(
            &self,
            sp: &StateProvider,
        ) -> Result<Vec<(String, String)>, Error> {
            let txn = self.store.create_transaction()?;
            let enu = sp.create_enumerator_async(&txn).await.unwrap()?;

            let mut result = Vec::<(String, String)>::new();

            loop {
                let (advanced, key, data, _) = enu.move_next_async().await.unwrap()?;
                if advanced.0 == 0 {
                    break;
                }
                result.push((
                    key.to_string(),
                    String::from_utf8_lossy(data.as_slice()).into_owned(),
                ))
            }
            txn.commit_async().await.unwrap()?;
            Ok(result)
        }
    }

    #[async_trait]
    impl rcstore_service_server::RcstoreService for rpc_svc {
        async fn add(
            &self,
            request: tonic::Request<AddRequest>,
        ) -> std::result::Result<tonic::Response<AddResponse>, tonic::Status> {
            let req = request.into_inner();
            let store_url = WString::from(req.store_url);
            let sp = self.get_state_provider(&store_url).await;
            if sp.is_err() {
                return Err(tonic::Status::internal(format!(
                    "Cannot get state provider {}",
                    sp.unwrap_err()
                )));
            }
            let sp = sp.unwrap();

            let key = WString::from(req.key);
            let val = req.val;

            let ok = self.add_interal(&sp, &key, val).await;
            match ok {
                Ok(_) => {
                    let resp = AddResponse {};
                    Ok(tonic::Response::new(resp))
                }
                Err(e) => Err(tonic::Status::internal(format!("cannot add : {}", e))),
            }
        }
        async fn get(
            &self,
            request: tonic::Request<GetRequest>,
        ) -> std::result::Result<tonic::Response<GetResponse>, tonic::Status> {
            let req = request.into_inner();
            let store_url = WString::from(req.store_url);
            let sp = self.get_state_provider(&store_url).await;
            if sp.is_err() {
                return Err(tonic::Status::internal(format!(
                    "Cannot get state provider {}",
                    sp.unwrap_err()
                )));
            }
            let sp = sp.unwrap();

            let key = WString::from(req.key);

            let ok = self.get_internal(&sp, &key).await;
            match ok {
                Ok(val) => {
                    let resp = GetResponse { val };
                    Ok(tonic::Response::new(resp))
                }
                Err(e) => Err(tonic::Status::internal(format!("cannot get : {}", e))),
            }
        }
        async fn remove(
            &self,
            request: tonic::Request<RemoveRequest>,
        ) -> std::result::Result<tonic::Response<RemoveResponse>, tonic::Status> {
            let req = request.into_inner();
            let store_url = WString::from(req.store_url);
            let sp = self.get_state_provider(&store_url).await;
            if sp.is_err() {
                return Err(tonic::Status::internal(format!(
                    "Cannot get state provider {}",
                    sp.unwrap_err()
                )));
            }
            let sp = sp.unwrap();

            let key = WString::from(req.key);
            let conditionalversion = req.conditional_version;

            let ok = self.remove_internal(&sp, &key, conditionalversion).await;
            match ok {
                Ok(removed) => {
                    let resp = RemoveResponse { removed };
                    Ok(tonic::Response::new(resp))
                }
                Err(e) => Err(tonic::Status::internal(format!("cannot remove : {}", e))),
            }
        }

        async fn enumerate_all(
            &self,
            request: tonic::Request<EnumerateRequest>,
        ) -> std::result::Result<tonic::Response<EnumerateResponse>, tonic::Status> {
            let req = request.into_inner();
            let store_url = WString::from(req.store_url);
            let sp = self.get_state_provider(&store_url).await;
            if sp.is_err() {
                return Err(tonic::Status::internal(format!(
                    "Cannot get state provider {}",
                    sp.unwrap_err()
                )));
            }
            let sp = sp.unwrap();
            let ok = self.enumerate_all_internal(&sp).await;
            match ok {
                Ok(v) => {
                    let resp = EnumerateResponse {
                        payload: v
                            .into_iter()
                            .map(|(k, v)| enumerate_response::KeyValue { key: k, value: v })
                            .collect(),
                    };
                    Ok(tonic::Response::new(resp))
                }
                Err(e) => Err(tonic::Status::internal(format!("cannot remove : {}", e))),
            }
        }
    }

    #[cfg(test)]
    mod test {
        use std::time::Duration;

        use mssf_core::{
            client::{
                svc_mgmt_client::{PartitionKeyType, ServiceEndpointRole},
                FabricClient,
            },
            WString,
        };

        #[tokio::test]
        async fn test_connect() {
            // resolve port on local onebox
            let fc = FabricClient::builder().build();
            let svcc = fc.get_service_manager();
            let resolution = svcc
                .resolve_service_partition(
                    &WString::from("fabric:/RcStore/RcStoreService"),
                    &PartitionKeyType::None,
                    None,
                    Duration::from_secs(1),
                    None,
                )
                .await
                .unwrap();
            // find primary
            let endpoint = resolution
                .get_endpoint_list()
                .iter()
                .find(|e| e.role == ServiceEndpointRole::StatefulPrimary)
                .expect("no primary found");
            let addr = endpoint.address.to_string();

            let mut client = super::rcstore_service_client::RcstoreServiceClient::connect(addr)
                .await
                .unwrap();

            let store_url = String::from("fabric:/mystore");

            // delete all entries
            {
                let req = tonic::Request::new(super::EnumerateRequest {
                    store_url: store_url.clone(),
                });
                let resp = client.enumerate_all(req).await.unwrap();
                for kv in resp.into_inner().payload {
                    let r = tonic::Request::new(super::RemoveRequest {
                        store_url: store_url.clone(),
                        key: kv.key,
                        conditional_version: -1, // -1 means ignore
                    });
                    let rp = client.remove(r).await.unwrap();
                    assert!(rp.into_inner().removed);
                }
            }
            // add
            {
                let req = tonic::Request::new(super::AddRequest {
                    store_url: store_url.clone(),
                    key: String::from("mykey"),
                    val: String::from("myval"),
                });
                let response = client.add(req).await;
                println!("RESPONSE={:?}", response);
            }
            // get
            {
                let req = tonic::Request::new(super::GetRequest {
                    store_url: store_url.clone(),
                    key: String::from("mykey"),
                });
                let response = client.get(req).await;
                println!("RESPONSE={:?}", response);
            }
        }
    }
}
