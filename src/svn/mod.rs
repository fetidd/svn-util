pub mod error;
pub mod filelist;
pub mod filetree;
pub mod state;
use crate::command::CmdResult;

use super::command::run_command;
use state::State;
use std::{path::PathBuf, str::FromStr};
pub use {
    error::{Error, Result},
    filelist::FileList,
};

pub fn svn_revert(paths: &[&str]) -> Result<CmdResult> {
    let mut args = vec!["revert"];
    args.extend_from_slice(&paths);
    run_command("svn", &args).map_err(Error::from)
}

pub fn svn_delete(paths: &[&str]) -> Result<CmdResult> {
    let mut args = vec!["remove"];
    args.extend_from_slice(&paths);
    run_command("svn", &args).map_err(Error::from)
}

pub fn svn_add(paths: &[&str]) -> Result<CmdResult> {
    let mut args = vec!["add"];
    args.extend_from_slice(&paths);
    run_command("svn", &args).map_err(Error::from)
}

pub fn svn_commit(paths: &[&str]) -> Result<CmdResult> {
    let mut args = vec!["commit"];
    args.extend_from_slice(&paths);
    run_command("svn", &args).map_err(Error::from)
}

pub fn parse_branch_name(svn_info: &str) -> Result<String> {
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
    Err(Error::BranchParseFailure)
}

pub fn get_branch_name(path: &PathBuf) -> Result<String> {
    let res = run_command("svn", &["info", &path.to_string_lossy()])?;
    match res.success() {
        true => parse_branch_name(res.output()),
        false => Err(Error::from(res.output())),
    }
}

pub type ParsedStatusLine = (State, PathBuf);

pub fn get_svn_status(path: &PathBuf) -> Result<Vec<ParsedStatusLine>> {
    let res = run_command("svn", &["status", &path.to_string_lossy()])?;
    match res.success() {
        true => parse_svn_status(res.output()),
        false => Err(Error::from(res.output())),
    }
}

fn parse_status_line(status_line: &str) -> Result<ParsedStatusLine> {
    let (status, path) = status_line.split_at(8);
    match State::from_str(status) {
        Ok(state) => {
            let path = PathBuf::from_str(path).expect("bad path");
            Ok((state, path))
        }
        Err(_) => Err(Error::UnrecognisedStatus(status.into())),
    }
}

fn parse_svn_status(svn_status: &str) -> Result<Vec<ParsedStatusLine>> {
    svn_status
        .lines()
        .filter(|line| svn_status_filter(line))
        .map(parse_status_line)
        .collect::<Result<Vec<ParsedStatusLine>>>()
}

pub fn is_conflict_part(path: &str) -> bool {
    parse_conflict_part(path).is_some()
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

fn trim_conflict_suffix(path_str: &str) -> &str {
    let i_merge = path_str.find(".merge-");
    let i_working = path_str.find(".working");
    match (i_merge, i_working) {
        (None, Some(i)) | (Some(i), None) => &path_str[..i],
        _ => path_str, // if both parts are in the string then treat it as a weirdly named normal file
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

#[derive(PartialEq, Debug)]
enum ConflictPart {
    Left,
    Right,
    Working,
}

/// Parses a string representing a file path and returns what kind of conflict part
/// it is, or None if its not one. If the file path matches more than 1 conflict part
/// we treat it as if it's a weirdly named file.
fn parse_conflict_part(path: &str) -> Option<ConflictPart> {
    match (
        path.contains(".merge-left"),
        path.contains(".merge-right"),
        path.contains(".working"),
    ) {
        (true, false, false) => Some(ConflictPart::Left),
        (false, true, false) => Some(ConflictPart::Right),
        (false, false, true) => Some(ConflictPart::Working),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    #[rstest]
    #[case("branch_name", Ok("branch_name".into()))]
    #[case("nested/branch_name", Ok("branch_name".into()))]
    #[case("output_missing_URL", Err(Error::BranchParseFailure))]
    #[case(
        "something_bad_happened",
        Err(Error::Unknown("unknown issue with svn".into()))
    )]
    fn test_get_branch_name(#[case] path: &str, #[case] exp: Result<String>) {
        let actual = get_branch_name(&PathBuf::from(path));
        assert_eq!(exp, actual);
    }

    #[rstest]
    #[case("", Ok(vec![]))]
    #[case("M       path/to/file.txt", Ok(vec![(State::Modified, PathBuf::from("path/to/file.txt"))]))]
    #[case("M       path/to/file.txt\nR       path/to/replaced_file.txt", Ok(vec![
        (State::Modified, PathBuf::from("path/to/file.txt")),
        (State::Replaced, PathBuf::from("path/to/replaced_file.txt")),
    ]))]
    fn test_parse_svn_status(#[case] svn_status: &str, #[case] exp: Result<Vec<ParsedStatusLine>>) {
        assert_eq!(exp, parse_svn_status(svn_status));
    }

    #[rstest]
    #[case("M       path/to/file.txt", Ok((State::Modified, PathBuf::from("path/to/file.txt"))))]
    #[case("C       path/to/file.txt", Ok((State::Conflicting, PathBuf::from("path/to/file.txt"))))]
    #[case("R       path/to/file.txt", Ok((State::Replaced, PathBuf::from("path/to/file.txt"))))]
    #[case("D       path/to/file.txt", Ok((State::Deleted, PathBuf::from("path/to/file.txt"))))]
    #[case("!       path/to/file.txt", Ok((State::Missing, PathBuf::from("path/to/file.txt"))))]
    #[case("?       path/to/file.txt", Ok((State::Unversioned, PathBuf::from("path/to/file.txt"))))]
    #[case("A       path/to/file.txt", Ok((State::Added, PathBuf::from("path/to/file.txt"))))]
    #[case(" M      path/to/file.txt", Ok((State::Clean, PathBuf::from("path/to/file.txt"))))]
    fn test_parse_status_line(#[case] status_line: &str, #[case] exp: Result<ParsedStatusLine>) {
        assert_eq!(exp, parse_status_line(status_line));
    }

    #[rstest]
    #[case("", false)]
    #[case("Summary", false)]
    #[case("Summary of conflicts", false)]
    #[case("onflicts:", false)]
    #[case("Text conflicts:", false)]
    #[case("literally anything else", true)]
    fn test_svn_status_filter(#[case] line: &str, #[case] exp: bool) {
        assert_eq!(exp, svn_status_filter(line));
    }

    #[rstest]
    #[case("derpderp.txt", "derpderp.txt")]
    #[case("", "")]
    #[case("file.txt.merge-left.r7", "file.txt")]
    #[case("file.txt.merge-right.r7", "file.txt")]
    #[case("file.txt.working.r7", "file.txt")]
    #[case("file.txt.working.merge-left.r4", "file.txt.working.merge-left.r4")]
    fn test_trim_conflict_suffix(#[case] path: &str, #[case] exp: &str) {
        assert_eq!(exp, trim_conflict_suffix(path));
    }

    #[rstest]
    #[case(".merge-left", Some(ConflictPart::Left))]
    #[case(".merge-right", Some(ConflictPart::Right))]
    #[case(".working", Some(ConflictPart::Working))]
    #[case("herpderp.txt.merge-left.r2", Some(ConflictPart::Left))]
    #[case("herpderp.txt.merge-right.r5", Some(ConflictPart::Right))]
    #[case("herpderp.txt.working.r3", Some(ConflictPart::Working))]
    #[case("herpderp.txt.merge-left.merge-right", None)]
    #[case("herpderp.txt.merge-right.working", None)]
    #[case("not_a_conflict_part.txt", None)]
    fn test_parse_conflict_part(#[case] path: &str, #[case] exp: Option<ConflictPart>) {
        assert_eq!(exp, parse_conflict_part(path));
    }
}
