//===- config.rs - BEMU configuration parsing -----------------------------===//
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//===----------------------------------------------------------------------===//

use snafu::{whatever, ResultExt, Whatever};

#[derive(Debug, Clone)]
pub struct BemuConfig {
    pub log: Option<String>,
    pub isa: String,
    pub procs: usize,
    pub mem_mb: usize,
}

impl Default for BemuConfig {
    fn default() -> Self {
        Self {
            log: Some("bemu.log".to_string()),
            isa: "rv64gc".to_string(),
            procs: 1,
            mem_mb: 2048,
        }
    }
}

impl BemuConfig {
    pub fn parse(args: Vec<String>) -> Result<Self, Whatever> {
        let mut config = Self::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--log" => {
                    i += 1;
                    if i >= args.len() {
                        whatever!("--log requires an argument");
                    }
                    config.log = Some(args[i].clone());
                }
                "--isa" => {
                    i += 1;
                    if i >= args.len() {
                        whatever!("--isa requires an argument");
                    }
                    config.isa = args[i].clone();
                }
                "-p" | "--procs" => {
                    i += 1;
                    if i >= args.len() {
                        whatever!("--procs requires an argument");
                    }
                    config.procs = args[i].parse().whatever_context("invalid procs value")?;
                }
                "-m" | "--mem" => {
                    i += 1;
                    if i >= args.len() {
                        whatever!("--mem requires an argument");
                    }
                    config.mem_mb = args[i].parse().whatever_context("invalid mem value")?;
                }
                _ => {
                    whatever!("unknown argument: {}", args[i]);
                }
            }
            i += 1;
        }

        Ok(config)
    }
}
