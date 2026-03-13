use tauri::command;

#[command]
fn hello_world() -> String {
    "Hello, world!".to_string()
}

#[command]
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![hello_world, greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
