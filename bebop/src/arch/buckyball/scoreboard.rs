use std::collections::HashMap;
use std::sync::Mutex;

// Pending request in scoreboard
#[derive(Debug, Clone)]
struct PendingRequest {
  rob_id: u64,
  pbank_id: u64,
  source: String,
  json_content: String,
}

// Pending read request in scoreboard
#[derive(Debug, Clone)]
struct PendingReadRequest {
  rob_id: u64,
  pbank_id: u64,
  start_addr: u64,
  count: u64,
  source: String, // "tdma" or "vecball"
}

// Scoreboard: track pending write requests per pbank
static SCOREBOARD: Mutex<Option<HashMap<u64, Vec<PendingRequest>>>> = Mutex::new(None);

// Scoreboard: track pending read requests per pbank
static READ_SCOREBOARD: Mutex<Option<HashMap<u64, Vec<PendingReadRequest>>>> = Mutex::new(None);

// Track in-flight requests (requests that have been sent to bank but not completed)
static IN_FLIGHT_REQUESTS: Mutex<Option<HashMap<u64, u64>>> = Mutex::new(None); // pbank_id -> rob_id

/// Initialize scoreboard
pub fn init_scoreboard() {
  *SCOREBOARD.lock().unwrap() = Some(HashMap::new());
  *READ_SCOREBOARD.lock().unwrap() = Some(HashMap::new());
  *IN_FLIGHT_REQUESTS.lock().unwrap() = Some(HashMap::new());
}

/// Check if a request has dependencies
/// Returns true if request can proceed, false if it should be blocked
/// Checks both in-flight requests and pending requests in scoreboard
pub fn check_dependency(pbank_id: u64, rob_id: u64) -> bool {
  let in_flight_opt = IN_FLIGHT_REQUESTS.lock().unwrap();
  let scoreboard_opt = SCOREBOARD.lock().unwrap();

  // Check in-flight requests
  if let Some(ref in_flight) = *in_flight_opt {
    if let Some(&pending_rob_id) = in_flight.get(&pbank_id) {
      if pending_rob_id < rob_id {
        // There's an in-flight request with smaller rob_id, this one must wait
        return false;
      }
    }
  }

  // Check pending write requests in scoreboard
  if let Some(ref scoreboard) = *scoreboard_opt {
    if let Some(pending_list) = scoreboard.get(&pbank_id) {
      // Check if there's any pending request with smaller rob_id
      for req in pending_list {
        if req.rob_id < rob_id {
          // There's a pending write request with smaller rob_id, this one must wait
          return false;
        }
      }
    }
  }

  true
}

/// Reserve a write request (called before the request is sent to mem_ctrl)
/// This allows read requests to detect dependencies even before write requests arrive
pub fn reserve_write_request(rob_id: u64, pbank_id: u64) {
  let mut scoreboard_opt = SCOREBOARD.lock().unwrap();
  if let Some(ref mut scoreboard) = *scoreboard_opt {
    // Add a placeholder request (with empty json_content, will be replaced when actual request arrives)
    scoreboard
      .entry(pbank_id)
      .or_insert_with(Vec::new)
      .push(PendingRequest {
        rob_id,
        pbank_id,
        source: "reserved".to_string(),
        json_content: String::new(),
      });
  }
}

/// Add a request to scoreboard (blocked due to dependency)
pub fn add_to_scoreboard(rob_id: u64, pbank_id: u64, source: String, json_content: String) {
  let mut scoreboard_opt = SCOREBOARD.lock().unwrap();
  if let Some(ref mut scoreboard) = *scoreboard_opt {
    // Check if there's already a reserved request for this rob_id and pbank_id
    let mut found_reserved = false;
    if let Some(pending_list) = scoreboard.get_mut(&pbank_id) {
      for req in pending_list.iter_mut() {
        if req.rob_id == rob_id && req.source == "reserved" {
          // Replace reserved request with actual request
          req.source = source.clone();
          req.json_content = json_content.clone();
          found_reserved = true;
          break;
        }
      }
    }

    if !found_reserved {
      // No reserved request found, add new one
      scoreboard
        .entry(pbank_id)
        .or_insert_with(Vec::new)
        .push(PendingRequest {
          rob_id,
          pbank_id,
          source,
          json_content,
        });
    }
  }
}

/// Mark a request as in-flight (sent to bank)
pub fn mark_in_flight(pbank_id: u64, rob_id: u64) {
  let mut in_flight_opt = IN_FLIGHT_REQUESTS.lock().unwrap();
  if let Some(ref mut in_flight) = *in_flight_opt {
    in_flight.insert(pbank_id, rob_id);
  }
}

/// Add a read request to scoreboard (blocked due to dependency)
pub fn add_read_to_scoreboard(rob_id: u64, pbank_id: u64, start_addr: u64, count: u64, source: String) {
  let mut read_scoreboard_opt = READ_SCOREBOARD.lock().unwrap();
  if let Some(ref mut read_scoreboard) = *read_scoreboard_opt {
    read_scoreboard
      .entry(pbank_id)
      .or_insert_with(Vec::new)
      .push(PendingReadRequest {
        rob_id,
        pbank_id,
        start_addr,
        count,
        source,
      });
  }
}

/// Get one ready read request from scoreboard (unified judgment each cycle)
/// Returns one (rob_id, pbank_id, start_addr, count, source) or None
pub fn get_one_ready_read_request() -> Option<(u64, u64, u64, u64, String)> {
  let mut read_scoreboard_opt = READ_SCOREBOARD.lock().unwrap();
  let in_flight_opt = IN_FLIGHT_REQUESTS.lock().unwrap();

  if let Some(ref mut read_scoreboard) = *read_scoreboard_opt {
    if let Some(ref in_flight) = *in_flight_opt {
      // Collect all candidates (one per pbank)
      let mut candidates: Vec<(u64, u64, u64, u64, String)> = Vec::new();

      for (pbank_id, pending_list) in read_scoreboard.iter_mut() {
        // Check if pbank is free
        let is_free = !in_flight.contains_key(pbank_id);

        if is_free && !pending_list.is_empty() {
          // Sort by rob_id to ensure order
          pending_list.sort_by_key(|r| r.rob_id);
          // Take the first request (smallest rob_id)
          let request = pending_list[0].clone();
          candidates.push((
            request.rob_id,
            request.pbank_id,
            request.start_addr,
            request.count,
            request.source,
          ));
        }
      }

      // Sort all candidates by rob_id globally, take the first one
      if !candidates.is_empty() {
        candidates.sort_by_key(|(rob_id, _, _, _, _)| *rob_id);
        let (rob_id, pbank_id, start_addr, count, source) = candidates.remove(0);

        // Remove from scoreboard
        if let Some(pending_list) = read_scoreboard.get_mut(&pbank_id) {
          pending_list.retain(|r| r.rob_id != rob_id);
          if pending_list.is_empty() {
            read_scoreboard.remove(&pbank_id);
          }
        }

        return Some((rob_id, pbank_id, start_addr, count, source));
      }
    }
  }

  None
}

/// Get ready read requests from scoreboard (deprecated, use get_one_ready_read_request instead)
/// Returns a list of (rob_id, pbank_id, start_addr, count, source)
pub fn get_ready_read_requests() -> Vec<(u64, u64, u64, u64, String)> {
  let mut ready = Vec::new();
  while let Some(req) = get_one_ready_read_request() {
    ready.push(req);
  }
  ready
}

/// Mark a request as completed (remove from in-flight)
pub fn mark_completed(pbank_id: u64) {
  let mut in_flight_opt = IN_FLIGHT_REQUESTS.lock().unwrap();
  if let Some(ref mut in_flight) = *in_flight_opt {
    in_flight.remove(&pbank_id);
  }

  // Check scoreboard for requests that can now proceed
  let mut scoreboard_opt = SCOREBOARD.lock().unwrap();
  if let Some(ref mut scoreboard) = *scoreboard_opt {
    if let Some(pending_list) = scoreboard.get_mut(&pbank_id) {
      // Sort by rob_id to process in order
      pending_list.sort_by_key(|r| r.rob_id);
    }
  }
}

/// Get one ready request from scoreboard (unified judgment each cycle)
/// Returns one (rob_id, pbank_id, source, json_content) or None
pub fn get_one_ready_request() -> Option<(u64, u64, String, String)> {
  let mut scoreboard_opt = SCOREBOARD.lock().unwrap();
  let in_flight_opt = IN_FLIGHT_REQUESTS.lock().unwrap();

  if let Some(ref mut scoreboard) = *scoreboard_opt {
    if let Some(ref in_flight) = *in_flight_opt {
      // Collect all ready requests (one per pbank)
      let mut candidates: Vec<(u64, u64, String, String)> = Vec::new();

      for (pbank_id, pending_list) in scoreboard.iter_mut() {
        // Check if pbank is free
        let is_free = !in_flight.contains_key(pbank_id);

        if is_free && !pending_list.is_empty() {
          // Sort by rob_id to ensure order
          pending_list.sort_by_key(|r| r.rob_id);
          // Take the first request (smallest rob_id)
          let request = pending_list[0].clone();
          candidates.push((request.rob_id, request.pbank_id, request.source, request.json_content));
        }
      }

      // Sort all candidates by rob_id globally, take the first one
      if !candidates.is_empty() {
        candidates.sort_by_key(|(rob_id, _, _, _)| *rob_id);
        let (rob_id, pbank_id, source, json_content) = candidates.remove(0);

        // Remove from scoreboard
        if let Some(pending_list) = scoreboard.get_mut(&pbank_id) {
          pending_list.retain(|r| r.rob_id != rob_id);
          if pending_list.is_empty() {
            scoreboard.remove(&pbank_id);
          }
        }

        return Some((rob_id, pbank_id, source, json_content));
      }
    }
  }

  None
}

/// Get ready requests from scoreboard (deprecated, use get_one_ready_request instead)
/// Returns a list of (rob_id, pbank_id, source, json_content)
pub fn get_ready_requests() -> Vec<(u64, u64, String, String)> {
  let mut ready = Vec::new();
  while let Some(req) = get_one_ready_request() {
    ready.push(req);
  }
  ready
}

/// Get number of pending requests in scoreboard
pub fn get_pending_count() -> usize {
  let scoreboard_opt = SCOREBOARD.lock().unwrap();
  if let Some(ref scoreboard) = *scoreboard_opt {
    scoreboard.values().map(|v| v.len()).sum()
  } else {
    0
  }
}

/// Get number of pending read requests in read scoreboard
pub fn get_pending_read_count() -> usize {
  let read_scoreboard_opt = READ_SCOREBOARD.lock().unwrap();
  if let Some(ref read_scoreboard) = *read_scoreboard_opt {
    read_scoreboard.values().map(|v| v.len()).sum()
  } else {
    0
  }
}

/// Get number of in-flight requests
pub fn get_in_flight_count() -> usize {
  let in_flight_opt = IN_FLIGHT_REQUESTS.lock().unwrap();
  if let Some(ref in_flight) = *in_flight_opt {
    in_flight.len()
  } else {
    0
  }
}

/// Check if all memory operations are complete
/// Returns true if there are no pending requests, no pending read requests, and no in-flight requests
pub fn is_all_memory_complete() -> bool {
  get_pending_count() == 0 && get_pending_read_count() == 0 && get_in_flight_count() == 0
}
