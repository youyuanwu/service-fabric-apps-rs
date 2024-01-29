#include "windows.h"
#include "ole2.h"

// some internal definition needed for this lib to work

MIDL_INTERFACE("0bba0a6a-8f00-41b5-9bbf-3ee30357028d")
IFabricDataLossHandler : public IUnknown
{
public:
    virtual HRESULT STDMETHODCALLTYPE BeginOnDataLoss( 
        /* [in] */ void *callback,
        /* [retval][out] */ void **context) = 0;
    
    virtual HRESULT STDMETHODCALLTYPE EndOnDataLoss( 
        /* [in] */ void *context,
        /* [retval][out] */ BOOLEAN *isStateChanged) = 0;
    
};