pub trait Hardware {
    fn uptime_ms(&self) -> u64;
    fn serial_log(&self, msg: &str);
}