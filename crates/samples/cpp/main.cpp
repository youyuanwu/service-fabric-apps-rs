#include <FabricStrings.h>
#include <winrt/base.h>

class StringProxy{
  winrt::com_ptr<IFabricStringsBytes> com_;
public:
  StringProxy(winrt::com_ptr<IFabricStringsBytes> com): com_(com){}

  std::string_view get_view(){
    const BYTE* buffer = {};
    ULONG len = {};
    this->com_->GetBytes(&buffer, &len);
    return std::string_view((char*)buffer, len);
  }
};

class StringImpl : public winrt::implements<StringImpl, IFabricStringsBytes>{
  std::string data_;
public:
  StringImpl(std::string data) : data_(data){}

  void STDMETHODCALLTYPE GetBytes( 
            /* [out] */ __RPC__deref_out_opt const BYTE **Buffer,
            /* [out] */ __RPC__out ULONG *BufferSize) override{
    assert(Buffer != nullptr);
    assert(BufferSize != nullptr);
    ULONG len = this->data_.size();
    const char* ptr = this->data_.c_str();
    *Buffer = (BYTE*)ptr;
    *BufferSize = len;
  }
};

class Callback : public winrt::implements<Callback, IFabricAsyncOperationCallback>{
  void STDMETHODCALLTYPE Invoke( 
            /* [in] */ __RPC__in_opt IFabricAsyncOperationContext *context) override{
    // do nothing for now.
  }
};

int main(){
  winrt::com_ptr<IFabricStringsApiTable> api;
  auto hr = GetFabricStringsApiTable(api.put());
  assert(hr == S_OK);
  winrt::com_ptr<IFabricStringsBytes> s1 = winrt::make<StringImpl>("mystr1");
  winrt::com_ptr<IFabricStringsBytes> s2 = winrt::make<StringImpl>("mystr2");
  winrt::com_ptr<IFabricAsyncOperationCallback> callback = winrt::make<Callback>();
  winrt::com_ptr<IFabricAsyncOperationContext> ctx;
  hr = api->BeginConcatStrings(s1.get(), s2.get(), 1000, callback.get(), ctx.put());
  assert(hr == S_OK);
  assert(ctx);
  while(1){
    if(!ctx->IsCompleted()){
      std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }else{
      break;
    }
  }
  winrt::com_ptr<IFabricStringsBytes> s3;
  hr = api->EndConcatStrings(ctx.get(), s3.put());
  assert(hr == S_OK);
  StringProxy proxy(s3);
  assert(proxy.get_view() == std::string("mystr1mystr2"));
}