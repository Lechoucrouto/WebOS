pub trait Hardware {
    fn now_ms(&self) -> f64;
    fn serial_log(&self, msg: &str);
    fn storage_save(&self, data: &str);
    fn storage_load(&self) -> Option<String>;
    fn factory_reset(&self);
}