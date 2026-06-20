use crate::cli::Cli;

#[derive(Debug)]
pub struct Form {
    pub fields: [String; 2],
    pub active: usize,
    pub msg: String,
}

impl Form {
    pub fn new(cli: &Cli) -> Self {
        Self {
            fields: [
                cli.log_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                "1".to_string(),
            ],
            active: 0,
            msg: "enter log dir, set harts, then press enter".to_string(),
        }
    }
}
