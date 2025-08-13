use std::sync::Arc;

use chrono::Utc;
use parking_lot::Mutex;
use serde::Deserialize;

const BIT_LEN_TIME: u64 = 39;
const BIT_LEN_SEQUENCE: u64 = 12;
const BIT_LEN_DATA_CENTER_ID: u64 = 8;
const BIT_LEN_MACHINE_ID: u64 = 63 - BIT_LEN_TIME - BIT_LEN_SEQUENCE - BIT_LEN_DATA_CENTER_ID;
const GENERATE_MASK_SEQUENCE: u16 = (1 << BIT_LEN_SEQUENCE) - 1;

#[derive(Debug, thiserror::Error)]
pub enum SnowflakeError {
    #[error("Mutex poisoned")]
    MutexPoisoned,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SnowflakeConfig {
    pub worker_id: u8,
    pub datacenter_id: u8,
    pub epoch: u64, // Timestamp in milliseconds
}

#[inline(always)]
fn unix_timestamp_ms() -> u64 {
    Utc::now().timestamp_millis() as u64
}

struct Internals {
    last_timestamp: u64,
    sequence: u16,
}

struct SnowflakeInner {
    epoch: u64,
    data_center_id: u8,
    worker_id: u8,
    internals: Mutex<Internals>,
}

pub struct Snowflake(Arc<SnowflakeInner>);

impl Snowflake {
    pub fn new(data_center_id: u8, worker_id: u8, epoch: u64) -> Self {
        let sequence = 0;
        let last_timestamp = 0;

        Snowflake(Arc::new(SnowflakeInner {
            epoch,
            data_center_id,
            worker_id,
            internals: Mutex::new(Internals {
                last_timestamp,
                sequence,
            }),
        }))
    }

    pub fn new_from_config(config: &SnowflakeConfig) -> Self {
        Self::new(config.datacenter_id, config.worker_id, config.epoch)
    }

    pub fn next_id(&self) -> Result<u64, SnowflakeError> {
        let mut internals = self.0.internals.lock();
        let mut timestamp = unix_timestamp_ms();

        if timestamp == internals.last_timestamp {
            internals.sequence = (internals.sequence + 1) & GENERATE_MASK_SEQUENCE;

            if internals.sequence == 0 {
                while timestamp <= internals.last_timestamp {
                    timestamp = unix_timestamp_ms();
                }
            }
        } else {
            internals.sequence = 0;
        }

        internals.last_timestamp = timestamp;

        Ok((timestamp - self.0.epoch)
            << (BIT_LEN_SEQUENCE + BIT_LEN_MACHINE_ID + BIT_LEN_DATA_CENTER_ID)
            | (internals.sequence as u64) << (BIT_LEN_MACHINE_ID + BIT_LEN_DATA_CENTER_ID)
            | (self.0.data_center_id as u64) << BIT_LEN_MACHINE_ID
            | (self.0.worker_id as u64))
    }
}

impl Clone for Snowflake {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
