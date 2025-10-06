// Compute unit for matrix multiplication

pub struct ComputeUnit {
  pub cycle_count: u64,
}

impl ComputeUnit {
  pub fn new() -> Self {
    Self { cycle_count: 0 }
  }

  pub fn matmul(
    &mut self,
    a: &[f32],
    b: &[f32],
    c: &mut [f32],
    m: usize,
    n: usize,
    k: usize,
  ) -> Result<(), String> {
    if a.len() < m * k {
      return Err(format!("Matrix A size {} < m*k={}", a.len(), m * k));
    }
    if b.len() < k * n {
      return Err(format!("Matrix B size {} < k*n={}", b.len(), k * n));
    }
    if c.len() < m * n {
      return Err(format!("Matrix C size {} < m*n={}", c.len(), m * n));
    }

    // C = A * B, where A is m×k, B is k×n, C is m×n
    for i in 0..m {
      for j in 0..n {
        let mut sum = 0.0;
        for p in 0..k {
          sum += a[i * k + p] * b[p * n + j];
        }
        c[i * n + j] = sum;
      }
    }

    // Simulate compute cycles (naive estimation)
    self.cycle_count += (m * n * k) as u64;
    Ok(())
  }

  pub fn reset_cycles(&mut self) {
    self.cycle_count = 0;
  }

  pub fn get_cycles(&self) -> u64 {
    self.cycle_count
  }
}

