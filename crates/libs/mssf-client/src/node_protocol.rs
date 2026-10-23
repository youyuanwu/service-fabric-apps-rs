use mssf_transport::{
    message::Message,
    serialization::serde::{ByteArray, Derived, Guid, NativeType, ObjectArray, Pointer, Typed},
};
use serde::{Deserialize, Serialize};

use crate::{
    nodes::*, query_arguments::QueryArguments, query_protocol::Query, Error, Paging, Result,
};

impl Query for NodeQueryDescription {
    type Output = NodeListPage;
    const NAME: &'static str = "GetNodeList";
    const PAGING_FIRST: bool = true;

    fn encode_parameters(&self, arguments: &mut QueryArguments) -> Result<()> {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Fields<'a> {
            node_name: Option<&'a str>,
            node_status_filter: Option<NodeStatusFilter>,
        }
        arguments.append(&Fields {
            node_name: self
                .parameters
                .node_name
                .as_deref()
                .filter(|s| !s.is_empty()),
            node_status_filter: (self.parameters.status_filter != NodeStatusFilter::DEFAULT)
                .then_some(self.parameters.status_filter),
        })
    }

    fn paging(&self) -> Option<&Paging> {
        Some(&self.paging)
    }

    fn decode_reply(message: &Message) -> Result<NodeListPage> {
        crate::query_protocol::decode_list::<NodeWire, _, 1>(message, false, NodeWire::into_node)
    }
}

#[derive(Deserialize)]
struct NodeWire {
    name: String,
    id: ByteArray,
    ip_address_or_fqdn: String,
    node_type: String,
    code_version: String,
    config_version: String,
    status: i64,
    up_time_seconds: i64,
    health_state: i64,
    is_seed_node: bool,
    upgrade_domain: String,
    fault_domain: String,
    id_copy: ByteArray,
    instance_id: u64,
    cluster_connection_port: u32,
    deactivation: DeactivationWire,
    is_stopped: bool,
    down_time_seconds: i64,
    up_at: i64,
    down_at: i64,
    http_gateway_port: u16,
}

impl NodeWire {
    fn into_node(self) -> Result<Node> {
        let id = crate::protocol::decode_node_id(self.id)?;
        if crate::protocol::decode_node_id(self.id_copy)? != id {
            return Err(Error::Protocol("node ID copies disagree"));
        }
        Ok(Node {
            name: self.name,
            id,
            ip_address_or_fqdn: self.ip_address_or_fqdn,
            node_type: self.node_type,
            code_version: self.code_version,
            config_version: self.config_version,
            status: NodeStatus::from_raw(self.status),
            up_time_seconds: self.up_time_seconds,
            health_state: HealthState::from_raw(self.health_state),
            is_seed_node: self.is_seed_node,
            upgrade_domain: self.upgrade_domain,
            fault_domain: self.fault_domain,
            instance_id: self.instance_id,
            cluster_connection_port: self.cluster_connection_port,
            deactivation: self.deactivation.into_deactivation()?,
            is_stopped: self.is_stopped,
            down_time_seconds: self.down_time_seconds,
            up_at: NodeTimestamp(self.up_at),
            down_at: NodeTimestamp(self.down_at),
            http_gateway_port: self.http_gateway_port,
        })
    }
}

#[derive(Deserialize)]
struct TaskId(String, i64);
#[derive(Deserialize)]
struct TaskWire(TaskId, i64);
#[derive(Deserialize)]
struct SafetySlot {
    value: Pointer<Typed<SafetyCheck>>,
}
#[derive(Deserialize)]
struct DeactivationWire {
    effective_intent: i64,
    status: i64,
    tasks: ObjectArray<Vec<TaskWire>>,
    pending_safety_checks: ObjectArray<Vec<SafetySlot>>,
}
impl DeactivationWire {
    fn into_deactivation(self) -> Result<NodeDeactivation> {
        Ok(NodeDeactivation {
            effective_intent: NodeDeactivationIntent::from_raw(self.effective_intent),
            status: NodeDeactivationStatus::from_raw(self.status),
            tasks: self
                .tasks
                .0
                .into_iter()
                .map(
                    |TaskWire(TaskId(id, task_type), intent)| NodeDeactivationTask {
                        id,
                        task_type: NodeDeactivationTaskType::from_raw(task_type),
                        intent: NodeDeactivationIntent::from_raw(intent),
                    },
                )
                .collect(),
            pending_safety_checks: self
                .pending_safety_checks
                .0
                .into_iter()
                .map(
                    |SafetySlot {
                         value: Pointer(value),
                     }| {
                        value
                            .map(|Typed(value)| value)
                            .ok_or(Error::Protocol("null pending safety check"))
                    },
                )
                .collect::<Result<_>>()?,
        })
    }
}

impl NativeType for SafetyCheck {
    fn type_info(&self) -> mssf_transport::Result<&[u8]> {
        const TAGS: [[u8; 4]; 8] = [
            0i32.to_le_bytes(),
            1i32.to_le_bytes(),
            2i32.to_le_bytes(),
            3i32.to_le_bytes(),
            4i32.to_le_bytes(),
            5i32.to_le_bytes(),
            6i32.to_le_bytes(),
            7i32.to_le_bytes(),
        ];
        Ok(match self {
            Self::Invalid => &TAGS[0],
            Self::EnsureSeedNodeQuorum => &TAGS[1],
            Self::Partition { kind, .. } => match kind {
                SafetyCheckKind::EnsurePartitionQuorum => &TAGS[2],
                SafetyCheckKind::WaitForPrimaryPlacement => &TAGS[3],
                SafetyCheckKind::WaitForPrimarySwap => &TAGS[4],
                SafetyCheckKind::WaitForReconfiguration => &TAGS[5],
                SafetyCheckKind::WaitForInBuildReplica => &TAGS[6],
                SafetyCheckKind::EnsureAvailability => &TAGS[7],
                _ => {
                    return Err(mssf_transport::Error::InvalidInput(
                        "unsupported safety-check dynamic kind",
                    ))
                }
            },
        })
    }
    fn deserialize_fields<'de, D: serde::Deserializer<'de>>(
        tag: &[u8],
        d: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw: [u8; 4] = tag
            .try_into()
            .map_err(|_| serde::de::Error::custom("invalid safety-check type tag"))?;
        let kind = i32::from_le_bytes(raw);
        #[derive(Deserialize)]
        struct Base {
            kind: i64,
        }
        #[derive(Deserialize)]
        struct Partition {
            base: mssf_transport::serialization::serde::Inline<Base>,
            partition: Derived<PartitionId>,
        }
        #[derive(Deserialize)]
        struct PartitionId {
            id: Guid,
        }
        let (wire_kind, value) = match kind {
            0 | 1 => {
                let base = Base::deserialize(d)?;
                (
                    base.kind,
                    if kind == 0 {
                        Self::Invalid
                    } else {
                        Self::EnsureSeedNodeQuorum
                    },
                )
            }
            2..=7 => {
                let wire = Partition::deserialize(d)?;
                (
                    wire.base.0.kind,
                    Self::Partition {
                        kind: SafetyCheckKind::from_raw(i64::from(kind)),
                        partition_id: uuid::Uuid::from_bytes_le(wire.partition.0.id.0),
                    },
                )
            }
            _ => {
                return Err(serde::de::Error::custom(
                    "unsupported safety-check dynamic kind",
                ))
            }
        };
        if wire_kind != i64::from(kind) {
            return Err(serde::de::Error::custom(
                "safety-check tag and kind disagree",
            ));
        }
        Ok(value)
    }
}
