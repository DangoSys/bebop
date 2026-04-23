//! Strict env parsing for production paths (no implicit defaults).

pub fn must_nonempty(name: &str) -> Result<String, String> {
    let v = std::env::var(name).map_err(|_| format!("{name} is not set"))?;
    if v.is_empty() {
        return Err(format!("{name} is empty"));
    }
    Ok(v)
}
