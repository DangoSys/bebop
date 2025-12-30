use sim::models::{Model, Reportable};
use sim::simulator::Simulation;

pub fn print_simulation_records(simulation: &mut Simulation) {
  println!("\n--- Simulation Records ---");

  for model in simulation.models().iter() {
    print_model_records(model, 0);
  }

  println!("--- End Records ---\n");
}

fn print_model_records(model: &Model, indent: usize) {
  let records = model.records();
  let indent_str = "  ".repeat(indent);

  if !records.is_empty() {
    println!("\n{}[{}]", indent_str, model.id());
    for record in records {
      println!("{}  Time {:.1}: {}", indent_str, record.time, record.action);
    }
  }
  // 这里只能打印顶层模型的 records
}
