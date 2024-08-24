use std::{
    cell::Cell,
    path::{Path, PathBuf},
    sync::Mutex,
};

use bytes::Bytes;
use mssf_core::sync::CancellationToken;
use mssf_ext::{
    data::OperationDataBuf,
    traits::{OperationData, OperationDataStream},
};
use tokio::fs::OpenOptions;

// return counting data until max number of times.
pub struct CountingOperationDataStream {
    count: Mutex<Cell<usize>>,
    max: usize,
    data: &'static str,
}

impl CountingOperationDataStream {
    pub fn new(max: usize, data: &'static str) -> Self {
        Self {
            count: Mutex::new(Cell::new(0)),
            max,
            data,
        }
    }
}

// dummy stream returns data 2 times and then none
impl OperationDataStream for CountingOperationDataStream {
    async fn get_next(
        &self,
        _: CancellationToken,
    ) -> mssf_core::Result<Option<impl OperationData>> {
        let mut c = self.count.lock().unwrap();
        if c.get() == self.max {
            return Ok(None);
        }

        let buf = Bytes::from(format!("{}:{}", &self.data, c.get()));
        *c.get_mut() += 1;
        Ok(Some(OperationDataBuf::new(buf)))
    }
}

// return predetermined vec of buffs
pub struct VecOperationDataStream<T: OperationData> {
    v: Mutex<Cell<Vec<T>>>,
}

impl<T: OperationData> VecOperationDataStream<T> {
    pub fn new(data: Vec<T>) -> Self {
        Self {
            v: Mutex::new(Cell::new(data)),
        }
    }
}

impl<T: OperationData> OperationDataStream for VecOperationDataStream<T> {
    async fn get_next(
        &self,
        _: CancellationToken,
    ) -> mssf_core::Result<Option<impl OperationData>> {
        let next = self.v.lock().unwrap().get_mut().pop();
        Ok(next)
    }
}

// file backed DB with a single content
pub struct SingleDB {
    lsn: FileLsn,
    data: FileState,
}

// managing file content
pub struct FileState {
    p: PathBuf,
}

pub struct FileLsn(pub FileState);

impl FileLsn {
    pub async fn create_or_attach(path: &Path) -> std::io::Result<Self> {
        let fs = FileState::create_or_attach(path).await?;
        Ok(Self(fs))
    }

    // write 0
    pub async fn set(&self, lsn: i64) -> std::io::Result<()> {
        let data = lsn.to_string();
        self.0.write_content(data).await
    }
    pub async fn get(&self) -> std::io::Result<i64> {
        let data = self.0.get_content().await?;
        let lsn = data.parse().expect("not a number");
        Ok(lsn)
    }
}

impl FileState {
    pub async fn create_or_attach(path: &Path) -> std::io::Result<Self> {
        // create file if not created.
        let _ = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .await?;
        Ok(Self { p: path.to_owned() })
    }

    // read all content
    pub async fn get_content(&self) -> std::io::Result<String> {
        let contents = tokio::fs::read_to_string(self.p.as_path()).await?;
        Ok(contents)
    }

    // overwrite all content
    pub async fn write_content(&self, contents: String) -> std::io::Result<()> {
        tokio::fs::write(self.p.as_path(), contents).await
    }
}

impl SingleDB {
    pub async fn create_or_attach(dir: &Path) -> std::io::Result<Self> {
        let p_lsn = dir.join("lsn.txt");
        let p_data = dir.join("data.txt");
        let f_lsn = FileLsn::create_or_attach(&p_lsn).await?;
        let f_data = FileState::create_or_attach(&p_data).await?;
        Ok(Self {
            lsn: f_lsn,
            data: f_data,
        })
    }

    // TODO: handle failure.
    pub async fn set_entry(&self, lsn: i64, data: String) -> std::io::Result<()> {
        self.lsn.set(lsn).await?;
        self.data.write_content(data).await
    }

    pub async fn get_entry(&self) -> std::io::Result<(i64, String)> {
        let lsn = self.lsn.get().await?;
        let data = self.data.get_content().await?;
        Ok((lsn, data))
    }
}

#[cfg(test)]
mod test {
    use std::env::temp_dir;

    use tokio::fs::create_dir_all;

    use crate::data::{FileLsn, FileState, SingleDB};

    #[tokio::test]
    async fn test_file_state() {
        let mut temp_dir = temp_dir();
        temp_dir.push("kvmaptest");
        // create test dir
        create_dir_all(&temp_dir).await.unwrap();

        let temp = temp_dir.join("kvmap.db");
        println!("using temp file: {}", temp.display());
        {
            let f = FileState::create_or_attach(&temp).await.unwrap();

            let content = "mycontent";
            f.write_content(content.to_string()).await.unwrap();
            let out = f.get_content().await.unwrap();
            assert_eq!(out, content)
        }

        // try attach the db
        {
            let f = FileState::create_or_attach(&temp).await.unwrap();
            let out = f.get_content().await.unwrap();
            assert_eq!(out, "mycontent")
        }

        // file lsn
        {
            let flsn = FileLsn::create_or_attach(&temp).await.unwrap();
            flsn.set(0).await.unwrap();
            assert_eq!(flsn.get().await.unwrap(), 0);
            flsn.set(99).await.unwrap();
            assert_eq!(flsn.get().await.unwrap(), 99);
        }

        // db
        {
            let db = SingleDB::create_or_attach(&temp_dir).await.unwrap();
            db.set_entry(0, "data1".to_string()).await.unwrap();
            let (lsn, data1) = db.get_entry().await.unwrap();
            assert_eq!(lsn, 0);
            assert_eq!(data1, "data1");
        }
    }
}
