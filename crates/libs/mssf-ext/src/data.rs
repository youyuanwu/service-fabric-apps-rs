use core::slice;

use bytes::Buf;
use mssf_com::{
    FabricRuntime::{IFabricOperationData, IFabricOperationData_Impl},
    FabricTypes::FABRIC_OPERATION_DATA_BUFFER,
};
use windows_core::implement;

use crate::traits::OperationData;

#[repr(C)]
struct OperationDataBuffer(FABRIC_OPERATION_DATA_BUFFER);

impl OperationDataBuffer {
    pub fn as_bytes(&self) -> &[u8] {
        let len = self.0.BufferSize;
        if len == 0 {
            return &[];
        }
        let ptr = self.0.Buffer;
        assert!(!ptr.is_null());
        unsafe { slice::from_raw_parts(ptr, len as usize) }
    }
}

// impl easy reading for the operation data
struct OperationDataReader(IFabricOperationData);

impl OperationDataReader {
    fn get_data(&self) -> mssf_core::Result<&[OperationDataBuffer]> {
        let mut count = 0_u32;
        let data = unsafe { self.0.GetData(std::ptr::addr_of_mut!(count))? };
        if count == 0 {
            return Ok(&[]);
        }
        assert!(!data.is_null());
        let list =
            unsafe { slice::from_raw_parts(data as *const OperationDataBuffer, count as usize) };
        Ok(list)
    }
}

/// Given a IFabricOperationData, wraps it around to satisfy Buf trait
/// expose com to rust
pub struct OperationDataProxy {
    owner: OperationDataReader,
    remaining: usize,
    i: usize, // index of buffer list
    j: usize, // sub index for curr buffer
}

impl OperationDataProxy {
    pub fn new(owner: IFabricOperationData) -> mssf_core::Result<Self> {
        let reader = OperationDataReader(owner);
        Self::new_from_reader(reader)
    }
    fn new_from_reader(owner: OperationDataReader) -> mssf_core::Result<Self> {
        let data = owner.get_data()?; // check th data is ok
                                      // calculate the total size
        let mut total = 0;
        for buff in data {
            total += buff.as_bytes().len();
        }
        Ok(Self {
            owner,
            remaining: total,
            i: 0,
            j: 0,
        })
    }

    fn get_data(&self) -> &[OperationDataBuffer] {
        self.owner.get_data().unwrap() // checked at new so this will not panic
    }

    fn get_curr_buff(&self) -> &OperationDataBuffer {
        &self.get_data()[self.i]
    }

    fn get_curr_buff_len(&self) -> usize {
        self.get_curr_buff().as_bytes().len()
    }

    fn get_curr_buff_remain(&self) -> usize {
        let len = self.get_curr_buff_len();
        debug_assert!(len >= self.j);
        len - self.j
    }

    // return none if ran out of buff
    fn advance_buff(&mut self) -> Option<()> {
        if self.j >= self.get_data().len() {
            return None; // ran out of buffs
        }
        let curr_remain = self.get_curr_buff_remain();
        self.j = 0; // restart
        self.i += 1;
        self.remaining -= curr_remain;
        Some(())
    }

    fn advance_curr_index(&mut self, j: usize) {
        self.j += j;
        self.remaining -= j;
        let _ = self.get_curr_buff_remain(); // force debug check
    }
}

impl Buf for OperationDataProxy {
    fn remaining(&self) -> usize {
        self.remaining
    }

    fn chunk(&self) -> &[u8] {
        // return the current internal buff
        let curr_buff = &self.get_curr_buff();
        let b = curr_buff.as_bytes();
        // find the right index in buff
        &b[self.j..]
    }

    fn advance(&mut self, cnt: usize) {
        let mut target_remain = cnt;
        let mut c = self.chunk();
        let mut c_remain = c.remaining();
        debug_assert!(self.get_curr_buff_remain() == c_remain);
        while target_remain > c_remain {
            let ok = self.advance_buff();
            if ok.is_none() {
                panic!("out of bound. Ran out of buffers.");
            }
            target_remain -= c_remain;
            // next remain
            c = self.chunk();
            c_remain = c.remaining();
        }
        if c_remain == 0 || target_remain == c_remain {
            // curr buf can just fit.
            self.advance_buff(); // move to next buff.
        } else if target_remain > 0 {
            // curr buf is the right one
            debug_assert!(target_remain <= self.chunk().remaining());
            self.advance_curr_index(target_remain);
        }
    }
}

impl OperationData for OperationDataProxy {
    fn get_data(&self) -> mssf_core::Result<&impl Buf> {
        Ok(self)
    }
}

/// Given a Buf, wrap it to be IFabricOperationData
/// expose rust to com
#[implement(IFabricOperationData)]
pub struct OperationDataBridge<T>
where
    T: Buf + 'static,
{
    _inner: T,
    cache: Vec<FABRIC_OPERATION_DATA_BUFFER>,
}

impl<T: Buf> OperationDataBridge<T> {
    pub fn new(mut b: T) -> Self {
        let mut cache = vec![];
        // fill the cache
        while b.has_remaining() {
            let c = b.chunk();
            let fb = FABRIC_OPERATION_DATA_BUFFER {
                BufferSize: c.len() as u32,
                Buffer: c.as_ptr() as *mut u8,
            };
            cache.push(fb);
            b.advance(c.len())
        }
        Self { _inner: b, cache }
    }
}

impl<T: Buf> IFabricOperationData_Impl for OperationDataBridge_Impl<T> {
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn GetData(&self, count: *mut u32) -> windows_core::Result<*mut FABRIC_OPERATION_DATA_BUFFER> {
        assert!(!count.is_null());
        let ptr = self.cache.as_ptr();
        unsafe { *count = self.cache.len() as u32 };
        Ok(ptr as *mut FABRIC_OPERATION_DATA_BUFFER)
    }
}

// simple wrap of any Buf to impl OperationData
pub struct OperationDataBuf<T: Buf> {
    data: T,
}

impl<T: Buf> OperationDataBuf<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

impl<T: Buf + Send + Sync + 'static> OperationData for OperationDataBuf<T> {
    fn get_data(&self) -> mssf_core::Result<&impl Buf> {
        Ok(&self.data)
    }
}

impl<T: Buf> Buf for OperationDataBuf<T> {
    fn remaining(&self) -> usize {
        self.data.remaining()
    }

    fn chunk(&self) -> &[u8] {
        self.data.chunk()
    }

    fn advance(&mut self, cnt: usize) {
        self.data.advance(cnt)
    }
}

#[cfg(test)]
mod test {

    use bytes::{Buf, Bytes};
    use mssf_com::FabricRuntime::IFabricOperationData;

    use super::{OperationDataBridge, OperationDataProxy};

    #[test]
    fn test_single_buff() {
        let test_str = "hello world";
        let b = Bytes::from(test_str);
        let bridge = OperationDataBridge::new(b).into();
        let proxy = OperationDataProxy::new(bridge).unwrap();
        assert_eq!(proxy.remaining(), test_str.len());
        let s = proxy.chunk();
        assert_eq!(test_str.as_bytes(), s);
    }

    #[test]
    fn multi_buff() {
        let test_str1 = "mystr1";
        let test_str2 = "mystr2";
        let b1 = Bytes::from(test_str1);
        let b2 = Bytes::from(test_str2);

        // chain should have 2 chuncks
        let ch = b1.chain(b2);
        assert_eq!(ch.chunk(), test_str1.as_bytes());

        let bridge: IFabricOperationData = OperationDataBridge::new(ch).into();

        // test advance just the first chunk
        {
            let mut proxy = OperationDataProxy::new(bridge.clone()).unwrap();
            proxy.advance(1);
            assert_eq!(proxy.chunk(), &test_str1.as_bytes()[1..]);
            proxy.advance(test_str1.len() - 1);
            assert_eq!(proxy.chunk(), test_str2.as_bytes());
        }

        // test advance into the second chunk
        {
            let mut proxy = OperationDataProxy::new(bridge.clone()).unwrap();
            proxy.advance(7);
            assert_eq!(proxy.chunk(), &test_str2.as_bytes()[1..]);
        }

        // test copy
        {
            let mut proxy = OperationDataProxy::new(bridge).unwrap();
            assert_eq!(proxy.remaining(), test_str1.len() + test_str2.len());
            let s = proxy.copy_to_bytes(proxy.remaining());
            assert_eq!(s.remaining(), test_str1.len() + test_str2.len());
            // should have 1 chunk
            assert_eq!(s.chunk(), "mystr1mystr2".as_bytes());
        }
    }

    #[test]
    fn empty_buff() {
        let ch = Bytes::from("")
            .chain(Bytes::from("mystr"))
            .chain(Bytes::from(""));
        let bridge: IFabricOperationData = OperationDataBridge::new(ch).into();
        // test copy
        {
            let mut proxy = OperationDataProxy::new(bridge.clone()).unwrap();
            let s = proxy.copy_to_bytes(proxy.remaining());
            assert_eq!(s.chunk(), "mystr".as_bytes());
        }
        // test advance
        {
            let mut proxy = OperationDataProxy::new(bridge.clone()).unwrap();
            proxy.advance(1);
            assert_eq!(proxy.chunk(), "ystr".as_bytes());
        }
    }
}
