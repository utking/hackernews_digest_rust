use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Error, Seek, Write},
};

use chrono::DateTime;

pub trait FileStorage {
    fn query_ids(&self, source: &str) -> Vec<i64>;
    fn insert_items(&mut self, items: &[Record]) -> Result<(), Error>;
    fn dump(&mut self) -> Result<(), Error>;
    fn vacuum(&mut self, retain_days: i64) -> Result<usize, Error>;
    fn from_fs(file: File) -> Self;
}

pub struct Storage {
    pub records: Vec<Record>,
    file: File,
}

#[derive(Clone, Debug)]
pub struct Record {
    pub id: i64,
    pub source: String,
    pub created_at: i64,
}

impl From<String> for Record {
    #[must_use]
    fn from(item: String) -> Self {
        match item.split(',').collect::<Vec<&str>>().as_slice() {
            [id, source, created_at] => Self {
                id: id.parse().unwrap_or(0),
                source: (*source).to_string(),
                created_at: created_at.parse().unwrap_or(0),
            },
            _ => Self {
                id: 0,
                source: String::new(),
                created_at: 0,
            },
        }
    }
}

impl Into<String> for Record {
    #[must_use]
    fn into(self) -> String {
        format!("{},{},{}", self.id, self.source, self.created_at)
    }
}

impl Storage {
    /// Add a record to the storage. If the record already exists, an error will
    /// be returned.
    fn add(&mut self, record: &Record) -> Result<(), Error> {
        if !self.exists(record.id, &record.source) {
            self.records.push(record.clone());
            return Ok(());
        }
        Err(Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "Record already exists: {id}_{source}",
                id = record.id,
                source = record.source
            ),
        ))
    }

    /// Remove a record from the storage. If the record does not exist, nothing
    /// will happen.
    fn remove(&mut self, id: i64, source: &str) {
        self.records.retain(|r| r.id != id || r.source != source);
    }

    /// Check if a record exists in the storage - based on the id and source
    /// as the composite unique key.
    fn exists(&self, id: i64, source: &str) -> bool {
        self.records
            .iter()
            .any(|r| r.id == id && r.source == source)
    }

    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.records.len()
    }

    #[allow(dead_code)]
    /// Reload records from the file
    fn reload(&mut self) -> Result<(), Error> {
        self.records.clear(); // clear the records before reloading
        self.file.seek(std::io::SeekFrom::Start(0))?; // rewind the file to the beginning
        let reader = BufReader::new(&self.file);
        reader
            .lines()
            .map_while(Result::ok)
            .map(Record::from)
            .for_each(|r| self.records.push(r));
        Ok(())
    }
}

impl FileStorage for Storage {
    #[must_use]
    /// Load records from a file. If the file does not exist, it will be created
    fn from_fs(file: File) -> Self
    where
        Self: Sized,
    {
        let mut new_storage = Self {
            records: Vec::new(),
            file,
        };

        let mut records = Vec::new();
        let reader = BufReader::new(&new_storage.file);
        for line in reader.lines() {
            let record = Record::from(line.unwrap());
            records.push(record);
        }
        new_storage.records = records;
        new_storage
    }

    /// Query the storage for a list of ids based on the source part of the
    /// composite unique key.
    fn query_ids(&self, source: &str) -> Vec<i64> {
        self.records
            .iter()
            .filter(|r| r.source == source)
            .map(|r| r.id)
            .collect()
    }

    /// Dump records to a file
    fn dump(&mut self) -> Result<(), Error> {
        // rewind the file to the beginning to start anew
        self.file
            .seek(std::io::SeekFrom::Start(0))
            .expect("Failed to seek to the beginning");
        let mut writer = BufWriter::new(&self.file);
        for record in &self.records {
            let line: String = (*record).clone().into();
            _ = writer
                .write(format!("{line}\n").as_bytes())
                .expect("Failed to write to file");
        }
        writer.flush().expect("Failed to flush the writer");
        Ok(())
    }

    /// Insert a list of items into the storage. If any of the items already exists,
    /// the operation will be aborted and the storage will be left unchanged.
    /// If an error occurs, the already inserted items will be removed (transaction
    /// revert).
    fn insert_items(&mut self, items: &[Record]) -> Result<(), Error> {
        let mut items_done = 0;
        for item in items {
            match self.add(item) {
                Ok(()) => {
                    items_done += 1;
                }
                Err(e) => {
                    // undo the items that were added
                    for undo_item in items {
                        if items_done > 0 {
                            items_done -= 1;
                            self.remove(undo_item.id, &undo_item.source);
                        } else {
                            break;
                        }
                    }
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Remove records that are older than the specified number of days.
    fn vacuum(&mut self, retain_days: i64) -> Result<usize, Error> {
        let now = chrono::Utc::now();
        let oldest = now - chrono::Duration::days(retain_days);
        let records_num = self.records.len();
        self.records
            .retain(|r| match DateTime::from_timestamp(r.created_at, 0) {
                Some(created_at) => {
                    let created_at =
                        chrono::TimeZone::from_utc_datetime(now.offset(), &created_at.naive_utc());
                    created_at > oldest
                }
                None => false,
            });
        self.dump()?;
        Ok(records_num - self.records.len())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_storage() {
        let mut storage = Storage {
            records: Vec::new(),
            file: tempfile::tempfile().unwrap(),
        };
        let record = Record {
            id: 1,
            source: "test".to_string(),
            created_at: 0,
        };
        storage.add(&record).unwrap();
        assert_eq!(storage.len(), 1);
        assert_eq!(storage.exists(1, "test"), true);
        assert_eq!(storage.exists(2, "test"), false);
        assert_eq!(storage.exists(3, "test2"), false);
        assert_eq!(storage.query_ids("test"), vec![1]);
        storage.remove(1, "test");
        assert_eq!(storage.len(), 0);
        assert_eq!(storage.exists(1, "test"), false);
    }

    #[test]
    fn test_record_from_string() {
        let record = Record::from("1,test,0".to_string());
        assert_eq!(record.id, 1);
        assert_eq!(record.source, "test");
        assert_eq!(record.created_at, 0);
    }

    #[test]
    fn test_record_into_string() {
        let record = Record {
            id: 1,
            source: "test".to_string(),
            created_at: 0,
        };
        let record_str: String = record.into();
        assert_eq!(record_str, "1,test,0");
    }

    #[test]
    fn test_adding_existing_record() {
        let mut storage = Storage {
            records: Vec::new(),
            file: tempfile::tempfile().unwrap(),
        };
        let record = Record {
            id: 1,
            source: "test".to_string(),
            created_at: 0,
        };
        storage.add(&record).unwrap();
        assert_eq!(storage.len(), 1);
        let result = storage.add(&record);
        assert!(result.is_err());
        assert_eq!(storage.len(), 1);
    }

    #[test]
    fn test_adding_and_removing_multiple_records() {
        let mut storage = Storage {
            records: Vec::new(),
            file: tempfile::tempfile().unwrap(),
        };
        let records = vec![
            Record {
                id: 1,
                source: "test".to_string(),
                created_at: 0,
            },
            Record {
                id: 2,
                source: "test".to_string(),
                created_at: 0,
            },
            Record {
                id: 3,
                source: "test".to_string(),
                created_at: 0,
            },
        ];
        storage.insert_items(&records).unwrap();
        assert_eq!(storage.len(), 3);
        assert_eq!(storage.exists(1, "test"), true);
        assert_eq!(storage.exists(2, "test"), true);
        assert_eq!(storage.exists(3, "test"), true);
        storage.remove(1, "test");
        assert_eq!(storage.len(), 2);
        assert_eq!(storage.exists(1, "test"), false);
        storage.remove(2, "test");
        assert_eq!(storage.len(), 1);
        assert_eq!(storage.exists(2, "test"), false);
        storage.remove(3, "test");
        assert_eq!(storage.len(), 0);
        assert_eq!(storage.exists(3, "test"), false);
        storage.remove(3, "test");
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn test_vacuum() {
        let mut storage = Storage {
            records: Vec::new(),
            file: tempfile::tempfile().unwrap(),
        };
        let one_day_ago = chrono::Utc::now() - chrono::Duration::days(1);
        let two_days_ago = chrono::Utc::now() - chrono::Duration::days(2);
        let records = vec![
            Record {
                id: 1,
                source: "test".to_string(),
                created_at: one_day_ago.timestamp(),
            },
            Record {
                id: 2,
                source: "test".to_string(),
                created_at: two_days_ago.timestamp(),
            },
            Record {
                id: 3,
                source: "test".to_string(),
                created_at: 0,
            },
        ];
        storage.insert_items(&records).unwrap();
        assert_eq!(storage.len(), 3);
        storage.vacuum(7).unwrap();
        assert_eq!(storage.len(), 2);
        storage.vacuum(2).unwrap();
        assert_eq!(storage.len(), 1);
        storage.vacuum(1).unwrap();
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn test_query_ids() {
        let tmp_file = tempfile::tempfile().unwrap();
        let mut storage = Storage::from_fs(tmp_file);
        let records = vec![
            Record {
                id: 1,
                source: "rss".to_string(),
                created_at: 0,
            },
            Record {
                id: 2,
                source: "api".to_string(),
                created_at: 0,
            },
            Record {
                id: 3,
                source: "rss".to_string(),
                created_at: 0,
            },
        ];
        storage.insert_items(&records).unwrap();
        assert_eq!(storage.query_ids("rss"), vec![1, 3]);
        assert_eq!(storage.query_ids("api"), vec![2]);
    }

    #[test]
    fn test_dump_file() {
        let tmp_file = tempfile::tempfile().unwrap();
        let mut storage = Storage::from_fs(tmp_file);
        let records = vec![
            Record {
                id: 1,
                source: "rss".to_string(),
                created_at: 0,
            },
            Record {
                id: 2,
                source: "api".to_string(),
                created_at: 0,
            },
            Record {
                id: 3,
                source: "rss".to_string(),
                created_at: 0,
            },
        ];
        storage.insert_items(&records).unwrap();
        storage.dump().unwrap();

        storage.reload().unwrap();
        assert_eq!(storage.len(), 3);
        assert_eq!(storage.query_ids("rss"), vec![1, 3]);
        assert_eq!(storage.query_ids("api"), vec![2]);
    }
}
