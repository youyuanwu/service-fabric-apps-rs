use mssf_com::{
    FabricRuntime::{IFabricOperation, IFabricOperationData, IFabricOperationData_Impl},
    FabricTypes::FABRIC_OPERATION_DATA_BUFFER,
};
use windows_core::implement;

use crate::{data::OperationDataProxy, traits::Operation};

pub struct OperationProxy {
    com_impl: IFabricOperation,
}

impl OperationProxy {
    pub fn new(com_impl: IFabricOperation) -> Self {
        Self { com_impl }
    }
}
impl Operation for OperationProxy {
    fn get_metadate(&self) -> crate::types::OperationMetadata {
        let raw = unsafe { self.com_impl.get_Metadata() };
        unsafe { raw.as_ref().unwrap().into() }
    }

    fn get_data(&self) -> mssf_core::Result<impl bytes::Buf> {
        // The returned buf has a ref count of the object.
        let data_wrap = IFabricOperationWrapForData(self.com_impl.clone()).into();
        let proxy = OperationDataProxy::new(data_wrap)?;
        Ok(proxy)
    }

    fn acknowledge(&self) -> mssf_core::Result<()> {
        unsafe { self.com_impl.Acknowledge() }.map_err(mssf_core::Error::from)
    }
}

// wrap type for operation to make it look like operation data
// so we can use the same conversion techniques
#[implement(IFabricOperationData)]
struct IFabricOperationWrapForData(IFabricOperation);

impl IFabricOperationData_Impl for IFabricOperationWrapForData_Impl {
    fn GetData(&self, count: *mut u32) -> windows_core::Result<*mut FABRIC_OPERATION_DATA_BUFFER> {
        unsafe { self.0.GetData(count) }
    }
}
