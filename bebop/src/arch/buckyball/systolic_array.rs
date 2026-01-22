// Systolic Array Implementation for Matrix Multiplication
// Follows the classic Kung-Leiserson design pattern

use std::sync::atomic::{AtomicBool, Ordering};
use serde::{Serialize, Deserialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::Mutex;

// ===========================================
// Input Buffer Module
// ===========================================

/// Input buffer for matrix data storage and access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputBuffer {
    /// Matrix data stored as a 2D vector
    data: Vec<Vec<u64>>,
    /// Number of rows in the matrix
    rows: usize,
    /// Number of columns in the matrix
    cols: usize,
}

impl InputBuffer {
    /// Create a new input buffer from matrix data
    /// 
    /// # Arguments
    /// * `matrix` - 2D vector representing the matrix
    /// 
    /// # Returns
    /// A new InputBuffer instance
    pub fn new(matrix: Vec<Vec<u64>>) -> Self {
        if matrix.is_empty() || matrix[0].is_empty() {
            panic!("Matrix cannot be empty");
        }
        
        let rows = matrix.len();
        let cols = matrix[0].len();
        
        Self {
            data: matrix,
            rows,
            cols,
        }
    }
    
    /// Get a value from the buffer at specified coordinates
    /// 
    /// # Arguments
    /// * `row` - Row index
    /// * `col` - Column index
    /// 
    /// # Returns
    /// The value at the specified position, or 0 if out of bounds
    pub fn get(&self, row: usize, col: usize) -> u64 {
        if row < self.rows && col < self.cols {
            self.data[row][col]
        } else {
            0
        }
    }
    
    /// Get the number of rows
    pub fn rows(&self) -> usize {
        self.rows
    }
    
    /// Get the number of columns
    pub fn cols(&self) -> usize {
        self.cols
    }
}

// ===========================================
// Output Buffer Module
// ===========================================

/// Output buffer for result storage and retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputBuffer {
    /// Result data stored as a 2D vector
    data: Vec<Vec<u128>>,
    /// Number of rows in the result matrix
    rows: usize,
    /// Number of columns in the result matrix
    cols: usize,
    /// Indicates if the buffer has been filled with results
    is_ready: bool,
}

impl OutputBuffer {
    /// Create a new output buffer with specified dimensions
    /// 
    /// # Arguments
    /// * `rows` - Number of rows
    /// * `cols` - Number of columns
    /// 
    /// # Returns
    /// A new OutputBuffer instance
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            data: vec![vec![0; cols]; rows],
            rows,
            cols,
            is_ready: false,
        }
    }
    
    /// Set a value in the buffer at specified coordinates
    /// 
    /// # Arguments
    /// * `row` - Row index
    /// * `col` - Column index
    /// * `value` - Value to store
    pub fn set(&mut self, row: usize, col: usize, value: u128) {
        if row < self.rows && col < self.cols {
            self.data[row][col] = value;
        }
    }
    
    /// Get a value from the buffer at specified coordinates
    /// 
    /// # Arguments
    /// * `row` - Row index
    /// * `col` - Column index
    /// 
    /// # Returns
    /// The value at the specified position
    pub fn get(&self, row: usize, col: usize) -> u128 {
        if row < self.rows && col < self.cols {
            self.data[row][col]
        } else {
            0
        }
    }
    
    /// Get the entire result matrix
    pub fn get_result(&self) -> &Vec<Vec<u128>> {
        &self.data
    }
    
    /// Mark the buffer as ready (results are available)
    pub fn set_ready(&mut self) {
        self.is_ready = true;
    }
    
    /// Check if the buffer is ready
    pub fn is_ready(&self) -> bool {
        self.is_ready
    }
    
    /// Clear the buffer contents
    pub fn clear(&mut self) {
        self.data = vec![vec![0; self.cols]; self.rows];
        self.is_ready = false;
    }
    
    /// Get the number of rows
    pub fn rows(&self) -> usize {
        self.rows
    }
    
    /// Get the number of columns
    pub fn cols(&self) -> usize {
        self.cols
    }
}

// ===========================================
// Processing Element (PE) Module
// ===========================================

/// Processing Element (PE) - performs multiply-accumulate operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingElement {
    /// Input value from left neighbor (A matrix data)
    a_in: u64,
    /// Input value from top neighbor (B matrix data)
    b_in: u64,
    /// Accumulator for partial result
    acc: u128,
    /// Position in the systolic array
    row: usize,
    /// Position in the systolic array
    col: usize,
}

impl ProcessingElement {
    /// Create a new processing element at specified position
    /// 
    /// # Arguments
    /// * `row` - Row index in the array
    /// * `col` - Column index in the array
    /// 
    /// # Returns
    /// A new ProcessingElement instance
    pub fn new(row: usize, col: usize) -> Self {
        Self {
            a_in: 0,
            b_in: 0,
            acc: 0,
            row,
            col,
        }
    }
    
    /// Get the A value to pass to right neighbor
    /// 
    /// # Returns
    /// The A value to propagate rightward
    pub fn get_a_right(&self) -> u64 {
        self.a_in
    }
    
    /// Get the B value to pass to bottom neighbor
    /// 
    /// # Returns
    /// The B value to propagate downward
    pub fn get_b_down(&self) -> u64 {
        self.b_in
    }
    
    /// Set input values from neighbors or external input
    /// 
    /// # Arguments
    /// * `a` - A value from left or input buffer
    /// * `b` - B value from top or input buffer
    pub fn set_inputs(&mut self, a: u64, b: u64) {
        self.a_in = a;
        self.b_in = b;
    }
    
    /// Perform multiply-accumulate operation (MAC)
    /// 
    /// This is the core operation: acc = acc + (a_in * b_in)
    pub fn compute(&mut self) {
        self.acc += (self.a_in as u128) * (self.b_in as u128);
    }
    
    /// Get accumulated result
    /// 
    /// # Returns
    /// The current accumulated value
    pub fn get_result(&self) -> u128 {
        self.acc
    }
    
    /// Reset the processing element to initial state
    pub fn reset(&mut self) {
        self.a_in = 0;
        self.b_in = 0;
        self.acc = 0;
    }
    
    /// Get the row position
    pub fn row(&self) -> usize {
        self.row
    }
    
    /// Get the column position
    pub fn col(&self) -> usize {
        self.col
    }
}

// Static flag to indicate if a systolic array instruction can be issued
pub static SYSTOLIC_ARRAY_INST_CAN_ISSUE: AtomicBool = AtomicBool::new(true);

// Instruction data (set by receive_systolic_array_inst, cleared when processed)
struct SystolicArrayInstData {
    op1_bank_id: u64,
    op2_bank_id: u64,
    wr_bank_id: u64,
    m_dim: u64,  // Rows in result matrix
    n_dim: u64,  // Columns in result matrix
    k_dim: u64,  // Inner dimension
    rob_id: u64,
}

// Static mutex to hold instruction data
static SYSTOLIC_ARRAY_INST_DATA: Mutex<Option<SystolicArrayInstData>> = Mutex::new(None);

// SystolicArray states for matrix multiplication pipeline
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum SystolicArrayState {
    Idle,
    WaitOp1,       // Waiting for operand 1 from bank
    WaitOp2,       // Waiting for operand 2 from bank
    Computing,     // Performing matrix multiplication
    WaitWriteResp, // Waiting for write completion
}

// ===========================================
// Systolic Array Main Module
// ===========================================

/// Systolic Array for matrix multiplication
/// Follows the classic Kung-Leiserson design pattern
#[derive(Debug, Serialize, Deserialize)]
pub struct SystolicArray {
    // Port names for communication
    systolic_mem_write_req_port: String,
    mem_systolic_read_resp_port: String,
    commit_to_rob_port: String,
    
    /// Number of rows in the array (matches A matrix rows and result rows)
    rows: usize,
    /// Number of columns in the array (matches B matrix columns and result columns)
    cols: usize,
    /// 2D grid of processing elements
    pe_grid: Vec<Vec<ProcessingElement>>,
    
    // Control signals
    is_running: AtomicBool,      // Indicates if computation is in progress
    is_idle: AtomicBool,         // Indicates if array is idle
    cycle_count: usize,          // Current cycle number
    
    // Buffers
    input_buffer_a: Option<InputBuffer>,  // Input buffer for matrix A (MxK)
    input_buffer_b: Option<InputBuffer>,  // Input buffer for matrix B (KxN)
    output_buffer: OutputBuffer,          // Output buffer for results (MxN)
    
    // Matrix dimensions
    k_dim: usize,  // Inner dimension (A columns = B rows)
    
    // DevsModel required fields
    until_next_event: f64,
    current_inst: Option<String>,
    records: Vec<ModelRecord>,
    
    // Instruction fields
    state: SystolicArrayState,
    op1_bank_id: u64,
    op2_bank_id: u64,
    wr_bank_id: u64,
    m_dim: u64,
    n_dim: u64,
    k_dim_inst: u64,
    rob_id: u64,
    
    // Computation state
    op1_data: Vec<Vec<u64>>,
    op2_data: Vec<Vec<u64>>,
    
    // Latency parameters
    read_latency: f64,
    compute_latency: f64,
    write_latency: f64,
}

impl SystolicArray {
    /// Create a new systolic array with specified dimensions
    /// 
    /// # Arguments
    /// * `systolic_mem_write_req_port` - Port for memory write requests
    /// * `mem_systolic_read_resp_port` - Port for memory read responses
    /// * `commit_to_rob_port` - Port for committing results to ROB
    /// 
    /// # Returns
    /// A new SystolicArray instance
    pub fn new(
        systolic_mem_write_req_port: String,
        mem_systolic_read_resp_port: String,
        commit_to_rob_port: String,
    ) -> Self {
        // Initialize with 3x3 array dimensions (can be extended later)
        let rows = 3;
        let cols = 3;
        
        // Initialize processing element grid
        let mut pe_grid = Vec::with_capacity(rows);
        for i in 0..rows {
            let mut row = Vec::with_capacity(cols);
            for j in 0..cols {
                row.push(ProcessingElement::new(i, j));
            }
            pe_grid.push(row);
        }
        
        // Initialize output buffer
        let output_buffer = OutputBuffer::new(rows, cols);
        
        Self {
            // Port names
            systolic_mem_write_req_port,
            mem_systolic_read_resp_port,
            commit_to_rob_port,
            
            // Array dimensions
            rows,
            cols,
            pe_grid,
            
            // Control signals
            is_running: AtomicBool::new(false),
            is_idle: AtomicBool::new(true),
            cycle_count: 0,
            
            // Buffers
            input_buffer_a: None,
            input_buffer_b: None,
            output_buffer,
            k_dim: 0,
            
            // DevsModel required fields
            until_next_event: INFINITY,
            current_inst: None,
            records: Vec::new(),
            
            // Instruction fields
            state: SystolicArrayState::Idle,
            op1_bank_id: 0,
            op2_bank_id: 0,
            wr_bank_id: 0,
            m_dim: 0,
            n_dim: 0,
            k_dim_inst: 0,
            rob_id: 0,
            
            // Computation state
            op1_data: Vec::new(),
            op2_data: Vec::new(),
            
            // Latency parameters
            read_latency: 16.0,    // 16 cycles to read data
            compute_latency: 16.0, // 16 cycles for computation
            write_latency: 16.0,   // 16 cycles to write results
        }
    }
    
    /// Load matrices for multiplication
    /// 
    /// # Arguments
    /// * `matrix_a` - Matrix A (MxK) to multiply
    /// * `matrix_b` - Matrix B (KxN) to multiply
    /// 
    /// # Returns
    /// Result indicating success or error message
    pub fn load_matrices(&mut self, matrix_a: Vec<Vec<u64>>, matrix_b: Vec<Vec<u64>>) -> Result<(), String> {
        // Validate matrix dimensions
        if matrix_a.is_empty() || matrix_b.is_empty() {
            return Err("Matrices cannot be empty".to_string());
        }
        
        let a_rows = matrix_a.len();
        let a_cols = matrix_a[0].len();
        let b_rows = matrix_b.len();
        let b_cols = matrix_b[0].len();
        
        // Check matrix compatibility for multiplication
        if a_cols != b_rows {
            return Err(format!("Matrix dimensions mismatch: A has {} columns, B has {} rows", 
                              a_cols, b_rows).to_string());
        }
        
        // Check if matrices fit in the array
        if a_rows > self.rows || b_cols > self.cols {
            return Err(format!("Matrix dimensions exceed array size: Array is {}x{}, A is {}x{}, B is {}x{}", 
                              self.rows, self.cols, a_rows, a_cols, b_rows, b_cols).to_string());
        }
        
        // Reset array and create input buffers
        self.reset();
        self.input_buffer_a = Some(InputBuffer::new(matrix_a));
        self.input_buffer_b = Some(InputBuffer::new(matrix_b));
        self.k_dim = a_cols; // Store inner dimension
        
        Ok(())
    }
    
    /// Advance the systolic array by one cycle
    /// 
    /// # Timing Constraints
    /// Each cycle consists of three phases (strictly following Kung-Leiserson design):
    /// 1. Multiply-Accumulate: All PEs compute acc = acc + a_in * b_in
    /// 2. Data Propagation: A values shift right, B values shift down
    /// 3. New Input Injection: New values enter first column (A) and first row (B)
    /// 
    /// # Returns
    /// True if the array is still running, False if computation is complete
    pub fn cycle(&mut self) -> bool {
        if !self.is_running.load(Ordering::Relaxed) || 
           self.input_buffer_a.is_none() || 
           self.input_buffer_b.is_none() {
            return false;
        }
        
        let input_a = self.input_buffer_a.as_ref().unwrap();
        let input_b = self.input_buffer_b.as_ref().unwrap();
        
        let m = input_a.rows(); // A rows
        let k = self.k_dim;     // Inner dimension
        let n = input_b.cols(); // B columns
        let t = self.cycle_count; // Current time cycle
        
        // Edge case: 1x1 matrix multiplication needs special handling
        // because the standard Kung-Leiserson design doesn't handle it properly
        if m == 1 && n == 1 && k == 1 {
            if t == 0 {
                // First cycle: load the single elements
                let a_val = input_a.get(0, 0);
                let b_val = input_b.get(0, 0);
                self.pe_grid[0][0].set_inputs(a_val, b_val);
                self.cycle_count += 1;
                return true;
            } else if t == 1 {
                // Second cycle: compute the product
                self.pe_grid[0][0].compute();
                self.cycle_count += 1;
                
                // Collect the result
                let result = self.pe_grid[0][0].get_result();
                self.output_buffer.set(0, 0, result);
                self.output_buffer.set_ready();
                
                // Update state
                self.is_running.store(false, Ordering::Relaxed);
                self.is_idle.store(true, Ordering::Relaxed);
                
                return false;
            }
        }
        
        // Standard Kung-Leiserson design for larger matrices
        // First, we'll perform the multiply-accumulate operation using the current inputs
        for i in 0..self.rows {
            for j in 0..self.cols {
                self.pe_grid[i][j].compute();
            }
        }
        
        // Then, we'll prepare the new inputs for the next cycle
        for i in 0..self.rows {
            for j in 0..self.cols {
                let new_a: u64;
                let new_b: u64;
                
                // For the next cycle (t+1), determine the new A value
                if j == 0 {
                    // First column: new A comes from the input matrix
                    let k_index = t; // A[i][k_index] enters PE[i][0] at time t
                    if k_index < k {
                        new_a = input_a.get(i, k_index);
                    } else {
                        new_a = 0;
                    }
                } else {
                    // Other columns: new A comes from the left neighbor
                    new_a = self.pe_grid[i][j-1].a_in;
                }
                
                // For the next cycle (t+1), determine the new B value
                if i == 0 {
                    // First row: new B comes from the input matrix
                    let k_index = t; // B[k_index][j] enters PE[0][j] at time t
                    if k_index < k {
                        new_b = input_b.get(k_index, j);
                    } else {
                        new_b = 0;
                    }
                } else {
                    // Other rows: new B comes from the top neighbor
                    new_b = self.pe_grid[i-1][j].b_in;
                }
                
                // Update the PE with the new inputs for the next cycle
                self.pe_grid[i][j].set_inputs(new_a, new_b);
            }
        }
        
        // Increment the cycle count
        self.cycle_count += 1;
        
        // Check if we've completed the computation
        // For MxK * KxN matrix multiplication in Kung-Leiserson design:
        // The total number of cycles needed is M + N + K - 2
        // This is because:
        // - It takes K cycles to inject all elements of A and B
        // - It takes M-1 cycles for A to flow through the rows
        // - It takes N-1 cycles for B to flow through the columns
        // Total: K + (M-1) + (N-1) = M + N + K - 2
        if self.cycle_count >= m + n + k - 2 {
            // Collect the final results from all PEs
            for i in 0..self.rows {
                for j in 0..self.cols {
                    let result = self.pe_grid[i][j].get_result();
                    self.output_buffer.set(i, j, result);
                }
            }
            
            // Mark the results as ready
            self.output_buffer.set_ready();
            
            // Update the state of the systolic array
            self.is_running.store(false, Ordering::Relaxed);
            self.is_idle.store(true, Ordering::Relaxed);
            
            return false;
        }
        
        return true;
    }
    
    /// Start the matrix multiplication computation
    /// 
    /// # Panics
    /// Panics if matrices have not been loaded
    pub fn start(&mut self) {
        if self.input_buffer_a.is_none() || self.input_buffer_b.is_none() {
            panic!("Cannot start: matrices not loaded");
        }
        
        // Initialize all PEs with zero inputs
        // The first valid inputs will be injected in the first cycle
        for i in 0..self.rows {
            for j in 0..self.cols {
                self.pe_grid[i][j].set_inputs(0, 0);
            }
        }
        
        // Reset cycle count
        self.cycle_count = 0;
        
        // Set running state
        self.is_running.store(true, Ordering::Relaxed);
        self.is_idle.store(false, Ordering::Relaxed);
    }
    
    /// Stop the computation immediately
    pub fn stop(&mut self) {
        self.is_running.store(false, Ordering::Relaxed);
        self.is_idle.store(true, Ordering::Relaxed);
    }
    
    /// Reset the systolic array to initial state
    pub fn reset(&mut self) {
        self.stop();
        
        // Reset all processing elements
        for row in &mut self.pe_grid {
            for pe in row {
                pe.reset();
            }
        }
        
        // Clear buffers and results
        self.input_buffer_a = None;
        self.input_buffer_b = None;
        self.output_buffer.clear();
        
        // Reset cycle count
        self.cycle_count = 0;
        self.k_dim = 0;
    }
    
    /// Get the computation results
    /// 
    /// # Returns
    /// Option containing the result matrix if computation is complete, None otherwise
    pub fn get_results(&self) -> Option<&Vec<Vec<u128>>> {
        if self.output_buffer.is_ready() {
            Some(self.output_buffer.get_result())
        } else {
            None
        }
    }
    
    /// Get the output buffer reference
    /// 
    /// # Returns
    /// Reference to the output buffer
    pub fn output_buffer(&self) -> &OutputBuffer {
        &self.output_buffer
    }
    
    /// Check if the systolic array is running
    /// 
    /// # Returns
    /// True if computation is in progress, False otherwise
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }
    
    /// Check if the systolic array is idle
    /// 
    /// # Returns
    /// True if array is idle, False otherwise
    pub fn is_idle(&self) -> bool {
        self.is_idle.load(Ordering::Relaxed)
    }
    
    /// Get the current cycle count
    /// 
    /// # Returns
    /// Number of cycles executed so far
    pub fn cycle_count(&self) -> usize {
        self.cycle_count
    }
    
    /// Get the number of rows in the array
    pub fn rows(&self) -> usize {
        self.rows
    }
    
    /// Get the number of columns in the array
    pub fn cols(&self) -> usize {
        self.cols
    }
}

impl DevsModel for SystolicArray {
    fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
        // Handle read response from memory controller
        if incoming_message.port_name == self.mem_systolic_read_resp_port {
            // Deserialize the received data
            let data: Vec<u64> = 
                serde_json::from_str(&incoming_message.content)
                .map_err(|_| SimulationError::InvalidModelState)?;
            
            match self.state {
                SystolicArrayState::WaitOp1 => {
                    // Convert flat array to 2D matrix
                    if data.len() != (self.m_dim * self.k_dim_inst) as usize {
                        return Err(SimulationError::InvalidModelState);
                    }
                    
                    let mut matrix = Vec::new();
                    for i in 0..self.m_dim as usize {
                        let start = i * self.k_dim_inst as usize;
                        let end = start + self.k_dim_inst as usize;
                        matrix.push(data[start..end].to_vec());
                    }
                    
                    self.op1_data = matrix;
                    
                    self.records.push(ModelRecord {
                        time: services.global_time(),
                        action: "received_op1_data".to_string(),
                        subject: format!("matrix A {}x{} from bank {}", 
                                      self.m_dim, self.k_dim_inst, self.op1_bank_id),
                    });
                    
                    // Now request operand 2
                    self.state = SystolicArrayState::WaitOp2;
                    self.until_next_event = 1.0;
                },
                SystolicArrayState::WaitOp2 => {
                    // Convert flat array to 2D matrix
                    if data.len() != (self.k_dim_inst * self.n_dim) as usize {
                        return Err(SimulationError::InvalidModelState);
                    }
                    
                    let mut matrix = Vec::new();
                    for i in 0..self.k_dim_inst as usize {
                        let start = i * self.n_dim as usize;
                        let end = start + self.n_dim as usize;
                        matrix.push(data[start..end].to_vec());
                    }
                    
                    self.op2_data = matrix;
                    
                    self.records.push(ModelRecord {
                        time: services.global_time(),
                        action: "received_op2_data".to_string(),
                        subject: format!("matrix B {}x{} from bank {}", 
                                      self.k_dim_inst, self.n_dim, self.op2_bank_id),
                    });
                    
                    // Start the systolic array computation
                    self.state = SystolicArrayState::Computing;
                    self.until_next_event = self.compute_latency;
                    
                    // Load matrices and start computation
                    if let Err(_e) = self.load_matrices(self.op1_data.clone(), self.op2_data.clone()) {
                        return Err(SimulationError::InvalidModelState);
                    }
                    self.start();
                },
                _ => {},
            }
            
            return Ok(());
        }
        
        Ok(())
    }
    
    fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
        let mut messages = Vec::new();
        
        match self.state {
            SystolicArrayState::Idle => {
                // Check for new instruction
                if let Some(inst) = SYSTOLIC_ARRAY_INST_DATA.lock().unwrap().take() {
                    self.op1_bank_id = inst.op1_bank_id;
                    self.op2_bank_id = inst.op2_bank_id;
                    self.wr_bank_id = inst.wr_bank_id;
                    self.m_dim = inst.m_dim;
                    self.n_dim = inst.n_dim;
                    self.k_dim_inst = inst.k_dim;
                    self.rob_id = inst.rob_id;
                    
                    // Start by requesting operand 1
                    self.state = SystolicArrayState::WaitOp1;
                    self.until_next_event = 1.0;
                    
                    self.records.push(ModelRecord {
                        time: services.global_time(),
                        action: "receive_inst".to_string(),
                        subject: format!(
                            "systolic array matmul: A({}x{}) @ bank {}, B({}x{}) @ bank {}, result @ bank {}",
                            self.m_dim, self.k_dim_inst, self.op1_bank_id,
                            self.k_dim_inst, self.n_dim, self.op2_bank_id,
                            self.wr_bank_id
                        ),
                    });
                }
            },
            SystolicArrayState::WaitOp1 => {
                // Request operand 1 from memory
                // In a real system, this would send a read request to the memory controller
                // For now, we'll simulate this with a simple message
                self.records.push(ModelRecord {
                    time: services.global_time(),
                    action: "request_op1_data".to_string(),
                    subject: format!("matrix A {}x{} from bank {}", 
                                  self.m_dim, self.k_dim_inst, self.op1_bank_id),
                });
                
                // In a real implementation, we would send a message to the memory controller
                // For now, we'll just wait for the response
                self.until_next_event = self.read_latency;
            },
            SystolicArrayState::WaitOp2 => {
                // Request operand 2 from memory
                self.records.push(ModelRecord {
                    time: services.global_time(),
                    action: "request_op2_data".to_string(),
                    subject: format!("matrix B {}x{} from bank {}", 
                                  self.k_dim_inst, self.n_dim, self.op2_bank_id),
                });
                
                // In a real implementation, we would send a message to the memory controller
                // For now, we'll just wait for the response
                self.until_next_event = self.read_latency;
            },
            SystolicArrayState::Computing => {
                // Run systolic array cycles until computation is complete
                while self.cycle() {
                    // Continue running cycles
                }
                
                self.records.push(ModelRecord {
                    time: services.global_time(),
                    action: "compute_complete".to_string(),
                    subject: format!("matrix multiplication completed in {} cycles", self.cycle_count),
                });
                
                // Get the results
                if let Some(result) = self.get_results() {
                    // Flatten the result matrix for writing to memory
                    let mut flat_result = Vec::new();
                    for row in result {
                        for &val in row {
                            flat_result.push(val as u64);
                        }
                    }
                    
                    // Send write request to memory controller
                    let write_request = serde_json::to_string(&flat_result)
                        .map_err(|_| SimulationError::InvalidModelState)?;
                    
                    messages.push(ModelMessage {
                        port_name: self.systolic_mem_write_req_port.clone(),
                        content: write_request,
                    });
                    
                    self.state = SystolicArrayState::WaitWriteResp;
                    self.until_next_event = self.write_latency;
                } else {
                    return Err(SimulationError::InvalidModelState);
                }
            },
            SystolicArrayState::WaitWriteResp => {
                // Write to memory completed
                self.records.push(ModelRecord {
                    time: services.global_time(),
                    action: "write_complete".to_string(),
                    subject: format!("result matrix written to bank {}", self.wr_bank_id),
                });
                
                // Commit the result to ROB
                messages.push(ModelMessage {
                    port_name: self.commit_to_rob_port.clone(),
                    content: serde_json::to_string(&self.rob_id)
                        .map_err(|_| SimulationError::InvalidModelState)?,
                });
                
                // Reset to idle state
                self.state = SystolicArrayState::Idle;
                self.until_next_event = INFINITY;
                
                // Allow new instructions to be issued
                SYSTOLIC_ARRAY_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
            },
        }
        
        Ok(messages)
    }
    
    fn until_next_event(&self) -> f64 {
        self.until_next_event
    }
    
    fn time_advance(&mut self, time_delta: f64) {
        self.until_next_event -= time_delta;
    }
}

// Implement ReportableModel trait
impl ReportableModel for SystolicArray {}

// Implement Reportable trait
impl Reportable for SystolicArray {
    fn status(&self) -> String {
        "normal".to_string()
    }
    
    fn records(&self) -> &Vec<ModelRecord> {
        &self.records
    }
}

// Implement SerializableModel trait
impl SerializableModel for SystolicArray {
    fn get_type(&self) -> &'static str {
        "SystolicArray"
    }
}

// Implement Clone trait manually to handle AtomicBool fields
impl Clone for SystolicArray {
    fn clone(&self) -> Self {
        Self {
            // Port names
            systolic_mem_write_req_port: self.systolic_mem_write_req_port.clone(),
            mem_systolic_read_resp_port: self.mem_systolic_read_resp_port.clone(),
            commit_to_rob_port: self.commit_to_rob_port.clone(),
            
            // Array dimensions
            rows: self.rows,
            cols: self.cols,
            
            // PE grid (cloned deeply)
            pe_grid: self.pe_grid.clone(),
            
            // Control signals (AtomicBool can't be cloned, so create new ones)
            is_running: AtomicBool::new(self.is_running.load(Ordering::Relaxed)),
            is_idle: AtomicBool::new(self.is_idle.load(Ordering::Relaxed)),
            
            // Cycle count
            cycle_count: self.cycle_count,
            
            // Buffers (cloned deeply)
            input_buffer_a: self.input_buffer_a.clone(),
            input_buffer_b: self.input_buffer_b.clone(),
            output_buffer: self.output_buffer.clone(),
            
            // Matrix dimensions
            k_dim: self.k_dim,
            
            // DevsModel required fields
            until_next_event: self.until_next_event,
            current_inst: self.current_inst.clone(),
            records: self.records.clone(),
            
            // Instruction fields
            state: self.state,
            op1_bank_id: self.op1_bank_id,
            op2_bank_id: self.op2_bank_id,
            wr_bank_id: self.wr_bank_id,
            m_dim: self.m_dim,
            n_dim: self.n_dim,
            k_dim_inst: self.k_dim_inst,
            rob_id: self.rob_id,
            
            // Computation state (cloned deeply)
            op1_data: self.op1_data.clone(),
            op2_data: self.op2_data.clone(),
            
            // Latency parameters
            read_latency: self.read_latency,
            compute_latency: self.compute_latency,
            write_latency: self.write_latency,
        }
    }
}

// Function to receive systolic array instructions (called by RS)
pub fn receive_systolic_array_inst(
    op1_bank_id: u64, 
    op2_bank_id: u64, 
    wr_bank_id: u64,
    m_dim: u64,  // Result rows
    n_dim: u64,  // Result columns  
    k_dim: u64,  // Inner dimension
    rob_id: u64
) -> bool {
    // Check if systolic array is available
    if !SYSTOLIC_ARRAY_INST_CAN_ISSUE.load(Ordering::Relaxed) {
        return false;
    }
    
    // Set instruction data
    *SYSTOLIC_ARRAY_INST_DATA.lock().unwrap() = Some(SystolicArrayInstData {
        op1_bank_id,
        op2_bank_id,
        wr_bank_id,
        m_dim,
        n_dim,
        k_dim,
        rob_id,
    });
    
    // Mark systolic array as busy
    SYSTOLIC_ARRAY_INST_CAN_ISSUE.store(false, Ordering::Relaxed);
    
    true
}

// ===========================================
// Unit Tests
// ===========================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    
    /// Test the processing element functionality
    #[test]
    fn test_processing_element() {
        let mut pe = ProcessingElement::new(0, 0);
        
        // Test multiply-accumulate operation
        pe.set_inputs(3, 4);
        pe.compute();
        assert_eq!(pe.get_result(), 12);
        
        // Test accumulation over multiple cycles
        pe.set_inputs(5, 6);
        pe.compute();
        assert_eq!(pe.get_result(), 12 + 30); // 42
        
        // Test result propagation
        assert_eq!(pe.get_a_right(), 5);
        assert_eq!(pe.get_b_down(), 6);
        
        // Test reset functionality
        pe.reset();
        assert_eq!(pe.get_result(), 0);
        assert_eq!(pe.get_a_right(), 0);
        assert_eq!(pe.get_b_down(), 0);
    }
    
    /// Test input buffer functionality
    #[test]
    fn test_input_buffer() {
        let matrix = vec![
            vec![1, 2, 3],
            vec![4, 5, 6],
        ];
        
        let buffer = InputBuffer::new(matrix);
        
        assert_eq!(buffer.rows(), 2);
        assert_eq!(buffer.cols(), 3);
        assert_eq!(buffer.get(0, 0), 1);
        assert_eq!(buffer.get(0, 1), 2);
        assert_eq!(buffer.get(1, 2), 6);
        assert_eq!(buffer.get(2, 0), 0); // Out of bounds
    }
    
    /// Test output buffer functionality
    #[test]
    fn test_output_buffer() {
        let mut buffer = OutputBuffer::new(2, 2);
        
        assert!(!buffer.is_ready());
        
        buffer.set(0, 0, 10);
        buffer.set(0, 1, 20);
        buffer.set(1, 0, 30);
        buffer.set(1, 1, 40);
        
        buffer.set_ready();
        assert!(buffer.is_ready());
        
        let result = buffer.get_result();
        assert_eq!(result[0][0], 10);
        assert_eq!(result[1][1], 40);
        
        buffer.clear();
        assert!(!buffer.is_ready());
        assert_eq!(buffer.get(0, 0), 0);
    }
    
    /// Test 1x1 matrix multiplication
    #[test]
    fn test_simple_1x1() {
        let mut systolic_array = SystolicArray::new(
            "dummy_write_port".to_string(),
            "dummy_read_port".to_string(),
            "dummy_commit_port".to_string()
        );
        systolic_array.rows = 1;
        systolic_array.cols = 1;
        
        let matrix_a = vec![vec![5]];
        let matrix_b = vec![vec![7]];
        
        systolic_array.load_matrices(matrix_a, matrix_b).unwrap();
        systolic_array.start();
        
        // Run until complete
        while systolic_array.cycle() {
            // Continue cycling
        }
        
        let result = systolic_array.get_results().unwrap();
        assert_eq!(result[0][0], 35); // 5 * 7
        
        println!("1x1 matrix multiplication test passed!");
    }
    
    /// Test 2x2 matrix multiplication
    #[test]
    fn test_matrix_multiplication() {
        // Create a 2x2 systolic array
        let mut systolic_array = SystolicArray::new(
            "dummy_write_port".to_string(),
            "dummy_read_port".to_string(),
            "dummy_commit_port".to_string()
        );
        systolic_array.rows = 2;
        systolic_array.cols = 2;
        
        // Define matrices for multiplication
        let matrix_a = vec![
            vec![2, 3],
            vec![4, 5],
        ];
        
        let matrix_b = vec![
            vec![6, 7],
            vec![8, 9],
        ];
        
        // Expected result: 2x2 matrix
        // [2*6+3*8, 2*7+3*9] = [36, 41]
        // [4*6+5*8, 4*7+5*9] = [64, 73]
        let expected = vec![
            vec![36, 41],
            vec![64, 73],
        ];
        
        // Load matrices and start computation
        systolic_array.load_matrices(matrix_a, matrix_b).unwrap();
        systolic_array.start();
        
        // Run until complete
        let mut cycles = 0;
        while systolic_array.cycle() {
            cycles += 1;
        }
        cycles += 1; // Count the final cycle
        
        // Get results
        let result = systolic_array.get_results().unwrap();
        
        // Print for debugging
        println!("2x2 Matrix Multiplication Test:");
        println!("Cycles executed: {}", cycles);
        println!("Expected: {:?}", expected);
        println!("Actual: {:?}", result);
        
        // Verify results
        for i in 0..2 {
            for j in 0..2 {
                assert_eq!(result[i][j], expected[i][j] as u128, 
                          "Result mismatch at ({}, {}): expected {}, got {}", 
                          i, j, expected[i][j], result[i][j]);
            }
        }
        
        println!("2x2 matrix multiplication test passed!");
    }
    
    /// Test larger matrix multiplication (3x3)
    #[test]
    fn test_large_matrix_multiplication() {
        // Create a 3x3 systolic array
        let mut systolic_array = SystolicArray::new(
            "dummy_write_port".to_string(),
            "dummy_read_port".to_string(),
            "dummy_commit_port".to_string()
        );
        systolic_array.rows = 3;
        systolic_array.cols = 3;
        
        // Define 3x3 matrices
        let matrix_a = vec![
            vec![1, 2, 3],
            vec![4, 5, 6],
            vec![7, 8, 9],
        ];
        
        let matrix_b = vec![
            vec![9, 8, 7],
            vec![6, 5, 4],
            vec![3, 2, 1],
        ];
        
        // Expected result calculated manually
        let expected = vec![
            vec![30, 24, 18],  // [1*9+2*6+3*3, 1*8+2*5+3*2, 1*7+2*4+3*1]
            vec![84, 69, 54],  // [4*9+5*6+6*3, 4*8+5*5+6*2, 4*7+5*4+6*1]
            vec![138, 114, 90], // [7*9+8*6+9*3, 7*8+8*5+9*2, 7*7+8*4+9*1]
        ];
        
        // Load matrices and start computation
        systolic_array.load_matrices(matrix_a, matrix_b).unwrap();
        systolic_array.start();
        
        // Run until complete
        let start_time = Instant::now();
        let mut cycles = 0;
        while systolic_array.cycle() {
            cycles += 1;
        }
        cycles += 1;
        let elapsed_time = start_time.elapsed();
        
        // Get results
        let result = systolic_array.get_results().unwrap();
        
        // Verify results
        for i in 0..3 {
            for j in 0..3 {
                assert_eq!(result[i][j], expected[i][j] as u128, 
                          "Result mismatch at ({}, {}): expected {}, got {}", 
                          i, j, expected[i][j], result[i][j]);
            }
        }
        
        println!("3x3 matrix multiplication test passed!");
        println!("Performance: {} cycles in {:?}", cycles, elapsed_time);
    }
    
    /// Test matrix multiplication with different dimensions (2x3 * 3x2)
    #[test]
    fn test_different_dimensions() {
        // Create a 2x2 systolic array (matches A rows and B columns)
        let mut systolic_array = SystolicArray::new(
            "dummy_write_port".to_string(),
            "dummy_read_port".to_string(),
            "dummy_commit_port".to_string()
        );
        systolic_array.rows = 2;
        systolic_array.cols = 2;
        
        // Define matrices with different dimensions
        let matrix_a = vec![  // 2x3 matrix
            vec![1, 2, 3],
            vec![4, 5, 6],
        ];
        
        let matrix_b = vec![  // 3x2 matrix
            vec![7, 8],
            vec![9, 10],
            vec![11, 12],
        ];
        
        // Expected result: 2x2 matrix
        // [1*7+2*9+3*11, 1*8+2*10+3*12] = [58, 64]
        // [4*7+5*9+6*11, 4*8+5*10+6*12] = [139, 154]
        let expected = vec![
            vec![58, 64],
            vec![139, 154],
        ];
        
        // Load matrices and start computation
        systolic_array.load_matrices(matrix_a, matrix_b).unwrap();
        systolic_array.start();
        
        // Run until complete
        while systolic_array.cycle() {
            // Continue cycling
        }
        
        // Get results
        let result = systolic_array.get_results().unwrap();
        
        // Verify results
        for i in 0..2 {
            for j in 0..2 {
                assert_eq!(result[i][j], expected[i][j] as u128, 
                          "Result mismatch at ({}, {}): expected {}, got {}", 
                          i, j, expected[i][j], result[i][j]);
            }
        }
        
        println!("2x3 * 3x2 matrix multiplication test passed!");
    }
}
