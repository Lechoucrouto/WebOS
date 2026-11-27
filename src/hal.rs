use alloc::vec::Vec;

pub trait Hardware {
    fn uptime_ms(&self) -> u64;
    fn serial_log(&self, msg: &str);
    fn storage_save(&self, data: &[u8]);
    fn storage_load(&self) -> Option<Vec<u8>>;
    fn factory_reset(&self);
}
