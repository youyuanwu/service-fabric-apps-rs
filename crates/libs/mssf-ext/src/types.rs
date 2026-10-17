use mssf_com::FabricTypes::{
    FABRIC_OPERATION_METADATA, FABRIC_OPERATION_TYPE, FABRIC_SERVICE_KIND,
    FABRIC_SERVICE_QUERY_DESCRIPTION,
};
use mssf_core::types::Uri;

pub struct ServiceQueryDescription {
    pub application_name: Uri,           // in url format
    pub servicename_filter: Option<Uri>, // in url format
}

impl ServiceQueryDescription {
    // raw type has lifetime the same as self
    pub fn get_raw(&self) -> FABRIC_SERVICE_QUERY_DESCRIPTION {
        FABRIC_SERVICE_QUERY_DESCRIPTION {
            ApplicationName: self.application_name.as_raw(),
            ServiceNameFilter: self
                .servicename_filter
                .as_ref()
                .unwrap_or(&Uri::default())
                .as_raw(),
            Reserved: std::ptr::null_mut(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServiceKind {
    Invalid,
    Stateless,
    Stateful,
}

impl From<&FABRIC_SERVICE_KIND> for ServiceKind {
    fn from(value: &FABRIC_SERVICE_KIND) -> Self {
        match *value {
            FABRIC_SERVICE_KIND::FABRIC_SERVICE_KIND_STATEFUL => ServiceKind::Stateful,
            FABRIC_SERVICE_KIND::FABRIC_SERVICE_KIND_STATELESS => ServiceKind::Stateless,
            _ => ServiceKind::Invalid,
        }
    }
}

//pub struct ServiceList(IFabricGetServiceListResult);

// TODO: need to export iter mod in core
// impl FabricListAccessor<FABRIC_SERVICE_QUERY_RESULT_ITEM> for ServiceList{

// }
#[derive(Debug, PartialEq, Eq)]
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
            FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_ATOMIC_GROUP_OPERATION => {
                OperationType::AtomicGroup
            }
            FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_COMMIT_ATOMIC_GROUP => {
                OperationType::CommitAtomicGroup
            }
            FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_CREATE_ATOMIC_GROUP => {
                OperationType::CreateAtomicGroup
            }
            FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_END_OF_STREAM => {
                OperationType::EndOfStream
            }
            FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_HAS_ATOMIC_GROUP_MASK => {
                OperationType::HasAtomicGroupMask
            }
            FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_INVALID => OperationType::Invalid,
            FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_NORMAL => OperationType::Normal,
            FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_ROLLBACK_ATOMIC_GROUP => {
                OperationType::RollbackAtomicGroup
            }
            _ => panic!("unknown {value:?}"),
        }
    }
}

impl From<OperationType> for FABRIC_OPERATION_TYPE {
    fn from(value: OperationType) -> Self {
        match value {
            OperationType::AtomicGroup => {
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_ATOMIC_GROUP_OPERATION
            }
            OperationType::CommitAtomicGroup => {
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_COMMIT_ATOMIC_GROUP
            }
            OperationType::CreateAtomicGroup => {
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_CREATE_ATOMIC_GROUP
            }
            OperationType::EndOfStream => {
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_END_OF_STREAM
            }
            OperationType::HasAtomicGroupMask => {
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_HAS_ATOMIC_GROUP_MASK
            }
            OperationType::Invalid => FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_INVALID,
            OperationType::Normal => FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_NORMAL,
            OperationType::RollbackAtomicGroup => {
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_ROLLBACK_ATOMIC_GROUP
            }
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
            sequence_number: value.SequenceNumber.0,
            atomic_group_id: value.AtomicGroupId.0,
        }
    }
}

#[cfg(test)]
mod test {
    use mssf_com::FabricTypes::{FABRIC_ATOMIC_GROUP_ID, FABRIC_SEQUENCE_NUMBER};

    use super::*;

    #[test]
    fn service_kinds_match_native_values() {
        for (native, expected) in [
            (
                FABRIC_SERVICE_KIND::FABRIC_SERVICE_KIND_STATEFUL,
                ServiceKind::Stateful,
            ),
            (
                FABRIC_SERVICE_KIND::FABRIC_SERVICE_KIND_STATELESS,
                ServiceKind::Stateless,
            ),
            (
                FABRIC_SERVICE_KIND::FABRIC_SERVICE_KIND_INVALID,
                ServiceKind::Invalid,
            ),
            (FABRIC_SERVICE_KIND(-1), ServiceKind::Invalid),
        ] {
            assert_eq!(ServiceKind::from(&native), expected);
        }
    }

    #[test]
    fn operation_types_match_native_values() {
        for (native, expected) in [
            (
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_ATOMIC_GROUP_OPERATION,
                OperationType::AtomicGroup,
            ),
            (
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_COMMIT_ATOMIC_GROUP,
                OperationType::CommitAtomicGroup,
            ),
            (
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_CREATE_ATOMIC_GROUP,
                OperationType::CreateAtomicGroup,
            ),
            (
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_END_OF_STREAM,
                OperationType::EndOfStream,
            ),
            (
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_HAS_ATOMIC_GROUP_MASK,
                OperationType::HasAtomicGroupMask,
            ),
            (
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_INVALID,
                OperationType::Invalid,
            ),
            (
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_NORMAL,
                OperationType::Normal,
            ),
            (
                FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_ROLLBACK_ATOMIC_GROUP,
                OperationType::RollbackAtomicGroup,
            ),
        ] {
            assert_eq!(OperationType::from(native), expected);
            assert_eq!(FABRIC_OPERATION_TYPE::from(expected), native);
        }
    }

    #[test]
    fn operation_metadata_preserves_native_identifiers() {
        let native = FABRIC_OPERATION_METADATA {
            Type: FABRIC_OPERATION_TYPE::FABRIC_OPERATION_TYPE_NORMAL,
            SequenceNumber: FABRIC_SEQUENCE_NUMBER(i64::MAX),
            AtomicGroupId: FABRIC_ATOMIC_GROUP_ID(-1),
            ..Default::default()
        };
        let metadata = OperationMetadata::from(&native);
        assert_eq!(metadata.operation_type, OperationType::Normal);
        assert_eq!(metadata.sequence_number, i64::MAX);
        assert_eq!(metadata.atomic_group_id, -1);
    }
}
