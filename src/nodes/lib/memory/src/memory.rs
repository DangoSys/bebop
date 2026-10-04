pub trait Memory {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn read_buffer(&self, offset: usize, output: &mut [u8]);
    fn write_buffer(&self, offset: usize, input: &[u8]);
    fn fill(&self, offset: usize, bytes: usize, value: u8);
}
