use std::sync::{Arc, Mutex};

static MEM16_CB: Mutex<Option<Arc<dyn Fn(u64) -> [u8; 16] + Send + Sync>>> = Mutex::new(None);
static MEM16_WRITER: Mutex<Option<Arc<dyn Fn(u64, [u8; 16]) + Send + Sync>>> = Mutex::new(None);

pub fn set_mem16_reader(f: impl Fn(u64) -> [u8; 16] + Send + Sync + 'static) {
    *MEM16_CB.lock().unwrap() = Some(Arc::new(f));
}

pub fn set_mem16_writer(f: impl Fn(u64, [u8; 16]) + Send + Sync + 'static) {
    *MEM16_WRITER.lock().unwrap() = Some(Arc::new(f));
}

#[no_mangle]
pub extern "C" fn bebop_rust_mem_read16(addr: u64, lo: *mut u64, hi: *mut u64) {
    let cb = MEM16_CB
        .lock()
        .unwrap()
        .clone()
        .expect("bebop_rust_mem_read16: mem reader not set");
    let b = cb(addr);
    let mut lov = 0u64;
    let mut hiv = 0u64;
    for i in 0..8 {
        lov |= (b[i] as u64) << (8 * i);
    }
    for i in 0..8 {
        hiv |= (b[i + 8] as u64) << (8 * i);
    }
    unsafe {
        *lo = lov;
        *hi = hiv;
    }
}

#[no_mangle]
pub extern "C" fn bebop_rust_mem_write16(addr: u64, lo: u64, hi: u64) {
    let mut b = [0u8; 16];
    for i in 0..8 {
        b[i] = ((lo >> (8 * i)) & 0xff) as u8;
    }
    for i in 0..8 {
        b[i + 8] = ((hi >> (8 * i)) & 0xff) as u8;
    }
    let cb = MEM16_WRITER
        .lock()
        .unwrap()
        .clone()
        .expect("bebop_rust_mem_write16: mem writer not set");
    cb(addr, b);
}

#[no_mangle]
pub extern "C" fn bebop_dpi_quant_u8(b0: u8, b1: u8, b2: u8, b3: u8, scale_bits: u32) -> u8 {
    let v = i32::from_le_bytes([b0, b1, b2, b3]);
    let scale = f32::from_bits(scale_bits);
    let q = ((v as f32) * scale).round().clamp(-128.0, 127.0) as i8;
    q as u8
}

#[no_mangle]
pub extern "C" fn bebop_dpi_dequant_i32_le(
    v_i8: u8,
    scale_bits: u32,
    o0: *mut u8,
    o1: *mut u8,
    o2: *mut u8,
    o3: *mut u8,
) {
    let v = v_i8 as i8;
    let scale = f32::from_bits(scale_bits);
    let o = ((v as f32) * scale).round() as i32;
    let le = o.to_le_bytes();
    unsafe {
        *o0 = le[0];
        *o1 = le[1];
        *o2 = le[2];
        *o3 = le[3];
    }
}
