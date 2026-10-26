use std::io::{self, Write};

use super::{RegressionResult, TestStatus};

fn xml(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            '\t' | '\n' | '\r' => c.to_string(),
            '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}' => c.to_string(),
            _ => "\u{fffd}".to_string(),
        })
        .collect()
}

pub fn write(
    output: &mut impl Write,
    suite: &str,
    results: &[(String, RegressionResult)],
    harness_failures: u64,
) -> io::Result<()> {
    let failures = results
        .iter()
        .filter(|(_, r)| matches!(r.status, TestStatus::Fail | TestStatus::Timeout))
        .count();
    let errors = results
        .iter()
        .filter(|(_, r)| matches!(r.status, TestStatus::Crash | TestStatus::InfraError))
        .count() as u64
        + harness_failures;
    let elapsed = results
        .iter()
        .map(|(_, r)| {
            let seconds = r
                .elapsed_sec
                .strip_suffix('s')
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "elapsed_sec must end in s"))?;
            let seconds: f64 = seconds
                .parse()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid elapsed_sec"));
            }
            Ok(seconds)
        })
        .collect::<io::Result<Vec<_>>>()?;
    writeln!(output, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
    writeln!(
        output,
        "<testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" time=\"{:.2}\">",
        xml(suite),
        results.len() as u64 + harness_failures,
        failures,
        errors,
        elapsed.iter().sum::<f64>()
    )?;
    for ((name, result), seconds) in results.iter().zip(elapsed) {
        writeln!(
            output,
            "  <testcase classname=\"{}\" name=\"{}\" time=\"{seconds:.2}\">",
            xml(&result.backend),
            xml(name)
        )?;
        if !result.success() {
            let tag = match result.status {
                TestStatus::Fail | TestStatus::Timeout => "failure",
                TestStatus::Crash | TestStatus::InfraError => "error",
                TestStatus::Pass => unreachable!(),
            };
            writeln!(
                output,
                "    <{tag} type=\"{}\" message=\"{}\">exit_code={:?}</{tag}>",
                result.status,
                xml(result.error_message.as_deref().unwrap_or_else(|| match result.status {
                    TestStatus::Fail => "Workload failed",
                    TestStatus::Timeout => "Workload timed out",
                    TestStatus::Crash => "Workload crashed",
                    TestStatus::InfraError => "Infrastructure error",
                    TestStatus::Pass => unreachable!(),
                })),
                result.exit_code
            )?;
        }
        // Keep large logs in artifacts; the report records their locations.
        writeln!(
            output,
            "    <system-out>elf={}\nartifacts={}\nstdout={}\nstderr={}</system-out>",
            xml(&result.elf_path.display().to_string()),
            xml(&result
                .artifact_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()),
            xml(&result
                .stdout_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()),
            xml(&result
                .stderr_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default())
        )?;
        writeln!(output, "  </testcase>")?;
    }
    for index in 0..harness_failures {
        writeln!(output, "  <testcase classname=\"harness\" name=\"unreported-failure-{index}\"><error message=\"Trial failed without a regression result; inspect the harness log\"/></testcase>")?;
    }
    writeln!(output, "</testsuite>")
}
