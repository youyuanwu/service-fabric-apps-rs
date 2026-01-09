use std::time::Duration;

use mssf_core::{
    client::{
        query_client::QueryClient,
        svc_mgmt_client::{
            FilterIdHandle, PartitionKeyType, ServiceEndpointRole, ServiceManagementClient,
        },
        FabricClient,
    },
    types::{
        QueryServiceReplicaStatus, ReplicaRole, RestartReplicaDescription,
        ServiceNotificationFilterDescription, ServiceNotificationFilterFlags,
        ServicePartitionInformation, ServicePartitionQueryDescription,
        ServicePartitionQueryResultItem, ServicePartitionStatus, ServiceReplicaQueryDescription,
        ServiceReplicaQueryResultItem, StatefulServiceReplicaQueryResult, Uri,
    },
    ErrorCode, WString, GUID,
};
use tokio::sync::Semaphore;

use crate::rpc::{DataSnPayload, EmptyPayload};
use lazy_static::lazy_static;

// limit 1 test at a time.
static PERMIT: Semaphore = Semaphore::const_new(1);
static TIMEOUT_LONG: Duration = Duration::from_secs(10);
static TIMEOUT: Duration = Duration::from_secs(1);
static RETRY_COUNT_LONG: usize = 30;
static RETRY_COUNT_SHORT: usize = 10;
static SVC_URI: &str = "fabric:/KvMap/KvMapService";
lazy_static! {
    static ref KV_MAP_SVC_URI: WString = WString::from(SVC_URI);
    static ref FABRIC_CLIENT: FabricClient = FabricClient::builder().build().unwrap();
}

// helper for managing app
pub struct KvMapMgmt {
    svc: ServiceManagementClient,
    query: QueryClient,
}

impl KvMapMgmt {
    pub fn new(c: &FabricClient) -> Self {
        Self {
            svc: c.get_service_manager().clone(),
            query: c.get_query_manager().clone(),
        }
    }

    pub async fn register_notification(&self) -> FilterIdHandle {
        let desc = ServiceNotificationFilterDescription {
            name: Uri::from(SVC_URI),
            flags: ServiceNotificationFilterFlags::NamePrefix,
        };
        // register takes more than 1 sec.
        self.svc
            .register_service_notification_filter(&desc, TIMEOUT_LONG, None)
            .await
            .unwrap()
    }

    pub async fn unregister_notification(&self, h: FilterIdHandle) {
        self.svc
            .unregister_service_notification_filter(h, TIMEOUT_LONG, None)
            .await
            .unwrap();
    }

    // first is primary
    pub async fn get_addrs(&self) -> mssf_core::Result<(String, String)> {
        let resolution = self
            .svc
            .resolve_service_partition(
                &Uri::from(KV_MAP_SVC_URI.clone()),
                &PartitionKeyType::None,
                None,
                TIMEOUT_LONG,
                None,
            )
            .await
            .unwrap();
        // find endpoints
        let endpoints = resolution.endpoints;

        // there is only 2 replicas

        let primary_addr = endpoints
            .iter()
            .find(|e| e.role == ServiceEndpointRole::StatefulPrimary);
        let secondary_addr = endpoints
            .iter()
            .find(|e| e.role == ServiceEndpointRole::StatefulSecondary);
        #[allow(clippy::unnecessary_unwrap)]
        if primary_addr.is_none() || secondary_addr.is_none() {
            Err(ErrorCode::E_FAIL.into())
        } else {
            Ok((
                primary_addr.unwrap().address.to_string(),
                secondary_addr.unwrap().address.to_string(),
            ))
        }
    }

    pub async fn get_addrs_retry(&self) -> (String, String) {
        for _ in 0..RETRY_COUNT_LONG {
            if let Ok(addrs) = self.get_addrs().await {
                return addrs;
            }
            tokio::time::sleep(TIMEOUT).await;
        }
        panic!("fail to get addrs");
    }

    pub async fn get_partition(&self) -> mssf_core::Result<(GUID, ServicePartitionStatus)> {
        let desc = ServicePartitionQueryDescription {
            service_name: Uri::from(KV_MAP_SVC_URI.clone()),
            partition_id_filter: None,
        };
        let list = self
            .query
            .get_partition_list(&desc, TIMEOUT_LONG, None)
            .await?;
        // there is only one partition
        let p = list.service_partitions.first().unwrap();
        let stateful = match p {
            ServicePartitionQueryResultItem::Stateful(s) => s.clone(),
            _ => panic!("not stateless"),
        };
        let info = stateful.partition_information;
        let single = match info {
            ServicePartitionInformation::Singleton(s) => s,
            _ => panic!("not singleton"),
        };
        Ok((single.id, stateful.partition_status))
    }

    pub async fn get_partition_wait_ready(&self) -> (GUID, ServicePartitionStatus) {
        for _ in 0..RETRY_COUNT_LONG {
            if let Ok((id, status)) = self.get_partition().await {
                if status == ServicePartitionStatus::Ready {
                    return (id, status);
                }
            }
            tokio::time::sleep(TIMEOUT).await;
        }
        panic!("partition not found or not ready");
    }

    // returns secondary for now.
    pub async fn get_replicas(
        &self,
        partition_id: GUID,
    ) -> mssf_core::Result<(
        StatefulServiceReplicaQueryResult,
        StatefulServiceReplicaQueryResult,
    )> {
        let desc = ServiceReplicaQueryDescription {
            partition_id,
            replica_id_or_instance_id_filter: None,
        };

        let replicas = self
            .query
            .get_replica_list(&desc, TIMEOUT_LONG, None)
            .await
            .unwrap();
        let replicas = replicas
            .service_replicas
            .iter()
            .map(|x| match x {
                ServiceReplicaQueryResultItem::Stateful(s) => s.clone(),
                _ => panic!("not stateful"),
            })
            .collect::<Vec<_>>();
        if replicas.len() < 2 {
            // not yet ready
            return Err(ErrorCode::E_FAIL.into());
        }

        let primary = replicas
            .iter()
            .find(|r| r.replica_role == ReplicaRole::Primary);
        if primary.is_none() {
            return Err(ErrorCode::E_FAIL.into());
        }
        let secondary = replicas
            .iter()
            .find(|r| r.replica_role != ReplicaRole::Primary);
        if secondary.is_none() {
            return Err(ErrorCode::E_FAIL.into());
        }
        Ok((primary.unwrap().clone(), secondary.unwrap().clone()))
    }

    pub async fn get_replicas_wait_healthy(
        &self,
        partition_id: GUID,
    ) -> (
        StatefulServiceReplicaQueryResult,
        StatefulServiceReplicaQueryResult,
    ) {
        for _ in 0..RETRY_COUNT_LONG {
            if let Ok((p, s)) = self.get_replicas(partition_id).await {
                if s.replica_status == QueryServiceReplicaStatus::Ready
                    && p.replica_status == QueryServiceReplicaStatus::Ready
                {
                    return (p, s);
                }
            }
            tokio::time::sleep(TIMEOUT).await;
        }
        panic!("replicas not found or not healthy");
    }

    pub async fn restart_replica(&self, node_name: WString, partition_id: GUID, replica_id: i64) {
        let desc = RestartReplicaDescription {
            node_name,
            partition_id,
            replica_or_instance_id: replica_id,
        };
        self.svc
            .restart_replica(&desc, TIMEOUT_LONG, None)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn read_write_test() {
    let _token = PERMIT.acquire().await.unwrap();
    let c = KvMapMgmt::new(&FABRIC_CLIENT);
    let h = c.register_notification().await;

    // wait for replica healthy
    let (partition_id, status) = c.get_partition_wait_ready().await;
    assert_eq!(status, ServicePartitionStatus::Ready);
    let (_, _) = c.get_replicas_wait_healthy(partition_id).await;

    // resolve port on local onebox
    let (primary_addr, secondary_addr) = c.get_addrs_retry().await;

    println!("primary_addr: {primary_addr}",);
    // connect primary via grpc
    let mut client = crate::rpc::kvmap_service_client::KvmapServiceClient::connect(primary_addr)
        .await
        .unwrap();
    // connect secondary
    println!("secondary_addr: {secondary_addr}",);
    let mut sec_client =
        crate::rpc::kvmap_service_client::KvmapServiceClient::connect(secondary_addr)
            .await
            .unwrap();

    // set data and read
    {
        let data = "mydata";
        // sn is ignored for now
        let req = tonic::Request::new(DataSnPayload {
            data: data.to_string(),
            sn: -1,
        });
        let response = client.set_data(req).await.expect("rpc failed").into_inner();
        let sn = response.sn;
        assert!(response.ok);
        assert_ne!(sn, 0);
        println!("RESPONSE={response:?}");

        // read from primary
        {
            let req2 = tonic::Request::new(EmptyPayload {});
            let response2 = client.get_data(req2).await.expect("rpc faile").into_inner();
            assert_eq!(response2.data, data);
            assert_eq!(sn, response2.sn);
            println!("RESPONSE={response2:?}");
        }
        // read from secondary
        {
            let req2 = tonic::Request::new(EmptyPayload {});
            let response2 = sec_client
                .get_data(req2)
                .await
                .expect("rpc faile")
                .into_inner();
            assert_eq!(response2.data, data);
            assert_eq!(sn, response2.sn);
            println!("RESPONSE={response2:?}");
        }
    }

    c.unregister_notification(h).await;
}

// TODO: perform failover.
#[tokio::test]
async fn failover_test() {
    let _token = PERMIT.acquire().await.unwrap();
    let c = KvMapMgmt::new(&FABRIC_CLIENT);
    let h = c.register_notification().await;

    let (partition_id, status) = c.get_partition_wait_ready().await;
    assert_eq!(status, ServicePartitionStatus::Ready);

    let (primary, secondary) = c.get_replicas_wait_healthy(partition_id).await;
    // restart secondary
    c.restart_replica(secondary.node_name, partition_id, secondary.replica_id)
        .await;

    // wait for replica id change of secondary
    for i in 0..RETRY_COUNT_SHORT {
        let (p2, s2) = c.get_replicas_wait_healthy(partition_id).await;
        if s2.replica_id != secondary.replica_id {
            assert_eq!(p2.replica_id, primary.replica_id);
            break;
        }
        if i == 10 {
            panic!("secondary replica id did not change");
        }
        tokio::time::sleep(TIMEOUT).await;
    }

    // save addr before primary failover
    let (p_addr, _) = c.get_addrs_retry().await;

    // restart primary
    c.restart_replica(primary.node_name, partition_id, primary.replica_id)
        .await;
    for i in 0..RETRY_COUNT_SHORT {
        let (p3, _) = c.get_replicas_wait_healthy(partition_id).await;

        if p3.replica_id != primary.replica_id {
            break;
        }

        if i == RETRY_COUNT_SHORT {
            panic!("primary replica id did not change");
        }
        tokio::time::sleep(TIMEOUT).await;
    }

    // wait for addr change of primary
    for i in 0..RETRY_COUNT_SHORT {
        let (p_addr2, _) = c.get_addrs_retry().await;
        if p_addr != p_addr2 {
            break;
        }
        if i == RETRY_COUNT_SHORT {
            panic!("primary addr did not change");
        }
        tokio::time::sleep(TIMEOUT).await;
    }
    c.unregister_notification(h).await;
}
