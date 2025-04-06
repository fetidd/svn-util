use std::process::Command;

pub struct CmdResult(bool, String, String);

impl CmdResult {
    pub fn success(&self) -> bool {
        self.0
    }

    /// Returns the stdout if the command was succesful, else the stderr
    pub fn output(&self) -> &str {
        if self.success() { &self.1 } else { &self.2 }
    }
}

impl From<std::process::Output> for CmdResult {
    fn from(value: std::process::Output) -> Self {
        Self(
            value.status.success(),
            unsafe { String::from_utf8_unchecked(value.stdout) },
            unsafe { String::from_utf8_unchecked(value.stderr) },
        )
    }
}

// The below code allows run_command to be mocked based on the arguments passed to it
// TODO this could be good practice for a macro
#[cfg(not(test))]
pub fn run_command(cmd: &str, args: &[&str]) -> std::result::Result<CmdResult, std::io::Error> {
    let mut cmd = Command::new(cmd);
    Ok(cmd.args(args).output()?.into())
}

#[cfg(test)]
pub fn run_command(cmd: &str, args: &[&str]) -> std::result::Result<CmdResult, std::io::Error> {
    match (cmd, args) {
        ("svn", args) => match args {
            ["info", "output_missing_URL"] => Ok(CmdResult(true, "info".into(), "".into())),
            ["info", "branch_name"] => Ok(CmdResult(true, "URL: branch_name".into(), "".into())),
            ["info", "nested/branch_name"] => {
                Ok(CmdResult(true, "URL: nested/branch_name".into(), "".into()))
            }
            ["info", "something_bad_happened"] => {
                Ok(CmdResult(false, "".into(), "unknown issue with svn".into()))
            }
            _ => panic!("invalid case: {cmd} {args:?}"),
        },
        _ => panic!("not a valid case"),
    }
}
