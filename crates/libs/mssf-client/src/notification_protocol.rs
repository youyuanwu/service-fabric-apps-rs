//! Source-derived notification schemas. No endpoint description enrichment.
use mssf_transport::{
    message::Message,
    serialization::serde::{from_slice, to_vec, CountedValues, Guid, ObjectArray, Pointer},
};
use serde::{Deserialize, Serialize};

use crate::{
    resolution::Generation, resolution_protocol::TableEntry, GatewayDescription, NamingUri, NodeId,
    Result,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Interval {
    pub start: i64,
    pub end: i64,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Versions {
    pub ranges: ObjectArray<Vec<Interval>>,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Flags {
    pub bits: u32,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Filter {
    pub id: u64,
    pub name: NamingUri,
    pub flags: Flags,
}
#[derive(Serialize)]
pub(crate) struct Connect<'a> {
    pub client: &'a str,
    pub generation: Generation,
    pub versions: Pointer<Versions>,
    pub filters: CountedValues<Vec<Pointer<Filter>>>,
}
#[derive(Serialize)]
pub(crate) struct Register<'a> {
    pub client: &'a str,
    pub filter: Pointer<Filter>,
}
#[derive(Serialize)]
pub(crate) struct Unregister<'a> {
    pub client: &'a str,
    pub id: u64,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct NodeInstance {
    pub id: NodeId,
    pub instance: u64,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Gateway {
    pub address: String,
    pub node: NodeInstance,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Connected {
    pub generation: Generation,
    pub last_deleted: i64,
    pub gateway: Gateway,
}
impl Connected {
    pub fn description(&self) -> GatewayDescription {
        GatewayDescription {
            address: self.gateway.address.clone(),
            node_id: (u128::from(self.gateway.node.id.high) << 64)
                | u128::from(self.gateway.node.id.low),
            node_instance: self.gateway.node.instance,
            node_name: self.gateway.name.clone(),
        }
    }
}
pub(crate) fn decode_connected(message: Message) -> Result<Connected> {
    crate::protocol::check_reply(&message, &["NotificationClientConnectionReply"])?;
    let body: Connected = from_slice(&message.body)?;
    // ServiceNotificationManager::CreateGatewayDescription intentionally emits
    // an empty address: node identity/name identify the actual notification gateway.
    if body.gateway.name.is_empty() {
        return Err(crate::Error::Protocol(
            "empty notification gateway identity",
        ));
    }
    Ok(body)
}
pub(crate) fn decode_registered(message: Message) -> Result<()> {
    crate::protocol::check_reply(&message, &["RegisterServiceNotificationFilterReply"])?;
    empty(&message)
}
pub(crate) fn decode_unregistered(message: Message) -> Result<()> {
    crate::protocol::check_reply(&message, &["UnregisterServiceNotificationFilterReply"])?;
    empty(&message)
}
fn empty(message: &Message) -> Result<()> {
    if !message.body.is_empty() {
        return Err(crate::Error::Protocol("filter reply has a body"));
    }
    Ok(())
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Activity {
    pub guid: Guid,
    pub index: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PageId {
    pub activity: Activity,
    pub index: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub entry: Pointer<TableEntry>,
    pub primary_only: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Page {
    pub id: PageId,
    pub generation: Generation,
    pub versions: Pointer<Versions>,
    pub entries: CountedValues<Vec<Pointer<Entry>>>,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct PageBody {
    pub page: Pointer<Page>,
}
pub(crate) fn decode_page(body: &[u8]) -> Result<Page> {
    from_slice::<PageBody>(body)?
        .page
        .0
        .ok_or(crate::Error::Protocol("null notification page"))
}

/// Allocation-free schema walk before any owned decoding. Includes wire input,
/// geometric Vec growth, decoded strings and endpoint/name conversion copies.
/// Unknown object suffixes are validated by the same bounded native Decoder.
pub(crate) fn page_allocation_bound(body: &[u8]) -> Result<usize> {
    use mssf_transport::serialization::{Decoder, MAX_COLLECTION_COUNT};
    let mut bytes = body.len() + 4096;
    let mut d = Decoder::new(body);
    d.object(|d| {
        d.pointer(|d, _| {
            d.object(|d| {
                d.object(|d| {
                    d.guid()?;
                    d.u64()?;
                    Ok(())
                })?;
                d.u64()?;
                Ok(())
            })?;
            d.object(|d| {
                d.i64()?;
                if d.byte_array_length()? != 16 {
                    return Err(crate::Error::Protocol("invalid generation owner"));
                }
                Ok(())
            })?;
            d.pointer(|d, _| {
                d.object_array(|d| {
                    bytes += 128;
                    d.i64()?;
                    d.i64()?;
                    Ok(())
                })?;
                Ok(())
            })?;
            let entries = d.u32()? as usize;
            if entries > MAX_COLLECTION_COUNT {
                return Err(crate::Error::Protocol("notification entry count limit"));
            }
            for _ in 0..entries {
                bytes += 4 * std::mem::size_of::<Entry>()
                    + 6 * std::mem::size_of::<crate::ServiceNotification>()
                    + 256;
                d.pointer(|d, _| {
                    d.pointer(|d, _| {
                        d.object(|d| {
                            d.guid()?;
                            Ok(())
                        })?;
                        bytes += d.string_allocation_bound()? * 6;
                        d.object(|d| {
                            d.boolean()?;
                            d.boolean()?;
                            bytes += d.string_allocation_bound()? * 2;
                            let count = d.u32()? as usize;
                            if count > MAX_COLLECTION_COUNT {
                                return Err(crate::Error::Protocol("endpoint count limit"));
                            }
                            bytes += count
                                * (4 * std::mem::size_of::<String>()
                                    + 4 * std::mem::size_of::<crate::ServiceEndpoint>()
                                    + 64);
                            for _ in 0..count {
                                bytes += d.string_allocation_bound()? * 2;
                            }
                            d.i64()?;
                            Ok(())
                        })?;
                        d.boolean()?;
                        Ok(())
                    })?
                    .ok_or(crate::Error::Protocol("null notification partition"))?;
                    d.boolean()?;
                    Ok(())
                })?
                .ok_or(crate::Error::Protocol("null notification entry"))?;
            }
            Ok(())
        })?
        .ok_or(crate::Error::Protocol("null notification page"))?;
        Ok(())
    })?;
    d.finish()?;
    Ok(bytes)
}
pub(crate) fn status(success: bool) -> Result<Vec<u8>> {
    #[derive(Serialize)]
    struct Status {
        error: i64,
    }
    // ErrorCodeValue::OperationFailed (HRESULT E_FAIL), not a Result payload.
    to_vec(&Status {
        error: if success { 0 } else { -2147467259 },
    })
}
