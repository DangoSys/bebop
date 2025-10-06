// Ball Decoder: decodes ball-level operations (matmul details)

#[derive(Debug, Clone)]
pub struct MatmulOp {
  pub a_addr: u64,
  pub b_addr: u64,
  pub c_addr: u64,
  pub m: usize,
  pub n: usize,
  pub k: usize,
}

pub struct BallDecoder;

impl BallDecoder {
  pub fn new() -> Self {
    Self
  }

  pub fn decode_matmul(
    &self,
    a_addr: u64,
    b_addr: u64,
    c_addr: u64,
    m: usize,
    n: usize,
    k: usize,
  ) -> Result<MatmulOp, String> {
    // Validate parameters at ball level
    if m == 0 || n == 0 || k == 0 {
      return Err("Matrix dimensions must be > 0".to_string());
    }

    println!(
      "[BallDecoder] Decoded matmul operation: {}×{} * {}×{} -> {}×{}",
      m, k, k, n, m, n
    );

    Ok(MatmulOp {
      a_addr,
      b_addr,
      c_addr,
      m,
      n,
      k,
    })
  }
}

