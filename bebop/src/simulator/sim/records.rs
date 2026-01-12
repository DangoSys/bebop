/// Macro to push a ModelRecord with common fields
///
/// Usage:
/// ```rust
/// model_record!(self, services, "action_name", "subject string");
/// model_record!(self, services, "action_name", format!("formatted {}", value));
/// ```
#[macro_export]
macro_rules! model_record {
  ($self:expr, $services:expr, $action:expr, $subject:expr) => {
    $self.records.push(sim::models::ModelRecord {
      time: $services.global_time(),
      action: $action.to_string(),
      subject: $subject.to_string(),
    });
  };
}
