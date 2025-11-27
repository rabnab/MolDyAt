pub trait Window<E> {
    fn get_size(&self) -> (u32, u32);
    // Input and events
    fn create_window(&self);
}
