use mssf_transport::message::Message;
use mssf_transport::serialization::serde::{NativeType, ObjectArray, Pointer, Typed};
use serde::{Deserialize, Serialize};

use crate::{
    health::*, query_arguments::QueryArguments, query_protocol::Query, Error, HealthState,
    NodeTimestamp, Result,
};

impl Query for NodeHealthQueryDescription {
    type Output = NodeHealth;
    const NAME: &'static str = "GetNodeHealth";

    fn encode_parameters(&self, arguments: &mut QueryArguments) -> Result<()> {
        let query = &self.parameters;
        // Bound caller work before UTF-16 scanning, not just before wire allocation.
        let name = &query.node_name;
        if name.is_empty()
            || name.len() > 4096 * 3
            || name.contains('\0')
            || name.encode_utf16().count() > 4096
        {
            return Err(Error::InvalidInput(
                "node health name must be 1..=4096 UTF-16 units without NUL",
            ));
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Policy {
            consider_warning_as_error: bool,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Filter {
            health_state_filter: u32,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Fields<'a> {
            cluster_health_policy: Option<String>,
            events_filter: Option<String>,
            node_name: &'a str,
        }
        fn json(value: &impl Serialize) -> Result<String> {
            serde_json::to_string(value)
                .map_err(|_| Error::InvalidInput("failed to serialize query JSON argument"))
        }
        arguments.append(&Fields {
            cluster_health_policy: query
                .health_policy
                .map(|policy| {
                    json(&Policy {
                        consider_warning_as_error: policy.consider_warning_as_error,
                    })
                })
                .transpose()?,
            events_filter: query
                .events_filter
                .map(|filter| {
                    json(&Filter {
                        health_state_filter: filter.bits(),
                    })
                })
                .transpose()?,
            node_name: name,
        })
    }

    fn decode_reply(message: &Message) -> Result<NodeHealth> {
        decode_reply(message)
    }
}

pub(crate) fn decode_reply(message: &Message) -> Result<NodeHealth> {
    crate::protocol::check_failure(message)?;
    let properties = message.properties()?;
    if properties.actor != 16 || properties.action != "OperationSuccessAction" {
        return Err(Error::Protocol("unexpected node health success envelope"));
    }
    #[derive(Deserialize)]
    struct ItemWire {
        _list: ObjectArray<Vec<ForbiddenListItem>>,
        item: Pointer<HealthWire>,
    }
    let result =
        crate::query_protocol::decode::<crate::query_protocol::TaggedPayload<ItemWire, 16>, _>(
            &message.body,
            false,
            |crate::query_protocol::TaggedPayload(wire)| {
                wire.item
                    .0
                    .ok_or(Error::Protocol("null node health item"))?
                    .into_health()
            },
        )?;
    if result.kind != 1 || result.paging.is_some() {
        return Err(Error::Protocol("node health result is not an unpaged Item"));
    }
    result
        .payload
        .ok_or(Error::Protocol("node health has no typed wrapper"))
}

struct ForbiddenListItem;
impl<'de> Deserialize<'de> for ForbiddenListItem {
    fn deserialize<D: serde::Deserializer<'de>>(_: D) -> std::result::Result<Self, D::Error> {
        Err(serde::de::Error::custom("node health item contains a list"))
    }
}
#[derive(Deserialize)]
struct EvaluationSlot {
    value: Pointer<Typed<HealthEvaluation>>,
}
#[derive(Deserialize)]
struct HealthWire {
    node_name: String,
    health_events: ObjectArray<Vec<EventWire>>,
    aggregated_health_state: i64,
    unhealthy_evaluations: ObjectArray<Vec<EvaluationSlot>>,
}
impl HealthWire {
    fn into_health(self) -> Result<NodeHealth> {
        Ok(NodeHealth {
            node_name: self.node_name,
            health_events: self.health_events.0.into_iter().map(Into::into).collect(),
            aggregated_health_state: HealthState::from_raw(self.aggregated_health_state),
            unhealthy_evaluations: self
                .unhealthy_evaluations
                .0
                .into_iter()
                .map(
                    |EvaluationSlot {
                         value: Pointer(value),
                     }| {
                        value
                            .map(|Typed(value)| value)
                            .ok_or(Error::Protocol("null health evaluation"))
                    },
                )
                .collect::<Result<_>>()?,
        })
    }
}
impl NativeType for HealthEvaluation {
    fn type_info(&self) -> mssf_transport::Result<&[u8]> {
        match self {
            Self::Event(_) => Ok(&[1, 0, 0, 0]),
        }
    }
    fn deserialize_fields<'de, D: serde::Deserializer<'de>>(
        tag: &[u8],
        d: D,
    ) -> std::result::Result<Self, D::Error> {
        if tag != 1i32.to_le_bytes() {
            return Err(serde::de::Error::custom(
                "unsupported health evaluation dynamic kind",
            ));
        }
        #[derive(Deserialize)]
        struct Evaluation {
            kind: i64,
            description: String,
            unhealthy_event: EventWire,
            consider_warning_as_error: bool,
            aggregated_health_state: i64,
        }
        let wire = Evaluation::deserialize(d)?;
        if wire.kind != 1 {
            return Err(serde::de::Error::custom(
                "health evaluation tag and kind disagree",
            ));
        }
        Ok(Self::Event(EventHealthEvaluation {
            description: wire.description,
            unhealthy_event: wire.unhealthy_event.into(),
            consider_warning_as_error: wire.consider_warning_as_error,
            aggregated_health_state: HealthState::from_raw(wire.aggregated_health_state),
        }))
    }
}

#[derive(Deserialize)]
struct EventWire {
    source_id: String,
    property: String,
    time_to_live_ticks: i64,
    health_state: i64,
    description: String,
    sequence_number: i64,
    source_utc_timestamp: i64,
    last_modified_utc_timestamp: i64,
    is_expired: bool,
    remove_when_expired: bool,
    last_ok_transition_at: i64,
    last_warning_transition_at: i64,
    last_error_transition_at: i64,
}

impl From<EventWire> for HealthEvent {
    fn from(wire: EventWire) -> Self {
        Self {
            source_id: wire.source_id,
            property: wire.property,
            time_to_live_ticks: wire.time_to_live_ticks,
            health_state: HealthState::from_raw(wire.health_state),
            description: wire.description,
            sequence_number: wire.sequence_number,
            source_utc_timestamp: NodeTimestamp(wire.source_utc_timestamp),
            last_modified_utc_timestamp: NodeTimestamp(wire.last_modified_utc_timestamp),
            is_expired: wire.is_expired,
            remove_when_expired: wire.remove_when_expired,
            last_ok_transition_at: NodeTimestamp(wire.last_ok_transition_at),
            last_warning_transition_at: NodeTimestamp(wire.last_warning_transition_at),
            last_error_transition_at: NodeTimestamp(wire.last_error_transition_at),
        }
    }
}
