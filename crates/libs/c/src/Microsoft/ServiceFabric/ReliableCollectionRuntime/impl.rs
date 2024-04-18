pub trait IFabricDataLossHandler_Impl: Sized {
    fn BeginOnDataLoss(
        &self,
        callback: *mut ::core::ffi::c_void,
        context: *mut *mut ::core::ffi::c_void,
    ) -> ::windows_core::Result<()>;
    fn EndOnDataLoss(
        &self,
        context: *mut ::core::ffi::c_void,
        isstatechanged: *mut u8,
    ) -> ::windows_core::Result<()>;
}
impl ::windows_core::RuntimeName for IFabricDataLossHandler {}
impl IFabricDataLossHandler_Vtbl {
    pub const fn new<
        Identity: ::windows_core::IUnknownImpl<Impl = Impl>,
        Impl: IFabricDataLossHandler_Impl,
        const OFFSET: isize,
    >() -> IFabricDataLossHandler_Vtbl {
        unsafe extern "system" fn BeginOnDataLoss<
            Identity: ::windows_core::IUnknownImpl<Impl = Impl>,
            Impl: IFabricDataLossHandler_Impl,
            const OFFSET: isize,
        >(
            this: *mut ::core::ffi::c_void,
            callback: *mut ::core::ffi::c_void,
            context: *mut *mut ::core::ffi::c_void,
        ) -> ::windows_core::HRESULT {
            let this = (this as *const *const ()).offset(OFFSET) as *const Identity;
            let this = (*this).get_impl();
            this.BeginOnDataLoss(
                ::core::mem::transmute_copy(&callback),
                ::core::mem::transmute_copy(&context),
            )
            .into()
        }
        unsafe extern "system" fn EndOnDataLoss<
            Identity: ::windows_core::IUnknownImpl<Impl = Impl>,
            Impl: IFabricDataLossHandler_Impl,
            const OFFSET: isize,
        >(
            this: *mut ::core::ffi::c_void,
            context: *mut ::core::ffi::c_void,
            isstatechanged: *mut u8,
        ) -> ::windows_core::HRESULT {
            let this = (this as *const *const ()).offset(OFFSET) as *const Identity;
            let this = (*this).get_impl();
            this.EndOnDataLoss(
                ::core::mem::transmute_copy(&context),
                ::core::mem::transmute_copy(&isstatechanged),
            )
            .into()
        }
        Self {
            base__: ::windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            BeginOnDataLoss: BeginOnDataLoss::<Identity, Impl, OFFSET>,
            EndOnDataLoss: EndOnDataLoss::<Identity, Impl, OFFSET>,
        }
    }
    pub fn matches(iid: &::windows_core::GUID) -> bool {
        iid == &<IFabricDataLossHandler as ::windows_core::Interface>::IID
    }
}
