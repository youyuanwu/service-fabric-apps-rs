use mssf_com::FabricTypes::{
    FABRIC_OPERATION_METADATA, FABRIC_OPERATION_TYPE, FABRIC_OPERATION_TYPE_ATOMIC_GROUP_OPERATION,
    FABRIC_OPERATION_TYPE_COMMIT_ATOMIC_GROUP, FABRIC_OPERATION_TYPE_CREATE_ATOMIC_GROUP,
    FABRIC_OPERATION_TYPE_END_OF_STREAM, FABRIC_OPERATION_TYPE_HAS_ATOMIC_GROUP_MASK,
    FABRIC_OPERATION_TYPE_INVALID, FABRIC_OPERATION_TYPE_NORMAL,
    FABRIC_OPERATION_TYPE_ROLLBACK_ATOMIC_GROUP, FABRIC_SERVICE_KIND, FABRIC_SERVICE_KIND_STATEFUL,
    FABRIC_SERVICE_KIND_STATELESS, FABRIC_SERVICE_QUERY_DESCRIPTION, FABRIC_URI,
};
use mssf_core::WString;

pub struct ServiceQueryDescription {
    pub application_name: WString,           // in url format
    pub servicename_filter: Option<WString>, // in url format
}

impl ServiceQueryDescription {
    // raw type has lifetime the same as self
    pub fn get_raw(&self) -> FABRIC_SERVICE_QUERY_DESCRIPTION {
        FABRIC_SERVICE_QUERY_DESCRIPTION {
            ApplicationName: FABRIC_URI(self.application_name.as_ptr() as *mut u16),
            ServiceNameFilter: if self.servicename_filter.is_none() {
                FABRIC_URI(std::ptr::null_mut())
            } else {
                FABRIC_URI(self.servicename_filter.as_ref().unwrap().as_ptr() as *mut u16)
            },
            Reserved: std::ptr::null_mut(),
        }
    }
}

pub enum ServiceKind {
    Invalid,
    Stateless,
    Stateful,
}

impl From<&FABRIC_SERVICE_KIND> for ServiceKind {
    fn from(value: &FABRIC_SERVICE_KIND) -> Self {
        match *value {
            FABRIC_SERVICE_KIND_STATEFUL => ServiceKind::Stateful,
            FABRIC_SERVICE_KIND_STATELESS => ServiceKind::Stateless,
            _ => ServiceKind::Invalid,
        }
    }
}

//pub struct ServiceList(IFabricGetServiceListResult);

// TODO: need to export iter mod in core
// impl FabricListAccessor<FABRIC_SERVICE_QUERY_RESULT_ITEM> for ServiceList{

// }
pub enum OperationType {
    AtomicGroup,
    CommitAtomicGroup,
    CreateAtomicGroup,
    EndOfStream,
    HasAtomicGroupMask,
    Invalid,
    Normal,
    RollbackAtomicGroup,
}

impl From<FABRIC_OPERATION_TYPE> for OperationType {
    fn from(value: FABRIC_OPERATION_TYPE) -> Self {
        match value {
            FABRIC_OPERATION_TYPE_ATOMIC_GROUP_OPERATION => OperationType::AtomicGroup,
            FABRIC_OPERATION_TYPE_COMMIT_ATOMIC_GROUP => OperationType::CommitAtomicGroup,
            FABRIC_OPERATION_TYPE_CREATE_ATOMIC_GROUP => OperationType::CreateAtomicGroup,
            FABRIC_OPERATION_TYPE_END_OF_STREAM => OperationType::EndOfStream,
            FABRIC_OPERATION_TYPE_HAS_ATOMIC_GROUP_MASK => OperationType::HasAtomicGroupMask,
            FABRIC_OPERATION_TYPE_INVALID => OperationType::Invalid,
            FABRIC_OPERATION_TYPE_NORMAL => OperationType::Normal,
            FABRIC_OPERATION_TYPE_ROLLBACK_ATOMIC_GROUP => OperationType::RollbackAtomicGroup,
            _ => panic!("unknown {value:?}"),
        }
    }
}

impl From<OperationType> for FABRIC_OPERATION_TYPE {
    fn from(value: OperationType) -> Self {
        match value {
            OperationType::AtomicGroup => FABRIC_OPERATION_TYPE_ATOMIC_GROUP_OPERATION,
            OperationType::CommitAtomicGroup => FABRIC_OPERATION_TYPE_COMMIT_ATOMIC_GROUP,
            OperationType::CreateAtomicGroup => FABRIC_OPERATION_TYPE_CREATE_ATOMIC_GROUP,
            OperationType::EndOfStream => FABRIC_OPERATION_TYPE_END_OF_STREAM,
            OperationType::HasAtomicGroupMask => FABRIC_OPERATION_TYPE_HAS_ATOMIC_GROUP_MASK,
            OperationType::Invalid => FABRIC_OPERATION_TYPE_INVALID,
            OperationType::Normal => FABRIC_OPERATION_TYPE_NORMAL,
            OperationType::RollbackAtomicGroup => FABRIC_OPERATION_TYPE_ROLLBACK_ATOMIC_GROUP,
        }
    }
}

// FABRIC_OPERATION_METADATA
pub struct OperationMetadata {
    pub operation_type: OperationType,
    pub sequence_number: i64,
    pub atomic_group_id: i64,
}

impl From<&FABRIC_OPERATION_METADATA> for OperationMetadata {
    fn from(value: &FABRIC_OPERATION_METADATA) -> Self {
        Self {
            operation_type: value.Type.into(),
            sequence_number: value.SequenceNumber,
            atomic_group_id: value.AtomicGroupId,
        }
    }
}
