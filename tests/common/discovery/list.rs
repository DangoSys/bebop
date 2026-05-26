use std::io::Write;

use super::super::args::RegressionArgs;
use super::{discover_tests, ElfTestCase};

pub fn write_nextest_terse_list(
    args: &RegressionArgs,
    extension: Option<&str>,
    match_case: impl Fn(&ElfTestCase) -> bool,
    trial_name: impl Fn(&ElfTestCase) -> String,
) -> std::io::Result<()> {
    if args.ignored {
        return Ok(());
    }

    let test_cases = discover_tests(args, extension, match_case).map_err(|e| std::io::Error::other(e.to_string()))?;

    let mut stdout = std::io::stdout().lock();
    for tc in test_cases {
        writeln!(stdout, "{}: test", trial_name(&tc))?;
    }
    Ok(())
}
