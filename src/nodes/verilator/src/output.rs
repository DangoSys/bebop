use bebop_elf::OutputCapture;
use std::{path::Path, sync::Mutex};

static OUTPUT: Mutex<Option<OutputCapture>> = Mutex::new(None);

pub fn initialize_output(path: &Path) -> Result<(), String> {
    *OUTPUT.lock().expect("output capture poisoned") = OutputCapture::from_elf(path)?;
    Ok(())
}

pub fn captured_output() -> Result<Option<Vec<u8>>, String> {
    OUTPUT
        .lock()
        .expect("output capture poisoned")
        .as_ref()
        .map(|output| output.data().map(<[u8]>::to_vec))
        .transpose()
}

#[no_mangle]
pub extern "C" fn bbsim_output_word(hart: u32, word: u32) -> bool {
    OUTPUT
        .lock()
        .expect("output capture poisoned")
        .as_mut()
        .is_some_and(|output| output.push(hart as usize, word).expect("invalid output capture"))
}
