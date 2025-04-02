pub mod filelist;
pub mod filetree;
pub mod state;

pub use filelist::FileList;

use std::{path::PathBuf, process::Command, str::FromStr};

use state::State;

use crate::error::{Error, ErrorKind};

pub fn get_svn_status(path: &PathBuf) -> Result<String, Error> {
    let mut status_cmd = std::process::Command::new("svn");
    match status_cmd
        .args(["status", &path.to_str().expect("bad path")])
        .output()
    {
        Ok(output) => Ok(String::from_utf8(output.stdout).expect("svn gave bad status output")),
        Err(_) => todo!(),
    }
}

pub fn get_branch_name(path: &PathBuf) -> Result<String, Error> {
    let mut cmd = Command::new("svn");
    let output = cmd
        .args(["info", &path.to_str().expect("bad path")])
        .output()
        .map_err(|e| Error {
            kind: ErrorKind::SvnError,
            message: e.to_string(),
        })?;
    match output.status.success() {
        true => {
            let svn_info = String::from_utf8(output.stdout).expect("svn returned invalid utf-8!"); // TODO handle stderr?
            for line in svn_info.lines() {
                if line.starts_with("URL:") {
                    // find the branch name part
                    let path = PathBuf::from(&line[5..]);
                    let branch_name = path
                        .file_name()
                        .expect("branch url ended in ..")
                        .to_str()
                        .expect("branch url was invalid utf-8!")
                        .to_string();

                    return Ok(branch_name);
                }
            }
            Err(Error {
                kind: ErrorKind::SvnError,
                message: "couldn't find the URL line in svn info output".into(),
            })
        }
        false => Err(Error {
            kind: ErrorKind::SvnError,
            message: String::from_utf8(output.stderr).expect("svn error was invalid utf-8!"),
        }),
    }
}

pub type ParsedStatusLine = (State, PathBuf);

fn parse_status_line(status_line: &str) -> Result<ParsedStatusLine, ()> {
    let (status, path) = status_line.split_at(8);
    match State::from_str(status) {
        Ok(state) => {
            let path = PathBuf::from_str(path).expect("bad path");
            Ok((state, path))
        }
        Err(e) => Err(e),
    }
}

fn parse_svn_status(svn_status: &str) -> Result<Vec<ParsedStatusLine>, ()> {
    svn_status
        .lines()
        .filter(|line| svn_status_filter(line))
        .map(parse_status_line)
        .collect::<Result<Vec<ParsedStatusLine>, ()>>()
}

pub fn is_conflict_part(path: &str) -> bool {
    path.contains(".merge-") || path.contains(".working")
}

fn svn_status_filter(line: &str) -> bool {
    !(line.is_empty() || line.starts_with("Summary") || line.contains("onflicts:"))
}

fn create_empty_text_conflict(file: &PathBuf) -> Conflict {
    Conflict::Text {
        file: file.clone(),
        left: None,
        working: None,
        right: None,
    }
}

fn trim_conflict_suffix<'a>(path_str: &'a str) -> &'a str {
    let i_merge = path_str.find(".merge-");
    let i_working = path_str.find(".working");
    match (i_merge, i_working) {
        (None, Some(i)) | (Some(i), None) => &path_str[..i],
        _ => path_str,
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Conflict {
    Text {
        file: PathBuf,
        left: Option<PathBuf>,
        right: Option<PathBuf>,
        working: Option<PathBuf>,
    },
}

#[derive(PartialEq)]
enum ConflictPart {
    Left,
    Right,
    Working,
}

fn parse_conflict_part(path: &str) -> Option<ConflictPart> {
    if path.contains(".merge-left") {
        Some(ConflictPart::Left)
    } else if path.contains(".merge-right") {
        Some(ConflictPart::Right)
    } else if path.contains(".working") {
        Some(ConflictPart::Working)
    } else {
        None
    }
}
